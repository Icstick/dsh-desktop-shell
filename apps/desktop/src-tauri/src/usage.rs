//! Usage collector (M3-C, IF-USAGE, ADR-0016).
//!
//! Local-first usage ledger for Desktop-owned events (Managed runtime
//! sessions, terminal sessions, notification deliveries):
//! - `record` validates the frozen usage-record schema semantics (source
//!   1-64 chars, period start <= end, non-negative tokens, currency
//!   `^[A-Z]{3}$`, isEstimate always present) and appends to AppData
//!   `usage-records-v1.jsonl` (append-only, rolling-capped at 4096);
//! - `snapshot` aggregates the ledger newest-first with totals, matching
//!   usage-snapshot.schema.json;
//! - the terminal / notification / Managed runtime wiring lives in the
//!   command layer and feeds this collector. Records carry only source,
//!   period and token estimates — never terminal output or notification
//!   content (AC-USG-001);
//! - records are persisted locally and are never sent over the network
//!   (AC-USG-002). There is no network code in this module.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const SCHEMA_VERSION: u8 = 1;
/// Rolling cap for the append-only usage ledger; the snapshot schema bounds
/// records at 4096, so the ledger never outgrows what a snapshot can carry.
const RECORDS_CAP: usize = 4096;
const RECORDS_FILE_NAME: &str = "usage-records-v1.jsonl";
const MAX_SOURCE_CHARS: usize = 64;
/// Sources recorded by the Desktop-owned wiring (commands.rs).
pub(crate) const SOURCE_TERMINAL: &str = "terminal";
pub(crate) const SOURCE_NOTIFICATION: &str = "notification";
pub(crate) const SOURCE_RUNTIME: &str = "runtime";
/// Managed states that close an open runtime session and record it.
const RUNTIME_END_STATES: [&str; 3] = ["stopped", "crashed", "safe_stop"];

/// Mirrors specs/usage/usage-record.schema.json. A record carries no
/// content-bearing fields (AC-USG-001); `period` is stored as RFC 3339
/// date-time strings exactly like the schema requires.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UsageRecord {
    schema_version: u8,
    source: String,
    period: UsagePeriod,
    input_tokens: u64,
    output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_read_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    is_estimate: bool,
    recorded_at_unix_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UsagePeriod {
    start: String,
    end: String,
}

/// Mirrors specs/usage/usage-snapshot.schema.json.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    schema_version: u8,
    generated_at_unix_ms: u64,
    records: Vec<UsageRecord>,
    totals: UsageTotals,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct UsageTotals {
    input_tokens: u64,
    output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<f64>,
    /// `null` when a cost exists but no currency is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    estimate_count: u64,
}

/// Mirrors specs/usage/usage-snapshot-request.schema.json.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageSnapshotRequest {
    schema_version: u8,
    since_unix_ms: Option<u64>,
}

impl UsageSnapshotRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn since_unix_ms(&self) -> Option<u64> {
        self.since_unix_ms
    }
}

/// One usage event to record. Every field maps to the frozen schema
/// surface; `is_estimate` is mandatory.
#[derive(Debug, Clone)]
pub(crate) struct RecordRequest {
    pub source: String,
    pub start_unix_ms: u64,
    pub end_unix_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub is_estimate: bool,
    pub cost: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UsageError {
    MalformedRequest,
    StoreUnavailable,
    ClockUnavailable,
}

/// Shared collector state across commands.
#[derive(Clone, Default)]
pub struct UsageService {
    inner: Arc<Mutex<UsageCore>>,
}

#[derive(Default)]
struct UsageCore {
    /// Monotonic per-process sequence for returned record ids.
    sequence: u64,
    /// session_id -> start unix ms for open terminal sessions.
    terminal_sessions: HashMap<String, u64>,
    /// environment_id -> start unix ms for open Managed runtime sessions.
    runtime_sessions: HashMap<String, u64>,
}

pub(crate) fn records_path(app: &AppHandle) -> Result<PathBuf, UsageError> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(RECORDS_FILE_NAME))
        .map_err(|_| UsageError::StoreUnavailable)
}

/// Validate and append one usage record. Returns the record id (a local,
/// non-persisted correlation id) or a validation/storage error.
pub(crate) fn record(
    service: &UsageService,
    path: &Path,
    request: RecordRequest,
) -> Result<String, UsageError> {
    record_at(service, path, request, unix_ms()?)
}

/// Record a notification delivery usage event (zero-length period). Called
/// by the command layer only after the notification was actually audited.
pub(crate) fn record_notification(
    service: &UsageService,
    path: &Path,
) -> Result<String, UsageError> {
    let now = unix_ms()?;
    record_at(
        service,
        path,
        RecordRequest {
            source: SOURCE_NOTIFICATION.to_string(),
            start_unix_ms: now,
            end_unix_ms: now,
            input_tokens: 0,
            output_tokens: 0,
            is_estimate: true,
            cost: None,
            currency: None,
        },
        now,
    )
}

/// Remember when a terminal session started (in-memory only; the ledger is
/// written once the session closes).
pub(crate) fn mark_terminal_session_start(service: &UsageService, session_id: &str) {
    let Ok(now) = unix_ms() else { return };
    let Ok(mut core) = service.inner.lock() else {
        return;
    };
    core.terminal_sessions
        .entry(session_id.to_string())
        .or_insert(now);
}

/// Close a terminal session and record its duration as an estimate. A close
/// without an observed start (e.g. the app restarted mid-session) never
/// fabricates a record.
pub(crate) fn mark_terminal_session_end(
    service: &UsageService,
    path: &Path,
    session_id: &str,
) -> Result<(), UsageError> {
    let start = {
        let mut core = lock_core(service)?;
        core.terminal_sessions.remove(session_id)
    };
    let Some(start) = start else { return Ok(()) };
    let end = unix_ms()?;
    record(
        service,
        path,
        RecordRequest {
            source: SOURCE_TERMINAL.to_string(),
            start_unix_ms: start,
            end_unix_ms: end,
            input_tokens: 0,
            output_tokens: 0,
            is_estimate: true,
            cost: None,
            currency: None,
        },
    )?;
    Ok(())
}

/// Observe one Managed runtime state read (command status path): `healthy`
/// opens a session timer, `stopped`/`crashed`/`safe_stop` closes it and
/// records the elapsed period as an estimate. Repeated reads of the same
/// state are no-ops, so each period is recorded exactly once. Failures never
/// block the caller's status read.
pub(crate) fn observe_runtime_state(
    service: &UsageService,
    path: &Path,
    environment_id: &str,
    runtime_state: &str,
) -> Result<(), UsageError> {
    if runtime_state == "healthy" {
        let now = unix_ms()?;
        let mut core = lock_core(service)?;
        core.runtime_sessions
            .entry(environment_id.to_string())
            .or_insert(now);
        return Ok(());
    }
    if !RUNTIME_END_STATES.contains(&runtime_state) {
        return Ok(());
    }
    let start = {
        let mut core = lock_core(service)?;
        core.runtime_sessions.remove(environment_id)
    };
    let Some(start) = start else { return Ok(()) };
    let end = unix_ms()?;
    record(
        service,
        path,
        RecordRequest {
            source: SOURCE_RUNTIME.to_string(),
            start_unix_ms: start,
            end_unix_ms: end,
            input_tokens: 0,
            output_tokens: 0,
            is_estimate: true,
            cost: None,
            currency: None,
        },
    )?;
    Ok(())
}

/// Aggregate the ledger into a snapshot: records newest-first (recordedAt
/// descending), filtered by `since_unix_ms` when given.
pub(crate) fn snapshot(
    path: &Path,
    since_unix_ms: Option<u64>,
) -> Result<UsageSnapshot, UsageError> {
    snapshot_at(path, since_unix_ms, unix_ms()?)
}

fn record_at(
    service: &UsageService,
    path: &Path,
    request: RecordRequest,
    now_ms: u64,
) -> Result<String, UsageError> {
    validate(&request, now_ms)?;
    let mut core = lock_core(service)?;
    core.sequence = core.sequence.wrapping_add(1);
    let id = format!("usage-{now_ms}-{}", core.sequence);
    let record = UsageRecord {
        schema_version: SCHEMA_VERSION,
        source: request.source,
        period: UsagePeriod {
            start: format_utc(request.start_unix_ms),
            end: format_utc(request.end_unix_ms),
        },
        input_tokens: request.input_tokens,
        output_tokens: request.output_tokens,
        cache_read_tokens: None,
        cost: request.cost,
        currency: request.currency,
        is_estimate: request.is_estimate,
        recorded_at_unix_ms: now_ms,
    };
    append_record(path, &record)?;
    Ok(id)
}

fn snapshot_at(
    path: &Path,
    since_unix_ms: Option<u64>,
    now_ms: u64,
) -> Result<UsageSnapshot, UsageError> {
    let mut records: Vec<UsageRecord> = read_records(path)?
        .into_iter()
        .filter(|record| since_unix_ms.is_none_or(|since| record.recorded_at_unix_ms >= since))
        .collect();
    records.sort_by(|left, right| {
        right
            .recorded_at_unix_ms
            .cmp(&left.recorded_at_unix_ms)
            .then_with(|| right.period.start.cmp(&left.period.start))
            .then_with(|| left.source.cmp(&right.source))
    });
    Ok(UsageSnapshot {
        schema_version: SCHEMA_VERSION,
        generated_at_unix_ms: now_ms,
        totals: aggregate(&records),
        records,
    })
}

fn aggregate(records: &[UsageRecord]) -> UsageTotals {
    let mut totals = UsageTotals::default();
    for record in records {
        totals.input_tokens = totals.input_tokens.saturating_add(record.input_tokens);
        totals.output_tokens = totals.output_tokens.saturating_add(record.output_tokens);
        if record.is_estimate {
            totals.estimate_count = totals.estimate_count.saturating_add(1);
        }
        if let Some(cost) = record.cost {
            match totals.cost {
                None => {
                    totals.cost = Some(cost);
                    totals.currency = record.currency.clone();
                }
                Some(total) if totals.currency == record.currency => {
                    totals.cost = Some(total + cost);
                }
                // Mixed or unlabeled currencies are never summed: the total
                // stays anchored to the first currency seen.
                Some(_) => {}
            }
        }
    }
    totals
}

/// Schema semantics for a record request. `is_estimate` is a plain bool in
/// the request/record surface, so it is always present by construction.
fn validate(request: &RecordRequest, now_ms: u64) -> Result<(), UsageError> {
    let source_chars = request.source.chars().count();
    if source_chars == 0 || source_chars > MAX_SOURCE_CHARS {
        return Err(UsageError::MalformedRequest);
    }
    if request.start_unix_ms > request.end_unix_ms {
        return Err(UsageError::MalformedRequest);
    }
    // A ledger entry describes a completed, past interval; an end in the
    // future is rejected.
    if request.end_unix_ms > now_ms {
        return Err(UsageError::MalformedRequest);
    }
    if request
        .cost
        .is_some_and(|cost| !cost.is_finite() || cost < 0.0)
    {
        return Err(UsageError::MalformedRequest);
    }
    if request
        .currency
        .as_deref()
        .is_some_and(|currency| !is_valid_currency(currency))
    {
        return Err(UsageError::MalformedRequest);
    }
    Ok(())
}

fn is_valid_currency(currency: &str) -> bool {
    currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase())
}

/// Append one record to the JSONL ledger. Writers are serialized by the
/// service lock, so a local append-only trail stays well-formed.
fn append_record(path: &Path, record: &UsageRecord) -> Result<(), UsageError> {
    let parent = path.parent().ok_or(UsageError::StoreUnavailable)?;
    fs::create_dir_all(parent).map_err(|_| UsageError::StoreUnavailable)?;
    restrict_directory(parent)?;

    let payload = serde_json::to_string(record).map_err(|_| UsageError::StoreUnavailable)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| UsageError::StoreUnavailable)?;
    file.write_all(format!("{payload}\n").as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| UsageError::StoreUnavailable)?;
    drop(file);
    restrict_file(path)?;
    roll_cap(path)
}

/// Rolling cap: keep only the most recent RECORDS_CAP records.
fn roll_cap(path: &Path) -> Result<(), UsageError> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(());
    };
    let lines: Vec<&str> = content
        .split('\n')
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() <= RECORDS_CAP {
        return Ok(());
    }
    let keep = &lines[lines.len() - RECORDS_CAP..];
    let mut payload = String::new();
    for line in keep {
        payload.push_str(line);
        payload.push('\n');
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|_| UsageError::StoreUnavailable)?;
    file.write_all(payload.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| UsageError::StoreUnavailable)
}

/// Best-effort read: a torn or schema-violating line is skipped rather than
/// failing the whole ledger (the ledger is evidence, not control).
fn read_records(path: &Path) -> Result<Vec<UsageRecord>, UsageError> {
    let Ok(file) = fs::File::open(path) else {
        return Ok(Vec::new());
    };
    let mut records = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<UsageRecord>(&line) {
            records.push(record);
        }
    }
    Ok(records)
}

fn lock_core(service: &UsageService) -> Result<MutexGuard<'_, UsageCore>, UsageError> {
    service
        .inner
        .lock()
        .map_err(|_| UsageError::StoreUnavailable)
}

fn unix_ms() -> Result<u64, UsageError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| UsageError::ClockUnavailable)?
        .as_millis()
        .try_into()
        .map_err(|_| UsageError::ClockUnavailable)
}

/// Format unix milliseconds as an RFC 3339 UTC timestamp
/// (`YYYY-MM-DDTHH:MM:SS.mmmZ`), the date-time shape the usage schemas use.
fn format_utc(unix_ms: u64) -> String {
    let (days, millis) = (unix_ms / 86_400_000, unix_ms % 86_400_000);
    let (year, month, day) = civil_from_days(days as i64);
    let (hours, rest) = (millis / 3_600_000, millis % 3_600_000);
    let (minutes, rest) = (rest / 60_000, rest % 60_000);
    let (seconds, millis) = (rest / 1_000, rest % 1_000);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}

/// Days since 1970-01-01 to civil (year, month, day), Howard Hinnant's
/// algorithm (correct for the u64 range of unix milliseconds).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), UsageError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| UsageError::StoreUnavailable)
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), UsageError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file(path: &Path) -> Result<(), UsageError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| UsageError::StoreUnavailable)
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) -> Result<(), UsageError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let id = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "dsh-desktop-usage-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn request(
        source: &str,
        start: u64,
        end: u64,
        input: u64,
        output: u64,
        is_estimate: bool,
    ) -> RecordRequest {
        RecordRequest {
            source: source.to_string(),
            start_unix_ms: start,
            end_unix_ms: end,
            input_tokens: input,
            output_tokens: output,
            is_estimate,
            cost: None,
            currency: None,
        }
    }

    const NOW: u64 = 1_787_792_400_000;

    #[test]
    fn record_validates_source_period_and_token_semantics() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        // Source must be 1-64 chars (schema minLength/maxLength).
        let empty = request("", 1000, 2000, 0, 0, true);
        assert!(matches!(
            record_at(&service, &path, empty, NOW),
            Err(UsageError::MalformedRequest)
        ));
        let overlong = request(&"x".repeat(65), 1000, 2000, 0, 0, true);
        assert!(matches!(
            record_at(&service, &path, overlong, NOW),
            Err(UsageError::MalformedRequest)
        ));

        // Period must satisfy start <= end.
        let reversed = request("terminal", 2000, 1000, 0, 0, true);
        assert!(matches!(
            record_at(&service, &path, reversed, NOW),
            Err(UsageError::MalformedRequest)
        ));

        // A zero-length period is valid and nothing invalid was persisted.
        let zero = request("terminal", 1000, 1000, 0, 0, true);
        assert!(record_at(&service, &path, zero, NOW).is_ok());
        assert_eq!(read_records(&path).expect("ledger records").len(), 1);
    }

    #[test]
    fn record_rejects_future_timestamps() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        // An interval ending after the collector clock is impossible.
        let future = request("terminal", NOW - 1000, NOW + 1, 0, 0, true);
        assert!(matches!(
            record_at(&service, &path, future, NOW),
            Err(UsageError::MalformedRequest)
        ));
        // An interval ending exactly at the clock is accepted.
        let present = request("terminal", NOW - 1000, NOW, 0, 0, true);
        assert!(record_at(&service, &path, present, NOW).is_ok());
        assert_eq!(read_records(&path).expect("ledger records").len(), 1);
    }

    #[test]
    fn record_validates_currency_and_cost() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        let mut usd = request("runtime", 1000, 2000, 0, 0, true);
        usd.currency = Some("USD".to_string());
        assert!(record_at(&service, &path, usd, NOW).is_ok());

        for bad in ["usd", "US", "USDT", "US1"] {
            let mut record = request("runtime", 1000, 2000, 0, 0, true);
            record.currency = Some(bad.to_string());
            assert!(
                matches!(
                    record_at(&service, &path, record, NOW),
                    Err(UsageError::MalformedRequest)
                ),
                "currency {bad} must be rejected"
            );
        }

        // Cost is bounded by the schema minimum of 0.
        let mut negative = request("runtime", 1000, 2000, 0, 0, true);
        negative.cost = Some(-0.01);
        assert!(matches!(
            record_at(&service, &path, negative, NOW),
            Err(UsageError::MalformedRequest)
        ));
        assert_eq!(read_records(&path).expect("ledger records").len(), 1);
    }

    #[test]
    fn records_without_is_estimate_are_skipped() {
        let directory = TestDirectory::new();
        let path = directory.0.join("usage-records-v1.jsonl");
        // A line missing the mandatory isEstimate field (schema requirement)
        // is not a valid usage record and must never reach a snapshot.
        fs::write(
            &path,
            "{\"schemaVersion\":1,\"source\":\"runtime\",\"period\":{\"start\":\"2026-08-28T10:00:00Z\",\"end\":\"2026-08-28T11:00:00Z\"},\"inputTokens\":1,\"outputTokens\":1,\"recordedAtUnixMs\":1787792400000}\n",
        )
        .expect("seed ledger without isEstimate");

        let snap = snapshot_at(&path, None, NOW).expect("snapshot");
        assert!(
            snap.records.is_empty(),
            "records without isEstimate must not validate"
        );
        assert_eq!(snap.totals.estimate_count, 0);
    }

    #[test]
    fn snapshot_orders_records_newest_first() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        record_at(
            &service,
            &path,
            request("terminal", 1000, 2000, 0, 0, true),
            NOW,
        )
        .expect("oldest");
        record_at(
            &service,
            &path,
            request("runtime", 1000, 2000, 0, 0, true),
            NOW + 500,
        )
        .expect("middle");
        record_at(
            &service,
            &path,
            request("notification", 1000, 1000, 0, 0, true),
            NOW + 1000,
        )
        .expect("newest");

        let snap = snapshot_at(&path, None, NOW + 2000).expect("snapshot");
        let recorded: Vec<u64> = snap
            .records
            .iter()
            .map(|record| record.recorded_at_unix_ms)
            .collect();
        assert_eq!(recorded, vec![NOW + 1000, NOW + 500, NOW]);
    }

    #[test]
    fn snapshot_totals_and_estimate_count_aggregate() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        record_at(
            &service,
            &path,
            request("terminal", 1000, 2000, 120, 30, true),
            NOW,
        )
        .expect("estimate record");
        record_at(
            &service,
            &path,
            request("runtime", 2000, 3000, 200, 40, false),
            NOW + 1,
        )
        .expect("measured record");
        record_at(
            &service,
            &path,
            request("notification", 3000, 3000, 0, 0, true),
            NOW + 2,
        )
        .expect("estimate record");

        let snap = snapshot_at(&path, None, NOW + 3).expect("snapshot");
        assert_eq!(snap.generated_at_unix_ms, NOW + 3);
        assert_eq!(snap.totals.input_tokens, 320);
        assert_eq!(snap.totals.output_tokens, 70);
        assert_eq!(snap.totals.estimate_count, 2);
        assert_eq!(snap.records.len(), 3);
    }

    #[test]
    fn snapshot_filters_records_by_since() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        record_at(
            &service,
            &path,
            request("terminal", 1000, 2000, 100, 10, true),
            NOW,
        )
        .expect("before window");
        record_at(
            &service,
            &path,
            request("runtime", 2000, 3000, 200, 20, true),
            NOW + 1000,
        )
        .expect("inside window");
        record_at(
            &service,
            &path,
            request("runtime", 3000, 4000, 300, 30, true),
            NOW + 2000,
        )
        .expect("inside window");

        let snap = snapshot_at(&path, Some(NOW + 1000), NOW + 3000).expect("snapshot");
        assert_eq!(snap.records.len(), 2);
        assert_eq!(snap.records[0].recorded_at_unix_ms, NOW + 2000);
        assert_eq!(snap.records[1].recorded_at_unix_ms, NOW + 1000);
        assert_eq!(snap.totals.input_tokens, 500);
        assert_eq!(snap.totals.output_tokens, 50);
        assert_eq!(snap.totals.estimate_count, 2);
    }

    #[test]
    fn snapshot_totals_aggregate_cost_within_one_currency() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        let mut first = request("runtime", 1000, 2000, 0, 0, true);
        first.cost = Some(1.5);
        first.currency = Some("USD".to_string());
        record_at(&service, &path, first, NOW).expect("usd record");

        let mut second = request("terminal", 2000, 3000, 0, 0, true);
        second.cost = Some(2.5);
        second.currency = Some("USD".to_string());
        record_at(&service, &path, second, NOW + 1).expect("usd record");

        // Same-currency records sum under the newest cost-bearing record's
        // currency (snapshot order is newest-first).
        let snap = snapshot_at(&path, None, NOW + 1).expect("snapshot");
        assert_eq!(snap.totals.cost, Some(4.0));
        assert_eq!(snap.totals.currency.as_deref(), Some("USD"));

        // A newer record in a different currency re-anchors the total; the
        // older foreign-currency costs are never mixed in.
        let mut third = request("notification", 3000, 3000, 0, 0, true);
        third.cost = Some(9.0);
        third.currency = Some("EUR".to_string());
        record_at(&service, &path, third, NOW + 2).expect("eur record");
        let snap = snapshot_at(&path, None, NOW + 3).expect("snapshot");
        assert_eq!(snap.totals.cost, Some(9.0));
        assert_eq!(snap.totals.currency.as_deref(), Some("EUR"));
    }

    #[test]
    fn usage_ledger_rolls_to_the_most_recent_cap() {
        let directory = TestDirectory::new();
        let path = directory.0.join("usage-records-v1.jsonl");
        let mut payload = String::new();
        for index in 0..(RECORDS_CAP as u64 + 25) {
            payload.push_str(&format!(
                "{{\"schemaVersion\":1,\"source\":\"runtime\",\"period\":{{\"start\":\"2026-08-28T10:00:00Z\",\"end\":\"2026-08-28T11:00:00Z\"}},\"inputTokens\":0,\"outputTokens\":0,\"isEstimate\":true,\"recordedAtUnixMs\":{index}}}"
            ));
            payload.push('\n');
        }
        fs::write(&path, payload).expect("seed ledger");
        roll_cap(&path).expect("roll cap");

        let records = read_records(&path).expect("ledger records");
        assert_eq!(records.len(), RECORDS_CAP);
        assert_eq!(records[0].recorded_at_unix_ms, 25);
        assert_eq!(
            records[RECORDS_CAP - 1].recorded_at_unix_ms,
            RECORDS_CAP as u64 + 24
        );
    }

    #[test]
    fn terminal_session_records_duration_as_estimate() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        mark_terminal_session_start(&service, "pty-session-1");
        // A close without an observed start never fabricates a record.
        mark_terminal_session_end(&service, &path, "pty-unknown").expect("unknown close");
        assert_eq!(read_records(&path).expect("ledger records").len(), 0);

        mark_terminal_session_end(&service, &path, "pty-session-1").expect("session close");
        let records = read_records(&path).expect("ledger records");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source, SOURCE_TERMINAL);
        assert!(records[0].is_estimate);
        assert_eq!((records[0].input_tokens, records[0].output_tokens), (0, 0));
        assert!(records[0].period.start <= records[0].period.end);
    }

    #[test]
    fn runtime_observation_records_exactly_one_session_per_period() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        observe_runtime_state(&service, &path, "managed-local", "healthy").expect("healthy");
        // Repeated reads of the same state never re-arm or duplicate.
        observe_runtime_state(&service, &path, "managed-local", "healthy").expect("still healthy");
        observe_runtime_state(&service, &path, "managed-local", "starting").expect("transient");

        observe_runtime_state(&service, &path, "managed-local", "crashed").expect("crashed");
        observe_runtime_state(&service, &path, "managed-local", "crashed").expect("repeat crashed");

        // A new healthy period opens and closes with safe_stop.
        observe_runtime_state(&service, &path, "managed-local", "healthy").expect("healthy again");
        observe_runtime_state(&service, &path, "managed-local", "safe_stop").expect("safe stop");

        let records = read_records(&path).expect("ledger records");
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|record| record.source == SOURCE_RUNTIME));
        assert!(records.iter().all(|record| record.is_estimate));
        assert!(
            records
                .iter()
                .all(|record| record.input_tokens == 0 && record.output_tokens == 0)
        );
        assert!(
            records
                .iter()
                .all(|record| record.period.start <= record.period.end)
        );
    }

    #[test]
    fn terminal_and_notification_wiring_never_leaks_content() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        // Simulate the command-layer wiring with identifiers that would be
        // sensitive if leaked: a terminal session id and a notification
        // subject. The ledger must never receive either.
        mark_terminal_session_start(&service, "pty-secret-7");
        record_notification(&service, &path).expect("notification usage record");
        mark_terminal_session_end(&service, &path, "pty-secret-7").expect("terminal usage record");

        let content = fs::read_to_string(&path).expect("ledger content");
        assert!(
            !content.contains("secret"),
            "usage ledger must not leak terminal session data"
        );
        assert!(
            !content.contains("pty-"),
            "usage ledger must not leak session ids"
        );

        // Every persisted line carries only schema fields — never
        // content-bearing keys such as body/data/output/title.
        for line in content.lines().filter(|line| !line.is_empty()) {
            let value: serde_json::Value = serde_json::from_str(line).expect("parse line");
            let object = value.as_object().expect("record object");
            assert!(
                object.keys().all(|key| matches!(
                    key.as_str(),
                    "schemaVersion"
                        | "source"
                        | "period"
                        | "inputTokens"
                        | "outputTokens"
                        | "cost"
                        | "currency"
                        | "isEstimate"
                        | "recordedAtUnixMs"
                )),
                "unexpected content-bearing key: {object:?}"
            );
            let period = object
                .get("period")
                .and_then(|value| value.as_object())
                .expect("period object");
            let mut period_keys: Vec<&String> = period.keys().collect();
            period_keys.sort();
            assert_eq!(period_keys, ["end", "start"]);
        }

        let records = read_records(&path).expect("ledger records");
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|record| record.input_tokens == 0 && record.output_tokens == 0)
        );
        assert!(records.iter().all(|record| record.is_estimate));
        assert!(
            records
                .iter()
                .any(|record| record.source == SOURCE_TERMINAL)
        );
        assert!(
            records
                .iter()
                .any(|record| record.source == SOURCE_NOTIFICATION)
        );
    }

    #[test]
    fn record_and_snapshot_serialize_schema_compliant_camel_case() {
        let directory = TestDirectory::new();
        let service = UsageService::default();
        let path = directory.0.join("usage-records-v1.jsonl");

        let id = record_at(
            &service,
            &path,
            request("runtime", 1000, 2000, 10, 5, true),
            NOW,
        )
        .expect("record");
        assert!(id.starts_with("usage-"), "record returns a local id: {id}");

        let record = read_records(&path)
            .expect("ledger records")
            .pop()
            .expect("record");
        let value = serde_json::to_value(&record).expect("serialize record");
        let object = value.as_object().expect("record object");
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "inputTokens",
                "isEstimate",
                "outputTokens",
                "period",
                "recordedAtUnixMs",
                "schemaVersion",
                "source"
            ],
            "optional fields must be omitted when absent"
        );
        assert_eq!(object.get("schemaVersion"), Some(&serde_json::json!(1)));
        assert_eq!(object.get("source"), Some(&serde_json::json!("runtime")));
        assert_eq!(object.get("isEstimate"), Some(&serde_json::json!(true)));
        let period = object
            .get("period")
            .and_then(|value| value.as_object())
            .expect("period object");
        assert_eq!(
            period.get("start"),
            Some(&serde_json::json!("1970-01-01T00:00:01.000Z"))
        );
        assert_eq!(
            period.get("end"),
            Some(&serde_json::json!("1970-01-01T00:00:02.000Z"))
        );

        let snap = snapshot_at(&path, None, NOW + 1).expect("snapshot");
        let snap_value = serde_json::to_value(&snap).expect("serialize snapshot");
        let snap_object = snap_value.as_object().expect("snapshot object");
        assert_eq!(
            snap_object.get("schemaVersion"),
            Some(&serde_json::json!(1))
        );
        assert!(snap_object.contains_key("generatedAtUnixMs"));
        assert!(snap_object.contains_key("records"));
        assert!(snap_object.contains_key("totals"));
        let totals = snap_object
            .get("totals")
            .and_then(|value| value.as_object())
            .expect("totals object");
        assert!(totals.contains_key("estimateCount"));
        assert!(!totals.contains_key("cost"), "cost omitted when absent");
    }

    #[test]
    fn format_utc_produces_rfc3339_timestamps() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_utc(1_787_792_400_000), "2026-08-27T01:00:00.000Z");
        assert_eq!(format_utc(1_787_911_200_000), "2026-08-28T10:00:00.000Z");
    }
}
