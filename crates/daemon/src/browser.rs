//! Browser capability of the daemon (M6-C3): the daemon is the **state
//! authority** for browser sessions (ADR-0019 decision 2, option A —
//! "rendering in the Shell, state authority in the daemon"). The M4
//! `SessionRegistry` (crates/browser-provider, the pure-logic session
//! state machine) now lives in the daemon; the WebView2 rendering stays
//! in the Shell.
//!
//! M6-C3 scope (registry semantics only): `browser.create` /
//! `browser.list` / `browser.status` / `browser.close` — the daemon
//! registers/observes sessions and broadcasts their lifecycle events
//! (`browser.session-created` / `browser.session-closed`) through the
//! `EventRouter`(crate::events). `navigate`/`snapshot` still execute in
//! the Shell (the render process); the state-sync protocol between Shell
//! render events and daemon state (attach, navigate/snapshot reporting,
//! handover re-attach after a Shell restart) is M6-C4.
//!
//! Wire contract: request/report/event shapes mirror
//! `specs/browser/*.schema.json` (BrowserCreateRequest/BrowserCloseRequest,
//! BrowserReport, BrowserEvent) and the envelope methods are the
//! namespaced `browser.create` / `browser.list` / `browser.status` /
//! `browser.close` form (the envelope method pattern
//! `^[a-z][a-z0-9._-]+$`; the M6-B1 placeholder method
//! `list_browsers` is superseded by the namespaced form, mirroring the
//! terminal capability).
//!
//! Sessions are owned by the creating connection (same rule as the
//! terminal capability, M6-C1): close from another connection is
//! rejected (NOT_PROCESS_OWNER) — in the multi-connection daemon world
//! an opaque session id alone is not an access token (AC-BRW-001
//! preserved). The registry keeps closed sessions for audit
//! (`get`/events), but the daemon view (`list`/`status`) exposes
//! live sessions only, mirroring `terminal.status` — the Shell restore
//! flow (M6-C4) re-attaches to live sessions after a restart.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use dsh_browser_provider::{BrowserError, BrowserSession, SessionRegistry, SessionState};

use crate::capabilities::{CapabilityContext, DaemonMethodError};
use crate::envelope::ErrorCode;
use crate::events::RouterEvent;

/// Schema version of every browser wire payload (specs/browser).
pub const SCHEMA_VERSION: u8 = 1;

/// Event methods of the browser lifecycle stream (envelope Event; the M5
/// surface event was `browser://event`, which is not a valid envelope
/// method — the namespaced `browser.session-*` form is).
pub const BROWSER_SESSION_CREATED_EVENT: &str = "browser.session-created";
pub const BROWSER_SESSION_CLOSED_EVENT: &str = "browser.session-closed";

/// Current time in unix milliseconds (lifecycle event timestamps).
pub fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ----------------------------------------------------------------------
// Wire types (specs/browser/*.schema.json)
// ----------------------------------------------------------------------

/// `browser.create` request (browser-create-request.schema.json).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserCreateRequest {
    pub schema_version: u8,
    pub mode: String,
}

impl BrowserCreateRequest {
    /// M6-C3: human surface only (the schema `mode` const). Agent
    /// interact stays a Shell-side concern until M6-C4.
    pub fn is_valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION && self.mode == "human_surface"
    }
}

/// `browser.close` request (browser-close-request.schema.json).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserCloseRequest {
    pub schema_version: u8,
    pub session_id: String,
}

/// `browser.create` / `browser.list` / `browser.status` report
/// entry (browser-report.schema.json).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserReport {
    pub schema_version: u8,
    pub session_id: String,
    pub state: String,
    pub mode: &'static str,
    pub current_url: Option<String>,
    pub created_at_unix_ms: u64,
    pub last_activity_unix_ms: Option<u64>,
    pub error: Option<String>,
}

/// Lifecycle kinds of the daemon browser events (the daemon adds
/// `created` to the M4 surface kinds; `closed` mirrors the registry
/// close event).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserLifecycleKind {
    Created,
    Closed,
}

impl BrowserLifecycleKind {
    /// Stable wire name of the kind (browser-event.schema.json `kind`
    /// enum, extended with `created` in M6-C3).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Closed => "closed",
        }
    }

    /// The envelope Event method carrying this kind.
    pub fn event_method(self) -> &'static str {
        match self {
            Self::Created => BROWSER_SESSION_CREATED_EVENT,
            Self::Closed => BROWSER_SESSION_CLOSED_EVENT,
        }
    }
}

/// One daemon lifecycle event, routed by session id through the
/// `EventRouter` (payload shape of browser-event.schema.json).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserLifecycleEvent {
    pub session_id: String,
    pub kind: BrowserLifecycleKind,
    pub occurred_at_unix_ms: u64,
    /// `None` for lifecycle events (no navigation involved).
    pub url: Option<String>,
}

impl BrowserLifecycleEvent {
    /// Build a lifecycle event stamped with the current time.
    pub fn new(session_id: impl Into<String>, kind: BrowserLifecycleKind) -> Self {
        Self {
            session_id: session_id.into(),
            kind,
            occurred_at_unix_ms: now_unix_ms(),
            url: None,
        }
    }
}

/// Serialized event payload (browser-event.schema.json shape, sent as the
/// envelope Event payload by the server writer).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEventPayload {
    pub schema_version: u8,
    pub session_id: String,
    pub kind: String,
    pub occurred_at_unix_ms: u64,
    pub url: Option<String>,
}

impl From<&BrowserLifecycleEvent> for BrowserEventPayload {
    fn from(event: &BrowserLifecycleEvent) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            session_id: event.session_id.clone(),
            kind: event.kind.as_str().to_string(),
            occurred_at_unix_ms: event.occurred_at_unix_ms,
            url: event.url.clone(),
        }
    }
}

// ----------------------------------------------------------------------
// BrowserHost: session registry + session bookkeeping
// ----------------------------------------------------------------------

/// Daemon-owned browser state: the `SessionRegistry` (the M4 state
/// machine, now the daemon authority) plus the session bookkeeping
/// (connection ownership). One host per daemon, Arc-shared with the
/// capability handlers. The host itself is a pure state layer: lifecycle
/// events are published by the envelope handlers, which own the router
/// wiring (same split as the terminal capability).
pub struct BrowserHost {
    registry: SessionRegistry,
    /// Creating connection per session (mutation ownership).
    owners: Mutex<HashMap<String, u64>>,
}

impl Default for BrowserHost {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserHost {
    pub fn new() -> Self {
        Self {
            registry: SessionRegistry::new(),
            owners: Mutex::new(HashMap::new()),
        }
    }

    /// The session registry (audit events / tests).
    pub fn registry(&self) -> &SessionRegistry {
        &self.registry
    }

    /// Register a new session in `created` state and record the
    /// creating connection as its owner.
    pub fn create(&self, owner: u64) -> Result<BrowserReport, BrowserError> {
        let session = self.registry.create()?;
        if let Ok(mut owners) = self.owners.lock() {
            owners.insert(session.session_id.clone(), owner);
        }
        Ok(public_report(session))
    }

    /// Close a live session (`-> closed`); drops the ownership record
    /// (the registry keeps the closed session for audit).
    pub fn close(&self, session_id: &str) -> Result<BrowserReport, BrowserError> {
        let report = self.registry.close(session_id)?;
        if let Ok(mut owners) = self.owners.lock() {
            owners.remove(session_id);
        }
        Ok(public_report(report))
    }

    /// Creating connection of a session, if it is still live.
    pub fn owner(&self, session_id: &str) -> Option<u64> {
        self.owners.lock().ok()?.get(session_id).copied()
    }

    /// Live session reports (`browser.list` / `browser.status`; daemon-wide
    /// view — the daemon is the resource authority, and the Shell restore
    /// flow needs to see every surviving session, M6-C4).
    pub fn reports(&self) -> Vec<BrowserReport> {
        self.registry
            .list()
            .into_iter()
            .filter(|session| session.state != SessionState::Closed)
            .map(public_report)
            .collect()
    }

    /// Live session count (daemon.status resources.browsers).
    pub fn session_count(&self) -> usize {
        self.reports().len()
    }
}

// ----------------------------------------------------------------------
// Envelope handlers
// ----------------------------------------------------------------------

/// `browser.create`: register a session in the daemon registry (the
/// state authority) and return its report. The Shell creates the render
/// window from the returned session id (rendering is Shell-owned,
/// ADR-0019 decision 2); the attach/navigate sync protocol is M6-C4.
pub fn handle_create(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: BrowserCreateRequest = parse_request("browser.create", payload)?;
    if !request.is_valid() {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "browser.create request is malformed (schemaVersion must be 1 and mode must be human_surface)",
            false,
        ));
    }
    let report = ctx
        .browser
        .create(ctx.connection_id)
        .map_err(map_browser_error)?;
    // Session established → the creating connection subscribes; lifecycle
    // events route to it by session id (same pattern as terminal.create).
    ctx.events.subscribe(ctx.connection_id, &report.session_id);
    ctx.events
        .publish(&RouterEvent::Browser(BrowserLifecycleEvent::new(
            &report.session_id,
            BrowserLifecycleKind::Created,
        )));
    Ok(serde_json::to_value(report).expect("browser report serializes"))
}

/// `browser.close`: validate, gate (owner), close the session and drop
/// the subscription.
pub fn handle_close(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: BrowserCloseRequest = parse_request("browser.close", payload)?;
    if request.schema_version != SCHEMA_VERSION || !valid_session_id(&request.session_id) {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "browser.close request is malformed",
            false,
        ));
    }
    check_owner(ctx, &request.session_id)?;
    ctx.browser
        .close(&request.session_id)
        .map_err(map_browser_error)?;
    // Publish before unsubscribing so the closed event still routes to
    // the owner's queue.
    ctx.events
        .publish(&RouterEvent::Browser(BrowserLifecycleEvent::new(
            &request.session_id,
            BrowserLifecycleKind::Closed,
        )));
    ctx.events
        .unsubscribe(ctx.connection_id, &request.session_id);
    Ok(serde_json::json!({}))
}

/// `browser.list`: live sessions as the registry view
/// (`{ browsers: [...] }`; the M6-B1 placeholder shape).
pub fn handle_list(ctx: &CapabilityContext) -> Result<serde_json::Value, DaemonMethodError> {
    let reports = ctx.browser.reports();
    Ok(serde_json::json!({ "browsers": reports }))
}

/// `browser.status`: live sessions as the daemon resource view
/// (`{ sessions, count }`, mirroring terminal.status).
pub fn handle_status(ctx: &CapabilityContext) -> Result<serde_json::Value, DaemonMethodError> {
    let sessions = ctx.browser.reports();
    Ok(serde_json::json!({ "sessions": sessions, "count": sessions.len() }))
}

// ----------------------------------------------------------------------
// Validation / gating helpers
// ----------------------------------------------------------------------

fn parse_request<T: DeserializeOwned>(
    method: &str,
    payload: &serde_json::Value,
) -> Result<T, DaemonMethodError> {
    serde_json::from_value(payload.clone()).map_err(|error| {
        failed(
            ErrorCode::MalformedMessage,
            format!("{method} payload does not match the request shape: {error}"),
            false,
        )
    })
}

/// Opaque session ids (AC-BRW-001): `^brw-[a-z0-9-]+$`, ≤ 64 chars.
pub fn valid_session_id(session_id: &str) -> bool {
    session_id.starts_with("brw-")
        && session_id.len() > 4
        && session_id.len() <= 64
        && session_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Session mutations require the invoking connection to be the creator
/// (the session is owned by the connection that established it).
fn check_owner(ctx: &CapabilityContext, session_id: &str) -> Result<(), DaemonMethodError> {
    match ctx.browser.owner(session_id) {
        None => Err(failed(
            ErrorCode::Unavailable,
            "browser session is unknown or already closed",
            false,
        )),
        Some(owner) if owner != ctx.connection_id => Err(failed(
            ErrorCode::NotProcessOwner,
            "browser session is owned by another connection",
            false,
        )),
        Some(_) => Ok(()),
    }
}

fn failed(code: ErrorCode, message: impl Into<String>, retryable: bool) -> DaemonMethodError {
    DaemonMethodError::MethodFailed {
        code,
        message: message.into(),
        retryable,
    }
}

/// Provider error → envelope error contract (the daemon keeps the
/// registry's error semantics; unknown/closed sessions are UNAVAILABLE,
/// mirroring the terminal mapping).
fn map_browser_error(error: BrowserError) -> DaemonMethodError {
    match error {
        BrowserError::NotFound => failed(
            ErrorCode::Unavailable,
            "browser session is unknown or already closed",
            false,
        ),
        BrowserError::Closed => failed(
            ErrorCode::Unavailable,
            "browser session is already closed",
            false,
        ),
        BrowserError::InvalidUrl(_) => failed(
            ErrorCode::MalformedMessage,
            "url rejected by the browser navigation policy",
            false,
        ),
        BrowserError::Other => failed(
            ErrorCode::Unavailable,
            "browser provider state is unavailable",
            true,
        ),
    }
}

fn public_report(session: BrowserSession) -> BrowserReport {
    BrowserReport {
        schema_version: SCHEMA_VERSION,
        session_id: session.session_id,
        state: session.state.as_str().to_string(),
        mode: "human_surface",
        current_url: session.current_url,
        created_at_unix_ms: session.created_at_unix_ms,
        last_activity_unix_ms: session.last_activity_unix_ms,
        error: session.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_request(mode: &str) -> BrowserCreateRequest {
        BrowserCreateRequest {
            schema_version: SCHEMA_VERSION,
            mode: mode.to_string(),
        }
    }

    #[test]
    fn create_request_validation_matrix() {
        assert!(create_request("human_surface").is_valid());
        // Unsupported mode (schema const).
        assert!(!create_request("agent_automation").is_valid());
        // Wrong schema version.
        let mut bad = create_request("human_surface");
        bad.schema_version = 2;
        assert!(!bad.is_valid());
    }

    #[test]
    fn session_id_validation() {
        assert!(valid_session_id("brw-1787000000000-1"));
        assert!(valid_session_id(&format!("brw-{}-1", now_unix_ms())));
        assert!(!valid_session_id("abc"));
        assert!(!valid_session_id("brw-"));
        assert!(!valid_session_id("brw-ABC"));
        assert!(!valid_session_id(&"brw-a".repeat(40)));
        assert!(!valid_session_id("brw-a b"));
    }

    #[test]
    fn payload_shapes_are_deny_unknown_fields() {
        // browser-create-request.valid.json shape parses.
        let parsed: BrowserCreateRequest =
            serde_json::from_value(json!({ "schemaVersion": 1, "mode": "human_surface" }))
                .expect("fixture shape");
        assert!(parsed.is_valid());

        // Unknown fields are rejected (additionalProperties: false).
        assert!(
            serde_json::from_value::<BrowserCreateRequest>(json!({
                "schemaVersion": 1,
                "mode": "human_surface",
                "sneaky": true,
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<BrowserCloseRequest>(json!({
                "schemaVersion": 1,
                "sessionId": "brw-1-1",
                "extra": 1,
            }))
            .is_err()
        );
    }

    #[test]
    fn host_create_close_and_live_view() {
        let host = BrowserHost::new();
        let created = host.create(7).expect("create");
        assert!(created.session_id.starts_with("brw-"));
        assert_eq!(created.state, "created");
        assert_eq!(created.mode, "human_surface");
        assert_eq!(host.owner(&created.session_id), Some(7));
        assert_eq!(host.session_count(), 1);
        assert_eq!(host.reports().len(), 1);

        let closed = host.close(&created.session_id).expect("close");
        assert_eq!(closed.state, "closed");
        assert_eq!(host.owner(&created.session_id), None);
        assert_eq!(host.session_count(), 0);
        assert!(
            host.reports().is_empty(),
            "closed sessions leave the live view"
        );
        // The registry keeps the closed session for audit.
        assert_eq!(
            host.registry()
                .get(&created.session_id)
                .expect("closed session stays queryable")
                .state,
            SessionState::Closed
        );
        // A second close is rejected by the registry.
        assert!(matches!(
            host.close(&created.session_id),
            Err(BrowserError::Closed)
        ));
    }

    #[test]
    fn lifecycle_event_wire_shapes() {
        let event =
            BrowserLifecycleEvent::new("brw-1787000000000-1", BrowserLifecycleKind::Created);
        assert_eq!(event.kind.as_str(), "created");
        assert_eq!(event.kind.event_method(), BROWSER_SESSION_CREATED_EVENT);
        let payload = BrowserEventPayload::from(&event);
        assert_eq!(payload.schema_version, SCHEMA_VERSION);
        assert_eq!(payload.session_id, "brw-1787000000000-1");
        assert_eq!(payload.kind, "created");
        assert_eq!(payload.url, None);

        let closed =
            BrowserLifecycleEvent::new("brw-1787000000000-1", BrowserLifecycleKind::Closed);
        assert_eq!(closed.kind.as_str(), "closed");
        assert_eq!(closed.kind.event_method(), BROWSER_SESSION_CLOSED_EVENT);
        assert_eq!(BrowserEventPayload::from(&closed).kind, "closed");
    }
}
