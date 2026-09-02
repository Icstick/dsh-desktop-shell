//! Minimal scheduler / TimerHost for IF-SCHEDULE-WAKE (ADR-0019
//! decision 6, M6-D).
//!
//! Wake contract: specs/protocol/schedule-wake-capability.schema.json
//! (method "wake"; the M6-D "repeat" extension adds periodic wakes).
//! The daemon exposes it as the "scheduler" capability
//! (scheduler.dsh-desktop.local/v1alpha1, kind Scheduler) with two
//! envelope methods:
//!
//! - Scheduler::register — "wake": schedule a wake that fires at
//!   "deadline" (or "requestedAt" when no deadline is given), optionally
//!   repeating every repeat.intervalMs up to repeat.count times.
//! - Scheduler::cancel — "cancel" (M6 convenience operation; the wake
//!   schema itself only defines "wake", so cancel's wire shape is
//!   documented here: {"wakeId": string}).
//!
//! Fired wakes execute a daemon-internal action: record the fire in
//! SchedulerStats (fire counter + lastFired), surfaced through
//! "daemon.status" under "scheduler" — the minimal M6 semantics. Full
//! scheduling policy (persistence, priorities, external actions) is M7.
//!
//! Threading: one worker thread owns the entry list; registrations and
//! cancellations mutate it under a mutex and nudge the worker through a
//! condvar, so a short-deadline wake is picked up immediately instead of
//! waiting for the idle poll.
//!
//! Validation mirrors the schema fail-closed: unknown fields are rejected
//! (deny_unknown_fields), wakeId is length-bounded, timestamps must be
//! RFC 3339, reason is an enum, and repeat.intervalMs is bounded
//! (sub-50 ms timers would busy-loop the worker; the cap keeps one request
//! from abusing the timer thread).

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::envelope::{now_timestamp, now_timestamp_like};

/// Capability apiVersion of the scheduler (IF-SCHEDULE-WAKE
/// api_version: scheduler.dsh-desktop.local/v1alpha1).
pub const SCHEDULER_API_VERSION: &str = "scheduler.dsh-desktop.local/v1alpha1";
/// Capability kind of the scheduler.
pub const SCHEDULER_KIND: &str = "Scheduler";
/// Envelope method that registers a wake (schema method const).
pub const SCHEDULER_WAKE_METHOD: &str = "wake";
/// Envelope method that cancels a pending wake (daemon-side extension).
pub const SCHEDULER_CANCEL_METHOD: &str = "cancel";

/// wakeId bounds (schema minLength/maxLength).
pub const WAKE_ID_MIN_LEN: usize = 8;
pub const WAKE_ID_MAX_LEN: usize = 128;

/// repeat.intervalMs bounds (schema minimum/maximum).
pub const MIN_INTERVAL_MS: u64 = 50;
pub const MAX_INTERVAL_MS: u64 = 86_400_000;

/// Idle poll when no wake is scheduled (the worker must observe
/// "shutdown" and late registrations without sleeping forever).
const IDLE_POLL: Duration = Duration::from_millis(50);

/// Reason a wake was requested (schema params.reason enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeReason {
    ScheduledDue,
    RecoveryRetry,
    UserRequested,
}

/// Periodic repeat spec (schema params.repeat, M6-D extension).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WakeRepeat {
    /// Fire every intervalMs after the first fire.
    pub interval_ms: u64,
    /// Total number of fires; omitted = repeat forever.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

/// Wire shape of a wake registration (schema params).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WakeRequestPayload {
    pub wake_id: String,
    pub requested_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    pub reason: WakeReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<WakeRepeat>,
}

/// Wire shape of a cancellation (scheduler.cancel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CancelRequestPayload {
    pub wake_id: String,
}

/// One scheduled wake.
#[derive(Debug, Clone)]
struct Entry {
    seq: u64,
    wake_id: String,
    fire_at: Instant,
    reason: WakeReason,
    requested_at: String,
    deadline: Option<String>,
    repeat: Option<WakeRepeat>,
    /// Remaining fires for a bounded repeat (None = unlimited).
    fires_left: Option<u64>,
}

/// Shared scheduler state (worker thread + API threads).
#[derive(Debug, Default)]
struct Inner {
    entries: Vec<Entry>,
    next_seq: u64,
    shutdown: bool,
    registered: u64,
    cancelled: u64,
    fired: u64,
    last_fired: Option<FiredWake>,
}

/// Scheduler counters surfaced through daemon.status.scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerStats {
    /// Accepted wake registrations (lifetime).
    pub registered: u64,
    /// Cancellations that removed at least one pending wake.
    pub cancelled: u64,
    /// Fired wakes (periodic repeats count individually).
    pub fired: u64,
    /// Wakes currently scheduled (including pending periodic repeats).
    pub pending: usize,
    /// The most recent fired wake, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fired: Option<FiredWake>,
}

/// Record of one fired wake (daemon-internal action result).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiredWake {
    pub wake_id: String,
    pub reason: WakeReason,
    pub requested_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    pub fired_at: String,
}

/// Why a scheduler operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerError {
    /// Params do not match the wake contract (mapped to MALFORMED_MESSAGE).
    InvalidPayload(String),
    /// The wakeId is already scheduled (mapped to CONFLICT).
    Conflict(String),
}

/// The TimerHost: owns the entry list and the fire worker thread.
pub struct Scheduler {
    inner: Arc<Mutex<Inner>>,
    wake: Arc<Condvar>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl Scheduler {
    /// Create a scheduler and start its worker thread.
    pub fn new() -> Self {
        let inner = Arc::new(Mutex::new(Inner::default()));
        let wake = Arc::new(Condvar::new());
        let worker = {
            let inner = Arc::clone(&inner);
            let wake = Arc::clone(&wake);
            thread::spawn(move || worker_loop(inner, wake))
        };
        Self {
            inner,
            wake,
            worker: Mutex::new(Some(worker)),
        }
    }

    /// Register a wake (envelope method "wake"). The request must match
    /// the schedule-wake schema; returns the accepted registration with
    /// the computed fire time.
    pub fn register(
        &self,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, SchedulerError> {
        let request: WakeRequestPayload = serde_json::from_value(payload.clone()).map_err(|e| {
            SchedulerError::InvalidPayload(format!(
                "wake params do not match the schedule-wake contract: {e}"
            ))
        })?;
        validate_wake_request(&request)?;
        let fire_time = fire_time(&request)?;
        let fire_at = Instant::now()
            + fire_time
                .duration_since(SystemTime::now())
                .unwrap_or_default();

        let mut inner = self.inner.lock().expect("scheduler lock poisoned");
        if inner.entries.iter().any(|e| e.wake_id == request.wake_id) {
            return Err(SchedulerError::Conflict(format!(
                "wakeId \"{}\" is already scheduled",
                request.wake_id
            )));
        }
        let entry = Entry {
            seq: inner.next_seq,
            wake_id: request.wake_id.clone(),
            fire_at,
            reason: request.reason,
            requested_at: request.requested_at.clone(),
            deadline: request.deadline.clone(),
            repeat: request.repeat.clone(),
            fires_left: request.repeat.as_ref().and_then(|r| r.count),
        };
        inner.next_seq += 1;
        inner.entries.push(entry);
        inner.registered += 1;
        let pending = inner.entries.len();
        drop(inner);
        self.wake.notify_all();

        Ok(serde_json::json!({
            "wakeId": request.wake_id,
            "scheduledFor": now_timestamp_like(fire_time),
            "repeat": request.repeat,
            "pending": pending,
        }))
    }

    /// Cancel a pending wake (envelope method "cancel"). Cancelling an
    /// unknown id is an idempotent success (cancelled: false).
    pub fn cancel(&self, payload: &serde_json::Value) -> Result<serde_json::Value, SchedulerError> {
        let request: CancelRequestPayload = serde_json::from_value(payload.clone())
            .map_err(|e| SchedulerError::InvalidPayload(format!("cancel params invalid: {e}")))?;
        if !(WAKE_ID_MIN_LEN..=WAKE_ID_MAX_LEN).contains(&request.wake_id.len()) {
            return Err(SchedulerError::InvalidPayload(format!(
                "wakeId length {} outside {WAKE_ID_MIN_LEN}..={WAKE_ID_MAX_LEN}",
                request.wake_id.len()
            )));
        }

        let mut inner = self.inner.lock().expect("scheduler lock poisoned");
        let before = inner.entries.len();
        inner.entries.retain(|e| e.wake_id != request.wake_id);
        let removed = before - inner.entries.len();
        if removed > 0 {
            inner.cancelled += 1;
        }
        let pending = inner.entries.len();
        drop(inner);
        self.wake.notify_all();

        Ok(serde_json::json!({
            "wakeId": request.wake_id,
            "cancelled": removed > 0,
            "pending": pending,
        }))
    }

    /// Current scheduler counters (for daemon.status).
    pub fn stats(&self) -> SchedulerStats {
        let inner = self.inner.lock().expect("scheduler lock poisoned");
        SchedulerStats {
            registered: inner.registered,
            cancelled: inner.cancelled,
            fired: inner.fired,
            pending: inner.entries.len(),
            last_fired: inner.last_fired.clone(),
        }
    }

    /// Stop the worker thread (idempotent; also called from Drop).
    pub fn shutdown(&self) {
        {
            let mut inner = self.inner.lock().expect("scheduler lock poisoned");
            inner.shutdown = true;
        }
        self.wake.notify_all();
        if let Some(handle) = self.worker.lock().expect("worker lock poisoned").take() {
            let _ = handle.join();
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Validate a wake request against the schema (bounds that serde cannot
/// express).
fn validate_wake_request(request: &WakeRequestPayload) -> Result<(), SchedulerError> {
    if !(WAKE_ID_MIN_LEN..=WAKE_ID_MAX_LEN).contains(&request.wake_id.len()) {
        return Err(SchedulerError::InvalidPayload(format!(
            "wakeId length {} outside {WAKE_ID_MIN_LEN}..={WAKE_ID_MAX_LEN}",
            request.wake_id.len()
        )));
    }
    if let Some(repeat) = &request.repeat {
        if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&repeat.interval_ms) {
            return Err(SchedulerError::InvalidPayload(format!(
                "repeat.intervalMs {} outside {MIN_INTERVAL_MS}..={MAX_INTERVAL_MS}",
                repeat.interval_ms
            )));
        }
        if repeat.count == Some(0) {
            return Err(SchedulerError::InvalidPayload(
                "repeat.count must be >= 1".to_string(),
            ));
        }
    }
    Ok(())
}

/// The fire time: "deadline" when given, else "requestedAt" (the schema
/// makes both RFC 3339 date-times; both are validated).
fn fire_time(request: &WakeRequestPayload) -> Result<SystemTime, SchedulerError> {
    let requested = parse_rfc3339(&request.requested_at).ok_or_else(|| {
        SchedulerError::InvalidPayload(format!(
            "requestedAt \"{}\" is not a valid RFC 3339 timestamp",
            request.requested_at
        ))
    })?;
    match &request.deadline {
        Some(deadline) => parse_rfc3339(deadline).ok_or_else(|| {
            SchedulerError::InvalidPayload(format!(
                "deadline \"{deadline}\" is not a valid RFC 3339 timestamp"
            ))
        }),
        None => Ok(requested),
    }
}

/// Worker: fire due entries, then sleep until the next deadline (or the
/// idle poll). wait_timeout never returns early, so a due entry is
/// picked up on schedule; spurious wakeups are handled by re-checking.
fn worker_loop(inner: Arc<Mutex<Inner>>, wake: Arc<Condvar>) {
    let mut guard = inner.lock().expect("scheduler lock poisoned");
    loop {
        let now = Instant::now();
        // Fire every due entry (entries are removed one by one).
        while let Some(index) = guard.entries.iter().position(|e| e.fire_at <= now) {
            let entry = guard.entries.remove(index);
            fire(&mut guard, entry);
        }
        if guard.shutdown {
            return;
        }
        let next = guard.entries.iter().map(|e| e.fire_at).min();
        let timeout = match next {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()),
            None => IDLE_POLL,
        };
        let (relocked, _) = wake
            .wait_timeout(guard, timeout)
            .expect("scheduler condvar not poisoned");
        guard = relocked;
    }
}

/// Daemon-internal fire action: record the fire, then reschedule
/// periodic wakes (or drop completed/one-shot ones).
fn fire(inner: &mut Inner, entry: Entry) {
    inner.fired += 1;
    inner.last_fired = Some(FiredWake {
        wake_id: entry.wake_id.clone(),
        reason: entry.reason,
        requested_at: entry.requested_at.clone(),
        deadline: entry.deadline.clone(),
        fired_at: now_timestamp(),
    });

    let Some(repeat) = entry.repeat else {
        return; // one-shot: done
    };
    let fires_left = entry.fires_left.map(|left| left - 1);
    if fires_left == Some(0) {
        return; // bounded repeat completed
    }
    inner.entries.push(Entry {
        seq: entry.seq,
        wake_id: entry.wake_id,
        fire_at: Instant::now() + Duration::from_millis(repeat.interval_ms),
        reason: entry.reason,
        requested_at: entry.requested_at,
        deadline: entry.deadline,
        repeat: Some(repeat),
        fires_left,
    });
}

/// Minimal RFC 3339 parser (no external crates): accepts
/// YYYY-MM-DDTHH:MM:SS[.fff](Z|+-HH:MM) with optional fraction digits.
/// Pre-1970 timestamps are rejected (out of the daemon's domain).
fn parse_rfc3339(input: &str) -> Option<SystemTime> {
    let mut cursor = Cursor {
        bytes: input.as_bytes(),
        pos: 0,
    };
    let year = i64::from(cursor.digits(4)?);
    cursor.eat(b'-')?;
    let month = cursor.digits(2)?;
    cursor.eat(b'-')?;
    let day = cursor.digits(2)?;
    cursor.eat(b'T')?;
    let hour = cursor.digits(2)?;
    cursor.eat(b':')?;
    let minute = cursor.digits(2)?;
    cursor.eat(b':')?;
    let second = cursor.digits(2)?;

    let mut millis = 0u64;
    if cursor.peek() == Some(b'.') {
        cursor.pos += 1;
        let mut scale = 100u64;
        let mut fraction_digits = 0;
        while fraction_digits < 3 {
            let Some(digit) = cursor.peek().and_then(|b| b.checked_sub(b'0')) else {
                break;
            };
            if digit > 9 {
                break;
            }
            millis += u64::from(digit) * scale;
            scale /= 10;
            cursor.pos += 1;
            fraction_digits += 1;
        }
        while cursor.peek().is_some_and(|b| b.is_ascii_digit()) {
            cursor.pos += 1;
        }
    }

    let mut offset_secs: i64 = 0;
    match cursor.peek()? {
        b'Z' | b'z' => cursor.pos += 1,
        b'+' | b'-' => {
            let sign = if cursor.peek()? == b'-' { -1 } else { 1 };
            cursor.pos += 1;
            let offset_hour = cursor.digits(2)?;
            cursor.eat(b':')?;
            let offset_minute = cursor.digits(2)?;
            if offset_hour > 23 || offset_minute > 59 {
                return None;
            }
            offset_secs = sign * (i64::from(offset_hour) * 3600 + i64::from(offset_minute) * 60);
        }
        _ => return None,
    }
    if !cursor.done() {
        return None;
    }
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let days = days_from_civil(year, i64::from(month), i64::from(day));
    let unix_secs =
        days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second)
            - offset_secs;
    let millis_total = unix_secs.checked_mul(1000)?.checked_add(millis as i64)?;
    u64::try_from(millis_total)
        .ok()
        .map(|ms| UNIX_EPOCH + Duration::from_millis(ms))
}

/// Byte cursor for the RFC 3339 parser.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn eat(&mut self, expected: u8) -> Option<()> {
        if self.peek()? == expected {
            self.pos += 1;
            Some(())
        } else {
            None
        }
    }

    fn digits(&mut self, count: usize) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            let byte = self.peek()?;
            let digit = byte.checked_sub(b'0')?;
            if digit > 9 {
                return None;
            }
            value = value.checked_mul(10)?.checked_add(u32::from(digit))?;
            self.pos += 1;
        }
        Some(value)
    }

    fn done(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

/// Days since 1970-01-01 (Howard Hinnant's civil algorithm; inverse of
/// envelope::civil_from_days).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_until(mut predicate: impl FnMut() -> bool, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        predicate()
    }

    fn valid_wake(wake_id: &str) -> serde_json::Value {
        // Relative timestamp: a fixed date goes stale and the worker then
        // fires immediately, removing the entry before a duplicate check
        // (observed as duplicate_wake_id_is_conflict failing once the
        // fixed date passed).
        let requested = SystemTime::now() + Duration::from_secs(3600);
        serde_json::json!({
            "wakeId": wake_id,
            "requestedAt": now_timestamp_like(requested),
            "reason": "scheduled_due",
        })
    }

    // --- payload validation ---

    #[test]
    fn short_wake_id_is_rejected() {
        let scheduler = Scheduler::new();
        let error = scheduler
            .register(&valid_wake("short"))
            .expect_err("wakeId below minLength");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));
    }

    #[test]
    fn long_wake_id_is_rejected() {
        let scheduler = Scheduler::new();
        let error = scheduler
            .register(&valid_wake(&"x".repeat(WAKE_ID_MAX_LEN + 1)))
            .expect_err("wakeId above maxLength");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));
    }

    #[test]
    fn unknown_reason_is_rejected() {
        let scheduler = Scheduler::new();
        let mut payload = valid_wake("w-00000001");
        payload["reason"] = serde_json::json!("nonsense");
        let error = scheduler
            .register(&payload)
            .expect_err("unknown reason enum value");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));
    }

    #[test]
    fn malformed_timestamps_are_rejected() {
        let scheduler = Scheduler::new();
        let mut payload = valid_wake("w-00000002");
        payload["requestedAt"] = serde_json::json!("not-a-date");
        let error = scheduler
            .register(&payload)
            .expect_err("requestedAt must be RFC 3339");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));

        let mut payload = valid_wake("w-00000003");
        payload["deadline"] = serde_json::json!("2026-08-31");
        let error = scheduler
            .register(&payload)
            .expect_err("deadline must be a full RFC 3339 timestamp");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));
    }

    #[test]
    fn unknown_payload_fields_are_rejected() {
        let scheduler = Scheduler::new();
        let mut payload = valid_wake("w-00000004");
        payload["sneaky"] = serde_json::json!(1);
        let error = scheduler
            .register(&payload)
            .expect_err("additionalProperties false");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));
    }

    #[test]
    fn repeat_bounds_are_validated() {
        let scheduler = Scheduler::new();
        let mut payload = valid_wake("w-00000005");
        payload["repeat"] = serde_json::json!({ "intervalMs": 10 });
        let error = scheduler
            .register(&payload)
            .expect_err("intervalMs below minimum");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));

        let mut payload = valid_wake("w-00000006");
        payload["repeat"] = serde_json::json!({ "intervalMs": 60, "count": 0 });
        let error = scheduler
            .register(&payload)
            .expect_err("count must be >= 1");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));
    }

    #[test]
    fn duplicate_wake_id_is_conflict() {
        let scheduler = Scheduler::new();
        scheduler
            .register(&valid_wake("w-00000007"))
            .expect("first registration");
        let error = scheduler
            .register(&valid_wake("w-00000007"))
            .expect_err("duplicate wakeId");
        assert!(matches!(error, SchedulerError::Conflict(_)));
    }

    // --- firing behaviour ---

    #[test]
    fn one_shot_wake_fires_after_delay() {
        let scheduler = Scheduler::new();
        let mut payload = valid_wake("w-one-shot-01");
        let fire_time = SystemTime::now() + Duration::from_millis(100);
        payload["deadline"] = serde_json::json!(now_timestamp_like(fire_time));

        let response = scheduler.register(&payload).expect("register");
        assert_eq!(response["wakeId"], "w-one-shot-01");
        assert_eq!(response["pending"], 1);
        let scheduled_for = response["scheduledFor"].as_str().expect("scheduledFor");
        assert!(scheduled_for.contains('T'), "RFC 3339: {scheduled_for}");

        assert!(
            wait_until(|| scheduler.stats().fired == 1, Duration::from_secs(3)),
            "wake must fire after ~100 ms"
        );
        let stats = scheduler.stats();
        assert_eq!(stats.pending, 0);
        let last = stats.last_fired.expect("lastFired recorded");
        assert_eq!(last.wake_id, "w-one-shot-01");
        assert_eq!(last.reason, WakeReason::ScheduledDue);
    }

    #[test]
    fn wake_without_deadline_fires_at_requested_at() {
        let scheduler = Scheduler::new();
        let payload = serde_json::json!({
            "wakeId": "w-requested-01",
            "requestedAt": now_timestamp_like(SystemTime::now() + Duration::from_millis(80)),
            "reason": "user_requested",
        });
        scheduler.register(&payload).expect("register");
        assert!(
            wait_until(|| scheduler.stats().fired == 1, Duration::from_secs(3)),
            "wake without deadline fires at requestedAt"
        );
        assert_eq!(
            scheduler.stats().last_fired.unwrap().reason,
            WakeReason::UserRequested
        );
    }

    #[test]
    fn periodic_wake_fires_count_times_then_stops() {
        let scheduler = Scheduler::new();
        let mut payload = valid_wake("w-periodic-01");
        payload["deadline"] = serde_json::json!(now_timestamp_like(
            SystemTime::now() + Duration::from_millis(60)
        ));
        payload["repeat"] = serde_json::json!({ "intervalMs": 60, "count": 3 });

        let response = scheduler.register(&payload).expect("register");
        assert_eq!(response["repeat"]["intervalMs"], 60);
        assert_eq!(response["repeat"]["count"], 3);

        assert!(
            wait_until(|| scheduler.stats().fired >= 3, Duration::from_secs(3)),
            "periodic wake must fire 3 times"
        );
        let stats = scheduler.stats();
        assert_eq!(stats.fired, 3);
        assert_eq!(stats.pending, 0, "bounded repeat completes and is removed");
        assert_eq!(stats.registered, 1);
    }

    #[test]
    fn unlimited_periodic_wake_keeps_firing_until_cancelled() {
        let scheduler = Scheduler::new();
        let mut payload = valid_wake("w-periodic-02");
        // No deadline: the wake fires at requestedAt; make it imminent so
        // the test is clock-independent (valid_wake's fixed timestamp may
        // be in the future on this machine).
        payload["requestedAt"] = serde_json::json!(now_timestamp_like(
            SystemTime::now() + Duration::from_millis(50)
        ));
        payload["repeat"] = serde_json::json!({ "intervalMs": 50 });

        scheduler.register(&payload).expect("register");
        assert!(
            wait_until(|| scheduler.stats().fired >= 3, Duration::from_secs(3)),
            "unlimited periodic wake keeps firing"
        );
        let cancel = scheduler
            .cancel(&serde_json::json!({ "wakeId": "w-periodic-02" }))
            .expect("cancel");
        assert_eq!(cancel["cancelled"], true);
        let fired_at_cancel = scheduler.stats().fired;
        thread::sleep(Duration::from_millis(150));
        assert_eq!(
            scheduler.stats().fired,
            fired_at_cancel,
            "no fires after cancel"
        );
        assert_eq!(scheduler.stats().pending, 0);
        assert_eq!(scheduler.stats().cancelled, 1);
    }

    #[test]
    fn cancel_prevents_a_pending_fire() {
        let scheduler = Scheduler::new();
        let mut payload = valid_wake("w-cancel-001");
        payload["deadline"] = serde_json::json!(now_timestamp_like(
            SystemTime::now() + Duration::from_millis(200)
        ));

        scheduler.register(&payload).expect("register");
        let cancel = scheduler
            .cancel(&serde_json::json!({ "wakeId": "w-cancel-001" }))
            .expect("cancel");
        assert_eq!(cancel["cancelled"], true);
        assert_eq!(cancel["pending"], 0);

        thread::sleep(Duration::from_millis(300));
        let stats = scheduler.stats();
        assert_eq!(stats.fired, 0, "cancelled wake must not fire");
        assert_eq!(stats.cancelled, 1);
    }

    #[test]
    fn cancel_unknown_wake_is_idempotent() {
        let scheduler = Scheduler::new();
        let cancel = scheduler
            .cancel(&serde_json::json!({ "wakeId": "w-ghost-0001" }))
            .expect("cancel");
        assert_eq!(cancel["cancelled"], false);
        assert_eq!(cancel["pending"], 0);
    }

    #[test]
    fn cancel_with_invalid_shape_is_rejected() {
        let scheduler = Scheduler::new();
        let error = scheduler
            .cancel(&serde_json::json!({ "wakeId": "short" }))
            .expect_err("cancel wakeId below minLength");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));
        let error = scheduler
            .cancel(&serde_json::json!({ "nope": 1 }))
            .expect_err("unknown cancel field");
        assert!(matches!(error, SchedulerError::InvalidPayload(_)));
    }

    // --- RFC 3339 parser ---

    #[test]
    fn rfc3339_parser_accepts_zulu_and_offsets() {
        let zulu = parse_rfc3339("2026-08-31T09:30:00.123Z").expect("zulu");
        let offset = parse_rfc3339("2026-08-31T09:30:00.123+00:00").expect("offset");
        assert_eq!(zulu, offset);
        let shifted = parse_rfc3339("2026-08-31T17:30:00.123+08:00").expect("shifted");
        assert_eq!(zulu, shifted, "UTC-8 == +08:00 wall time");
        let no_millis = parse_rfc3339("2026-08-31T09:30:00Z").expect("no millis");
        assert_eq!(zulu.duration_since(no_millis).unwrap().as_millis(), 123);
    }

    #[test]
    fn rfc3339_parser_rejects_garbage() {
        for bad in [
            "2026-08-31",
            "09:30:00Z",
            "2026-08-31T09:30:00",
            "2026-13-01T00:00:00Z",
            "2026-08-31T24:00:00Z",
            "2026-08-31T09:30:00+8:00",
            "2026-08-31T09:30:00.123Zextra",
            "not-a-date",
        ] {
            assert!(parse_rfc3339(bad).is_none(), "must reject: {bad}");
        }
    }
}
