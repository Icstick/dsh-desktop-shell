//! Notification registry (M3-A, IF-NOTIFICATION, ADR-0016).
//!
//! Local-first notification service with:
//! - content-policy enforcement (AC-NOT-001): only `explicit_body` may carry
//!   a body; `title_only` / `redacted_summary` requests with a body are
//!   rejected, and every audit record redacts the body unless the policy is
//!   `explicit_body`;
//! - dedupeKey folding (AC-NOT-002): a key is folded while its TTL window is
//!   open and a folded request is never re-audited;
//! - an append-only audit trail in AppData
//!   (`notification-audit-v1.jsonl`), rolling-capped at AUDIT_CAP records;
//! - a minimal Managed runtime watcher: the status read path reports each
//!   healthy/crashed/safe_stop/stopped transition exactly once and the Shell
//!   WebView receives it over `notification://event`.
//!
//! The Supervisor state machine itself is untouched; all wiring lives in the
//! command status read path (see commands::get_managed_runtime_status).

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

const SCHEMA_VERSION: u8 = 1;
/// Fold window for dedupeKey (AC-NOT-002).
const DEDUPE_TTL_MS: u64 = 60_000;
/// Rolling cap for the append-only audit trail.
const AUDIT_CAP: usize = 1000;
/// Soft cap for the in-memory dedupe window map; expired windows are pruned
/// once the map grows past this size.
const DEDUPE_MAP_LIMIT: usize = 1024;
const AUDIT_FILE_NAME: &str = "notification-audit-v1.jsonl";
/// Tauri event channel to the Shell WebView.
const EVENT_NAME: &str = "notification://event";
/// Source label for application-raised notifications.
pub(crate) const SOURCE_APP: &str = "shell";
/// Source label for Managed runtime transition notifications.
const SOURCE_RUNTIME: &str = "managed_runtime";
const MAX_TITLE_CHARS: usize = 128;
const MAX_BODY_CHARS: usize = 512;
const MAX_DEDUPE_KEY_CHARS: usize = 128;
/// Managed states that emit a `runtime_changed` notification, once per
/// observed transition.
const RUNTIME_CHANGE_STATES: [&str; 4] = ["healthy", "crashed", "safe_stop", "stopped"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEvent {
    TurnCompleted,
    ApprovalRequired,
    QuestionRequired,
    RuntimeChanged,
    ScheduleResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentPolicy {
    TitleOnly,
    RedactedSummary,
    ExplicitBody,
}

impl ContentPolicy {
    fn delivers_body(self) -> bool {
        matches!(self, Self::ExplicitBody)
    }
}

/// Mirrors specs/notification/notification-request.schema.json.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRequest {
    schema_version: u8,
    event: NotificationEvent,
    title: String,
    body: Option<String>,
    content_policy: ContentPolicy,
    dedupe_key: Option<String>,
}

impl NotificationRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn event(&self) -> NotificationEvent {
        self.event
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    pub(crate) fn content_policy(&self) -> ContentPolicy {
        self.content_policy
    }

    pub(crate) fn dedupe_key(&self) -> Option<&str> {
        self.dedupe_key.as_deref()
    }
}

/// Mirrors specs/notification/notification-report.schema.json.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationReport {
    schema_version: u8,
    id: String,
    event: NotificationEvent,
    title: String,
    content_policy: ContentPolicy,
    delivered_body: Option<String>,
    created_at_unix_ms: u64,
    dedupe_key: Option<String>,
    deduplicated: bool,
}

/// Mirrors specs/notification/notification-record.schema.json.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationRecord {
    schema_version: u8,
    id: String,
    event: NotificationEvent,
    title: String,
    content_policy: ContentPolicy,
    body: Option<String>,
    created_at_unix_ms: u64,
    dedupe_key: Option<String>,
    source: String,
}

impl NotificationReport {
    pub(crate) fn deduplicated(&self) -> bool {
        self.deduplicated
    }
}

/// Dismissal request for the `dismiss_notification` command.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationDismissRequest {
    schema_version: u8,
    notification_id: String,
}

impl NotificationDismissRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn notification_id(&self) -> &str {
        &self.notification_id
    }
}

/// Managed registry shared across commands.
#[derive(Clone, Default)]
pub struct NotificationService {
    inner: Arc<Mutex<NotificationCore>>,
}

#[derive(Default)]
struct NotificationCore {
    /// Monotonic per-process sequence for notification ids.
    sequence: u64,
    /// dedupeKey -> expiry (unix ms); the window is anchored at the first
    /// delivery and is never extended by folded requests.
    dedupe_until: HashMap<String, u64>,
    /// Dismissed notification ids (UI-level; the audit trail stays intact).
    dismissed: HashSet<String>,
    /// environment_id -> last observed Managed runtime state.
    last_runtime_state: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationError {
    MalformedRequest,
    AuditUnavailable,
    ClockUnavailable,
}

pub(crate) fn audit_path(app: &AppHandle) -> Result<PathBuf, NotificationError> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(AUDIT_FILE_NAME))
        .map_err(|_| NotificationError::AuditUnavailable)
}

/// AC-NOT-001: a request is valid only when it satisfies the frozen schema
/// surface — event/title/policy are required, body is exclusive to
/// `explicit_body`, and every string length is bounded.
fn validate_request(request: &NotificationRequest) -> bool {
    if request.schema_version != SCHEMA_VERSION {
        return false;
    }
    let title_chars = request.title.chars().count();
    if title_chars == 0 || title_chars > MAX_TITLE_CHARS {
        return false;
    }
    if let Some(key) = request.dedupe_key() {
        let key_chars = key.chars().count();
        if key_chars == 0 || key_chars > MAX_DEDUPE_KEY_CHARS {
            return false;
        }
    }
    match request.content_policy {
        ContentPolicy::TitleOnly | ContentPolicy::RedactedSummary => request.body().is_none(),
        ContentPolicy::ExplicitBody => request
            .body()
            .is_none_or(|body| body.chars().count() <= MAX_BODY_CHARS),
    }
}

/// Deliver one notification: policy-check, dedupe-fold, audit and report.
///
/// A folded request (dedupeKey inside its TTL window) returns
/// `deduplicated: true` and is never appended to the audit trail
/// (AC-NOT-002).
pub(crate) fn notify(
    service: &NotificationService,
    path: &Path,
    request: NotificationRequest,
    source: &str,
) -> Result<NotificationReport, NotificationError> {
    if !validate_request(&request) {
        return Err(NotificationError::MalformedRequest);
    }
    let mut core = lock_core(service)?;
    let now = unix_ms()?;

    let deduplicated = request
        .dedupe_key()
        .is_some_and(|key| dedupe_check(&mut core, key, now));
    if deduplicated {
        return Ok(NotificationReport {
            schema_version: SCHEMA_VERSION,
            id: next_id(now, &mut core),
            event: request.event(),
            title: request.title().to_string(),
            content_policy: request.content_policy(),
            delivered_body: None,
            created_at_unix_ms: now,
            dedupe_key: request.dedupe_key().map(str::to_string),
            deduplicated: true,
        });
    }

    // Audit redaction (ADR-0016): the body is written only under
    // explicit_body; every other policy audits a null body.
    let record = NotificationRecord {
        schema_version: SCHEMA_VERSION,
        id: next_id(now, &mut core),
        event: request.event(),
        title: request.title().to_string(),
        content_policy: request.content_policy(),
        body: request
            .content_policy()
            .delivers_body()
            .then(|| request.body().unwrap_or_default().to_string()),
        created_at_unix_ms: now,
        dedupe_key: request.dedupe_key().map(str::to_string),
        source: source.to_string(),
    };
    append_record(path, &record)?;

    Ok(NotificationReport {
        schema_version: SCHEMA_VERSION,
        id: record.id.clone(),
        event: record.event,
        title: record.title.clone(),
        content_policy: record.content_policy,
        delivered_body: record.body.clone(),
        created_at_unix_ms: record.created_at_unix_ms,
        dedupe_key: record.dedupe_key.clone(),
        deduplicated: false,
    })
}

/// Most recent notifications, newest first, excluding dismissed ids.
pub(crate) fn list(
    service: &NotificationService,
    path: &Path,
) -> Result<Vec<NotificationReport>, NotificationError> {
    let core = lock_core(service)?;
    let mut reports: Vec<NotificationReport> = read_records(path)?
        .into_iter()
        .filter(|record| !core.dismissed.contains(&record.id))
        .map(|record| NotificationReport {
            schema_version: SCHEMA_VERSION,
            id: record.id,
            event: record.event,
            title: record.title,
            content_policy: record.content_policy,
            delivered_body: record.body,
            created_at_unix_ms: record.created_at_unix_ms,
            dedupe_key: record.dedupe_key,
            deduplicated: false,
        })
        .collect();
    reports.sort_by(|left, right| {
        right
            .created_at_unix_ms
            .cmp(&left.created_at_unix_ms)
            .then_with(|| right.id.cmp(&left.id))
    });
    Ok(reports)
}

/// Dismiss a notification id so later lists omit it.
pub(crate) fn dismiss(
    service: &NotificationService,
    notification_id: &str,
) -> Result<(), NotificationError> {
    if !is_valid_notification_id(notification_id) {
        return Err(NotificationError::MalformedRequest);
    }
    let mut core = lock_core(service)?;
    core.dismissed.insert(notification_id.to_string());
    Ok(())
}

/// Minimal Managed runtime wiring (M3-A): observe one state read and return
/// a `runtime_changed` report exactly once per transition into a stable
/// state (healthy/crashed/safe_stop/stopped). The report is audited here;
/// the caller decides whether to emit it to the WebView.
pub(crate) fn runtime_change_report(
    service: &NotificationService,
    path: &Path,
    environment_id: &str,
    runtime_state: &str,
) -> Result<Option<NotificationReport>, NotificationError> {
    if !RUNTIME_CHANGE_STATES.contains(&runtime_state) {
        return Ok(None);
    }
    let mut core = lock_core(service)?;
    let previous = core
        .last_runtime_state
        .insert(environment_id.to_string(), runtime_state.to_string());
    if previous.as_deref() == Some(runtime_state) {
        return Ok(None);
    }

    let now = unix_ms()?;
    let record = NotificationRecord {
        schema_version: SCHEMA_VERSION,
        id: next_id(now, &mut core),
        event: NotificationEvent::RuntimeChanged,
        title: format!("Managed runtime {runtime_state}"),
        content_policy: ContentPolicy::TitleOnly,
        body: None,
        created_at_unix_ms: now,
        dedupe_key: None,
        source: SOURCE_RUNTIME.to_string(),
    };
    append_record(path, &record)?;

    Ok(Some(NotificationReport {
        schema_version: SCHEMA_VERSION,
        id: record.id.clone(),
        event: record.event,
        title: record.title.clone(),
        content_policy: record.content_policy,
        delivered_body: None,
        created_at_unix_ms: record.created_at_unix_ms,
        dedupe_key: None,
        deduplicated: false,
    }))
}

/// Emit a transition report to the Shell WebView when one occurred.
pub(crate) fn maybe_emit_runtime_change(
    app: &AppHandle,
    service: &NotificationService,
    path: &Path,
    environment_id: &str,
    runtime_state: &str,
) -> Result<(), NotificationError> {
    if let Some(report) = runtime_change_report(service, path, environment_id, runtime_state)? {
        let _ = app.emit(EVENT_NAME, &report);
    }
    Ok(())
}

/// Id pattern from the frozen schemas: `^notif-[a-z0-9-]+$`.
pub(crate) fn is_valid_notification_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("notif-") else {
        return false;
    };
    !rest.is_empty()
        && rest.len() <= 128
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn lock_core(
    service: &NotificationService,
) -> Result<MutexGuard<'_, NotificationCore>, NotificationError> {
    service
        .inner
        .lock()
        .map_err(|_| NotificationError::AuditUnavailable)
}

fn unix_ms() -> Result<u64, NotificationError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| NotificationError::ClockUnavailable)?
        .as_millis()
        .try_into()
        .map_err(|_| NotificationError::ClockUnavailable)
}

fn next_id(now_ms: u64, core: &mut NotificationCore) -> String {
    core.sequence = core.sequence.wrapping_add(1);
    format!("notif-{now_ms}-{}", core.sequence)
}

/// AC-NOT-002 fold check with an injectable clock. The window is anchored at
/// the first delivery; folded requests neither extend nor re-arm it, so the
/// TTL always expires from the original delivery.
fn dedupe_check(core: &mut NotificationCore, key: &str, now_ms: u64) -> bool {
    if core
        .dedupe_until
        .get(key)
        .is_some_and(|until| *until > now_ms)
    {
        return true;
    }
    if core.dedupe_until.len() >= DEDUPE_MAP_LIMIT {
        core.dedupe_until.retain(|_, until| *until > now_ms);
    }
    core.dedupe_until
        .insert(key.to_string(), now_ms.saturating_add(DEDUPE_TTL_MS));
    false
}

fn append_record(path: &Path, record: &NotificationRecord) -> Result<(), NotificationError> {
    let parent = path.parent().ok_or(NotificationError::AuditUnavailable)?;
    fs::create_dir_all(parent).map_err(|_| NotificationError::AuditUnavailable)?;
    restrict_directory(parent)?;

    let payload = serde_json::to_string(record).map_err(|_| NotificationError::AuditUnavailable)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| NotificationError::AuditUnavailable)?;
    // One write syscall per record; writers are additionally serialized by
    // the service lock, so a local append-only trail stays well-formed.
    file.write_all(format!("{payload}\n").as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| NotificationError::AuditUnavailable)?;
    drop(file);
    restrict_file(path)?;
    roll_cap(path)
}

/// Rolling cap: keep only the most recent AUDIT_CAP records.
fn roll_cap(path: &Path) -> Result<(), NotificationError> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(());
    };
    let lines: Vec<&str> = content
        .split('\n')
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() <= AUDIT_CAP {
        return Ok(());
    }
    let keep = &lines[lines.len() - AUDIT_CAP..];
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
        .map_err(|_| NotificationError::AuditUnavailable)?;
    file.write_all(payload.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| NotificationError::AuditUnavailable)
}

/// Best-effort read: a torn or malformed trailing line is skipped rather
/// than failing the whole list (the audit trail is evidence, not control).
fn read_records(path: &Path) -> Result<Vec<NotificationRecord>, NotificationError> {
    let Ok(file) = fs::File::open(path) else {
        return Ok(Vec::new());
    };
    let mut records = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<NotificationRecord>(&line) {
            records.push(record);
        }
    }
    Ok(records)
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), NotificationError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| NotificationError::AuditUnavailable)
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), NotificationError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file(path: &Path) -> Result<(), NotificationError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| NotificationError::AuditUnavailable)
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) -> Result<(), NotificationError> {
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
                "dsh-desktop-notification-test-{}-{id}",
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
        event: &str,
        title: &str,
        policy: &str,
        body: Option<&str>,
        dedupe_key: Option<&str>,
    ) -> NotificationRequest {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "event": event,
            "title": title,
            "contentPolicy": policy,
            "body": body,
            "dedupeKey": dedupe_key,
        }))
        .expect("notification request fixture")
    }

    #[test]
    fn content_policy_forces_body_drop_for_title_only_and_redacted_summary() {
        let directory = TestDirectory::new();
        let service = NotificationService::default();
        let path = directory.0.join("state/notification-audit-v1.jsonl");

        // AC-NOT-001: body is rejected unless the policy is explicit_body.
        let rejected = notify(
            &service,
            &path,
            request(
                "turn_completed",
                "Turn done",
                "title_only",
                Some("secret details"),
                None,
            ),
            SOURCE_APP,
        );
        assert!(matches!(rejected, Err(NotificationError::MalformedRequest)));
        let rejected = notify(
            &service,
            &path,
            request(
                "turn_completed",
                "Turn done",
                "redacted_summary",
                Some("secret details"),
                None,
            ),
            SOURCE_APP,
        );
        assert!(matches!(rejected, Err(NotificationError::MalformedRequest)));

        // explicit_body delivers the body.
        let report = notify(
            &service,
            &path,
            request(
                "turn_completed",
                "Turn done",
                "explicit_body",
                Some("summary"),
                None,
            ),
            SOURCE_APP,
        )
        .expect("explicit body accepted");
        assert_eq!(report.delivered_body.as_deref(), Some("summary"));
        assert!(!report.deduplicated);

        // title_only without body is accepted and never delivers a body.
        let report = notify(
            &service,
            &path,
            request(
                "runtime_changed",
                "Runtime changed",
                "title_only",
                None,
                None,
            ),
            SOURCE_APP,
        )
        .expect("title only accepted");
        assert_eq!(report.delivered_body, None);
        assert_eq!(report.content_policy, ContentPolicy::TitleOnly);
    }

    #[test]
    fn dedupe_key_folds_within_ttl_without_re_auditing() {
        let directory = TestDirectory::new();
        let service = NotificationService::default();
        let path = directory.0.join("notification-audit-v1.jsonl");

        let first = notify(
            &service,
            &path,
            request(
                "schedule_result",
                "Schedule",
                "explicit_body",
                Some("done"),
                Some("job-42"),
            ),
            SOURCE_APP,
        )
        .expect("first notify");
        let folded = notify(
            &service,
            &path,
            request(
                "schedule_result",
                "Schedule",
                "explicit_body",
                Some("done"),
                Some("job-42"),
            ),
            SOURCE_APP,
        )
        .expect("folded notify");

        assert!(!first.deduplicated);
        assert!(folded.deduplicated);
        assert_eq!(
            read_records(&path).expect("audit records").len(),
            1,
            "folded requests must not be re-audited"
        );
    }

    #[test]
    fn dedupe_key_expires_after_ttl_and_allows_resend() {
        let mut core = NotificationCore::default();
        let key = "job-7";
        let first_seen = 1_787_792_400_000_u64;

        assert!(
            !dedupe_check(&mut core, key, first_seen),
            "first occurrence arms the window"
        );
        assert!(
            dedupe_check(&mut core, key, first_seen + DEDUPE_TTL_MS - 1),
            "still folding just before expiry"
        );
        assert!(
            !dedupe_check(&mut core, key, first_seen + DEDUPE_TTL_MS + 1),
            "an expired window allows a new delivery"
        );
    }

    #[test]
    fn audit_redacts_body_except_for_explicit_body() {
        let directory = TestDirectory::new();
        let service = NotificationService::default();
        let path = directory.0.join("notification-audit-v1.jsonl");

        notify(
            &service,
            &path,
            request("question_required", "Question", "title_only", None, None),
            SOURCE_APP,
        )
        .expect("title only audit");
        notify(
            &service,
            &path,
            request(
                "question_required",
                "Question",
                "explicit_body",
                Some("details"),
                None,
            ),
            SOURCE_APP,
        )
        .expect("explicit body audit");

        let records = read_records(&path).expect("audit records");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].body, None, "title_only audits a null body");
        assert_eq!(
            records[1].body.as_deref(),
            Some("details"),
            "explicit_body audits the delivered body"
        );
        assert!(records.iter().all(|record| record.source == SOURCE_APP));
        assert!(
            records
                .iter()
                .all(|record| is_valid_notification_id(&record.id))
        );
    }

    #[test]
    fn dismiss_hides_notification_from_list() {
        let directory = TestDirectory::new();
        let service = NotificationService::default();
        let path = directory.0.join("notification-audit-v1.jsonl");

        let first = notify(
            &service,
            &path,
            request("approval_required", "Approval", "title_only", None, None),
            SOURCE_APP,
        )
        .expect("first");
        let second = notify(
            &service,
            &path,
            request(
                "approval_required",
                "Approval two",
                "title_only",
                None,
                None,
            ),
            SOURCE_APP,
        )
        .expect("second");

        assert_eq!(list(&service, &path).expect("list").len(), 2);
        dismiss(&service, &first.id).expect("dismiss");
        let remaining = list(&service, &path).expect("list");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, second.id);
    }

    #[test]
    fn list_orders_newest_first() {
        let directory = TestDirectory::new();
        let service = NotificationService::default();
        let path = directory.0.join("notification-audit-v1.jsonl");

        let first = notify(
            &service,
            &path,
            request(
                "approval_required",
                "Approval one",
                "title_only",
                None,
                None,
            ),
            SOURCE_APP,
        )
        .expect("first");
        let second = notify(
            &service,
            &path,
            request(
                "approval_required",
                "Approval two",
                "title_only",
                None,
                None,
            ),
            SOURCE_APP,
        )
        .expect("second");
        let third = notify(
            &service,
            &path,
            request(
                "approval_required",
                "Approval three",
                "title_only",
                None,
                None,
            ),
            SOURCE_APP,
        )
        .expect("third");

        let listed = list(&service, &path).expect("list");
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].id, third.id);
        assert_eq!(listed[1].id, second.id);
        assert_eq!(listed[2].id, first.id);
    }

    #[test]
    fn malformed_requests_are_rejected_without_audit_side_effects() {
        let directory = TestDirectory::new();
        let service = NotificationService::default();
        let path = directory.0.join("notification-audit-v1.jsonl");

        // Wrong schema version.
        let bad: NotificationRequest = serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "event": "runtime_changed",
            "title": "x",
            "contentPolicy": "title_only"
        }))
        .expect("deserialize bad version");
        assert!(matches!(
            notify(&service, &path, bad, SOURCE_APP),
            Err(NotificationError::MalformedRequest)
        ));

        // Empty and overlong titles (schema: minLength 1, maxLength 128).
        let bad = request("runtime_changed", "", "title_only", None, None);
        assert!(matches!(
            notify(&service, &path, bad, SOURCE_APP),
            Err(NotificationError::MalformedRequest)
        ));
        let bad = request(
            "runtime_changed",
            &"x".repeat(129),
            "title_only",
            None,
            None,
        );
        assert!(matches!(
            notify(&service, &path, bad, SOURCE_APP),
            Err(NotificationError::MalformedRequest)
        ));

        // Overlong body (schema: maxLength 512) and dedupe key (128).
        let bad = request(
            "turn_completed",
            "T",
            "explicit_body",
            Some(&"x".repeat(513)),
            None,
        );
        assert!(matches!(
            notify(&service, &path, bad, SOURCE_APP),
            Err(NotificationError::MalformedRequest)
        ));
        let bad = request(
            "turn_completed",
            "T",
            "explicit_body",
            None,
            Some(&"k".repeat(129)),
        );
        assert!(matches!(
            notify(&service, &path, bad, SOURCE_APP),
            Err(NotificationError::MalformedRequest)
        ));

        // Dismiss rejects ids outside the notif- pattern.
        assert!(matches!(
            dismiss(&service, "not-a-notification"),
            Err(NotificationError::MalformedRequest)
        ));

        assert_eq!(
            read_records(&path).expect("audit records").len(),
            0,
            "malformed requests must not touch the audit trail"
        );
    }

    #[test]
    fn runtime_change_emits_exactly_once_per_transition() {
        let directory = TestDirectory::new();
        let service = NotificationService::default();
        let path = directory.0.join("notification-audit-v1.jsonl");

        // First observation of a stable state is a transition.
        let first = runtime_change_report(&service, &path, "managed-local", "healthy")
            .expect("first report");
        let report = first.as_ref().expect("report");
        assert_eq!(report.event, NotificationEvent::RuntimeChanged);
        assert_eq!(report.content_policy, ContentPolicy::TitleOnly);
        assert_eq!(report.delivered_body, None);

        // Repeated reads of the same state never re-notify.
        assert!(
            runtime_change_report(&service, &path, "managed-local", "healthy")
                .expect("repeat read")
                .is_none()
        );

        // Transient states never notify.
        assert!(
            runtime_change_report(&service, &path, "managed-local", "starting")
                .expect("transient read")
                .is_none()
        );

        // A genuine transition notifies exactly once more.
        assert!(
            runtime_change_report(&service, &path, "managed-local", "crashed")
                .expect("crashed report")
                .is_some()
        );
        assert!(
            runtime_change_report(&service, &path, "managed-local", "crashed")
                .expect("repeat crashed read")
                .is_none()
        );

        // Every transition is audited with a redacted body under the
        // managed_runtime source.
        let records = read_records(&path).expect("audit records");
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|record| record.body.is_none()));
        assert!(records.iter().all(|record| record.source == SOURCE_RUNTIME));
        assert!(
            records
                .iter()
                .all(|record| record.event == NotificationEvent::RuntimeChanged)
        );
    }

    #[test]
    fn audit_trail_rolls_to_the_most_recent_cap() {
        let directory = TestDirectory::new();
        let path = directory.0.join("notification-audit-v1.jsonl");
        let mut payload = String::new();
        for index in 0..(AUDIT_CAP as u64 + 25) {
            payload.push_str(&format!(
                "{{\"schemaVersion\":1,\"id\":\"notif-1-{index}\",\"event\":\"schedule_result\",\"title\":\"t\",\"contentPolicy\":\"title_only\",\"body\":null,\"createdAtUnixMs\":{index},\"dedupeKey\":null,\"source\":\"shell\"}}"
            ));
            payload.push('\n');
        }
        fs::write(&path, payload).expect("seed audit trail");
        roll_cap(&path).expect("roll cap");

        let records = read_records(&path).expect("read records");
        assert_eq!(records.len(), AUDIT_CAP);
        assert_eq!(records[0].id, format!("notif-1-{}", 25));
        assert_eq!(
            records[AUDIT_CAP - 1].id,
            format!("notif-1-{}", AUDIT_CAP as u64 + 24)
        );
    }

    #[test]
    fn report_serializes_with_snake_case_event_and_camel_case_fields() {
        let directory = TestDirectory::new();
        let service = NotificationService::default();
        let path = directory.0.join("notification-audit-v1.jsonl");
        let report = notify(
            &service,
            &path,
            request(
                "runtime_changed",
                "Runtime changed",
                "title_only",
                None,
                None,
            ),
            SOURCE_APP,
        )
        .expect("notify");

        let value = serde_json::to_value(&report).expect("serialize report");
        let object = value.as_object().expect("report object");
        assert!(object.contains_key("schemaVersion"));
        assert!(object.contains_key("createdAtUnixMs"));
        assert!(object.contains_key("contentPolicy"));
        assert!(object.contains_key("deliveredBody"));
        assert!(object.contains_key("deduplicated"));
        assert_eq!(
            object.get("event").and_then(|value| value.as_str()),
            Some("runtime_changed")
        );
        assert_eq!(
            object.get("contentPolicy").and_then(|value| value.as_str()),
            Some("title_only")
        );
        assert_eq!(
            object.get("deduplicated").and_then(|value| value.as_bool()),
            Some(false)
        );
    }
}
