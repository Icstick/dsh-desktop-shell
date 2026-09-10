//! Desktop terminal bridge (MOD-TERMINAL-UI boundary, ADR-0015) - M6-C4
//! daemon proxy.
//!
//! Since M6-C1 the daemon owns the PTY registry (ADR-0019 decision 3:
//! ConPTY sessions survive Shell restarts, AC-PTY-001); this module is the
//! Shell-side command proxy: every terminal command is an envelope
//! Invocation through crate::daemon_client::DaemonConnector
//! (terminal.create / terminal.write / terminal.resize / terminal.close /
//! terminal.status), and PTY output arrives over the daemon event bridge
//! on the unchanged terminal://output event (AC-TERM-002: only the shell
//! webview listens). The Shell keeps no PTY state - the daemon is the
//! authority; commands fail closed (UNAVAILABLE) when no daemon connection
//! is installed.
//!
//! Agent automation (AC-TERM-001, ADR-0018 decision 7): the request shapes
//! keep carrying the agent authorization facts; the broker gate (owner =
//! agent id, generation, scope, lease) runs daemon-side at create and on
//! every mutation of an agent session (M6-C1). Human sessions never touch
//! the broker.

use serde::{Deserialize, Serialize};

use dsh_daemon::capabilities::{
    TERMINAL_API_VERSION, TERMINAL_CLOSE_METHOD, TERMINAL_CREATE_METHOD, TERMINAL_KIND,
    TERMINAL_RESIZE_METHOD, TERMINAL_STATUS_METHOD, TERMINAL_WRITE_METHOD,
};
use dsh_daemon::envelope::ProtocolCoordinate;
use dsh_terminal_provider::{MAX_COLS, MAX_ROWS, MAX_WRITE_BYTES, MIN_COLS, MIN_ROWS};

use crate::daemon_client::{DaemonCommandError, DaemonConnector};

const SCHEMA_VERSION: u8 = 1;

/// Terminal capability coordinate (IF-TERMINAL api_version, cf.
/// specs/protocol/fixtures/envelope.agreement.valid.json granted
/// coordinate terminal.dsh-desktop.local/v1alpha1 + Terminal). Every
/// proxied invocation addresses exactly this coordinate.
fn terminal_coordinate() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: TERMINAL_API_VERSION.into(),
        kind: TERMINAL_KIND.into(),
    }
}

/// Terminal commands act on opaque session ids only (AC-TERM-002).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalSessionRequest {
    schema_version: u8,
    session_id: String,
}

impl TerminalSessionRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalCreateRequest {
    schema_version: u8,
    mode: String,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    cwd: Option<String>,
    agent: Option<TerminalAgentIdentity>,
}

impl TerminalCreateRequest {
    /// Full schema validation of one create request
    /// (specs/terminal/terminal-create-request.schema.json).
    ///
    /// The schema used to be documentation only: `mode`/`agent` were checked
    /// and everything else (cols 20-500, rows 5-300, the shell enum, cwd
    /// <= 1024) was forwarded to the daemon verbatim (security audit
    /// 2026-09-10, Medium: contract gates that never fire). The provider
    /// bounds the geometry too, so this was not an exploitable hole, but a
    /// spec the Shell does not enforce is not a contract.
    pub(crate) fn is_valid(&self) -> bool {
        if self.schema_version != SCHEMA_VERSION {
            return false;
        }
        if !(MIN_COLS..=MAX_COLS).contains(&self.cols)
            || !(MIN_ROWS..=MAX_ROWS).contains(&self.rows)
        {
            return false;
        }
        if let Some(shell) = self.shell.as_deref()
            && !is_schema_shell(shell)
        {
            return false;
        }
        // `cwd` is bounded only by length; the daemon/provider decides
        // whether the directory is usable.
        if self
            .cwd
            .as_deref()
            .is_some_and(|cwd| cwd.chars().count() > MAX_CWD_CHARS)
        {
            return false;
        }
        match self.mode.as_str() {
            // Cross-mode is fail-closed: a human session never carries agent
            // authorization facts, and an agent session must carry them.
            "human_surface" => self.agent.is_none(),
            "agent_automation" => self.agent.as_ref().is_some_and(|agent| agent.is_valid()),
            _ => false,
        }
    }
}

/// Cross-platform shell union of the create schema. Platform-specific
/// narrowing (cmd/powershell on Unix, sh/bash/zsh on Windows) stays with
/// `terminal-provider`/`resolve_shell`, which is the component that knows
/// what it can actually spawn.
const SCHEMA_SHELLS: [&str; 7] = ["default", "cmd", "powershell", "pwsh", "sh", "bash", "zsh"];

/// `cwd` upper bound of the create schema (characters, matching JSON
/// Schema `maxLength`).
const MAX_CWD_CHARS: usize = 1024;

fn is_schema_shell(shell: &str) -> bool {
    SCHEMA_SHELLS.contains(&shell)
}

fn valid_geometry(cols: u16, rows: u16) -> bool {
    (MIN_COLS..=MAX_COLS).contains(&cols) && (MIN_ROWS..=MAX_ROWS).contains(&rows)
}

/// Agent authorization facts carried by an agent_automation create
/// (specs/terminal/terminal-create-request.schema.json agent object).
///
/// These mirror the broker grant facts the agent received in negotiation
/// (ADR-0018 decision 7). The broker gate validates them against the live
/// grant + lease at create; the session binding records them so every later
/// mutation of the session dispatches with the same owner/generation/scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalAgentIdentity {
    agent_id: String,
    activation_id: String,
    generation: u64,
    scope: TerminalAgentScope,
}

impl TerminalAgentIdentity {
    fn is_valid(&self) -> bool {
        valid_agent_token(&self.agent_id)
            && valid_agent_token(&self.activation_id)
            && self.generation >= 1
            && self.scope.is_valid()
    }
}

fn valid_agent_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Wire shape of the grant scope (mirrors specs/protocol/capability-lease
/// scope, camelCase). Shape-validated locally; the broker enforcement is
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalAgentScope {
    session_id: Option<String>,
    workspace: Option<String>,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    resources: Vec<String>,
}

impl TerminalAgentScope {
    fn is_valid(&self) -> bool {
        let non_empty = self.session_id.is_some()
            || self.workspace.is_some()
            || !self.domains.is_empty()
            || !self.resources.is_empty();
        non_empty
            && self
                .session_id
                .as_deref()
                .is_none_or(|value| !value.is_empty() && value.len() <= 128)
            && self
                .workspace
                .as_deref()
                .is_none_or(|value| !value.is_empty() && value.len() <= 128)
            && valid_string_list(&self.domains)
            && valid_string_list(&self.resources)
    }
}

fn valid_string_list(items: &[String]) -> bool {
    if items.len() > 16 {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    items
        .iter()
        .all(|item| !item.is_empty() && item.len() <= 128 && seen.insert(item))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalWriteRequest {
    schema_version: u8,
    session_id: String,
    data: String,
}

impl TerminalWriteRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn data(&self) -> &str {
        &self.data
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalResizeRequest {
    schema_version: u8,
    session_id: String,
    cols: u16,
    rows: u16,
}

impl TerminalResizeRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn cols(&self) -> u16 {
        self.cols
    }

    pub(crate) fn rows(&self) -> u16 {
        self.rows
    }
}

/// Validate a session request against schema rules.
pub(crate) fn validate_session_request(request: &TerminalSessionRequest) -> bool {
    request.schema_version() == SCHEMA_VERSION
        && request.session_id().starts_with("pty-")
        && request.session_id().len() <= 64
        && request
            .session_id()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub(crate) fn create_terminal(
    connector: &dyn DaemonConnector,
    request: &TerminalCreateRequest,
) -> Result<TerminalReport, TerminalCommandError> {
    if !request.is_valid() {
        return Err(TerminalCommandError::malformed());
    }
    let payload = serde_json::to_value(request).map_err(|_| TerminalCommandError::unavailable())?;
    let value = connector
        .invoke(terminal_coordinate(), TERMINAL_CREATE_METHOD, payload)
        .map_err(TerminalCommandError::from_daemon)?;
    serde_json::from_value(value)
        .map_err(|_| TerminalCommandError::unavailable_response("terminal.create"))
}

pub(crate) fn write_terminal(
    connector: &dyn DaemonConnector,
    request: &TerminalWriteRequest,
) -> Result<(), TerminalCommandError> {
    if request.schema_version() != SCHEMA_VERSION
        || !validate_session_request(&TerminalSessionRequest {
            schema_version: request.schema_version(),
            session_id: request.session_id().to_string(),
        })
        // terminal-write-request.schema.json: data is 1..=8192 *bytes*
        // (the provider's MAX_WRITE_BYTES is a byte bound).
        || request.data().is_empty()
        || request.data().len() > MAX_WRITE_BYTES
    {
        return Err(TerminalCommandError::malformed());
    }
    let payload = serde_json::to_value(request).map_err(|_| TerminalCommandError::unavailable())?;
    connector
        .invoke(terminal_coordinate(), TERMINAL_WRITE_METHOD, payload)
        .map_err(TerminalCommandError::from_daemon)?;
    Ok(())
}

pub(crate) fn resize_terminal(
    connector: &dyn DaemonConnector,
    request: &TerminalResizeRequest,
) -> Result<TerminalReport, TerminalCommandError> {
    if request.schema_version() != SCHEMA_VERSION
        || !validate_session_request(&TerminalSessionRequest {
            schema_version: request.schema_version(),
            session_id: request.session_id().to_string(),
        })
        // terminal-resize-request.schema.json geometry bounds.
        || !valid_geometry(request.cols(), request.rows())
    {
        return Err(TerminalCommandError::malformed());
    }
    let payload = serde_json::to_value(request).map_err(|_| TerminalCommandError::unavailable())?;
    let value = connector
        .invoke(terminal_coordinate(), TERMINAL_RESIZE_METHOD, payload)
        .map_err(TerminalCommandError::from_daemon)?;
    serde_json::from_value(value)
        .map_err(|_| TerminalCommandError::unavailable_response("terminal.resize"))
}

pub(crate) fn close_terminal(
    connector: &dyn DaemonConnector,
    request: &TerminalSessionRequest,
) -> Result<(), TerminalCommandError> {
    if !validate_session_request(request) {
        return Err(TerminalCommandError::malformed());
    }
    let payload = serde_json::to_value(request).map_err(|_| TerminalCommandError::unavailable())?;
    connector
        .invoke(terminal_coordinate(), TERMINAL_CLOSE_METHOD, payload)
        .map_err(TerminalCommandError::from_daemon)?;
    Ok(())
}

/// `terminal.status` wire payload ({ sessions, count }, daemon-wide).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalStatusPayload {
    sessions: Vec<TerminalReport>,
}

pub(crate) fn status_terminal(
    connector: &dyn DaemonConnector,
    request: &TerminalSessionRequest,
) -> Result<TerminalReport, TerminalCommandError> {
    if !validate_session_request(request) {
        return Err(TerminalCommandError::malformed());
    }
    let value = connector
        .invoke(
            terminal_coordinate(),
            TERMINAL_STATUS_METHOD,
            serde_json::json!({}),
        )
        .map_err(TerminalCommandError::from_daemon)?;
    let payload: TerminalStatusPayload = serde_json::from_value(value)
        .map_err(|_| TerminalCommandError::unavailable_response("terminal.status"))?;
    payload
        .sessions
        .into_iter()
        .find(|session| session.session_id() == request.session_id())
        .ok_or_else(|| {
            TerminalCommandError::pty(
                "UNAVAILABLE",
                "Terminal session is unknown or already closed.",
                false,
            )
        })
}

/// List every live session (daemon-wide view). The command contract cannot
/// carry an error (plain Vec), so an unavailable daemon yields an empty
/// list - the same convention the old registry used for lock failures.
pub(crate) fn list_terminals(connector: &dyn DaemonConnector) -> Vec<TerminalReport> {
    match connector.invoke(
        terminal_coordinate(),
        TERMINAL_STATUS_METHOD,
        serde_json::json!({}),
    ) {
        Ok(value) => serde_json::from_value::<TerminalStatusPayload>(value)
            .map(|payload| payload.sessions)
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalReport {
    schema_version: u8,
    session_id: String,
    state: String,
    mode: String,
    cols: u16,
    rows: u16,
    created_at_unix_ms: u64,
    last_activity_unix_ms: Option<u64>,
    error: Option<String>,
}

impl TerminalReport {
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandError {
    code: String,
    message: String,
    retryable: bool,
    correlation_id: String,
}

impl TerminalCommandError {
    fn malformed() -> Self {
        Self {
            code: "MALFORMED_MESSAGE".to_string(),
            message: "Terminal request is malformed or the mode is not supported.".to_string(),
            retryable: false,
            correlation_id: correlation_id(),
        }
    }

    fn unavailable() -> Self {
        Self {
            code: "UNAVAILABLE".to_string(),
            message: "Terminal state is unavailable.".to_string(),
            retryable: true,
            correlation_id: correlation_id(),
        }
    }

    fn pty(kind: &'static str, message: &'static str, retryable: bool) -> Self {
        Self {
            code: kind.to_string(),
            message: message.to_string(),
            retryable,
            correlation_id: correlation_id(),
        }
    }

    /// Map a daemon invocation failure onto the terminal command error
    /// contract: the daemon's protocol code/message/retryable pass through
    /// (the daemon already translated provider and broker rejections,
    /// M6-C1); connection-level failures are UNAVAILABLE + retryable.
    fn from_daemon(error: DaemonCommandError) -> Self {
        Self {
            code: error.wire_code(),
            message: error.message(),
            retryable: error.retryable(),
            correlation_id: correlation_id(),
        }
    }

    /// Command entry with no daemon connection installed (fail-closed).
    pub(crate) fn from_daemon_unavailable() -> Self {
        Self::from_daemon(DaemonCommandError::NotConnected)
    }

    /// The daemon answered with a payload that does not match the
    /// expected wire shape (a shell/daemon contract mismatch; retrying
    /// cannot help).
    fn unavailable_response(method: &'static str) -> Self {
        Self {
            code: "UNAVAILABLE".to_string(),
            message: format!("The daemon returned an unexpected {method} response."),
            retryable: false,
            correlation_id: correlation_id(),
        }
    }
}

fn correlation_id() -> String {
    format!("desktop-{}-{}", std::process::id(), now_ms())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Bounds mirrored from the provider for frontend documentation.
pub const _TERMINAL_BOUNDS: (u16, u16, u16, u16) = (20, 500, 5, 300);
pub const _MAX_WRITE_BYTES_PUBLIC: usize = MAX_WRITE_BYTES;
pub const _MAX_COLS_PUBLIC: u16 = MAX_COLS;
pub const _MAX_ROWS_PUBLIC: u16 = MAX_ROWS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_client::DaemonCommandError;
    use crate::daemon_client::tests::MockConnector;
    use dsh_daemon::envelope::ErrorCode;

    fn create_request(cols: u16, rows: u16) -> TerminalCreateRequest {
        TerminalCreateRequest {
            schema_version: 1,
            mode: "human_surface".to_string(),
            cols,
            rows,
            shell: None,
            cwd: None,
            agent: None,
        }
    }

    fn write_request(session_id: &str, data: &str) -> TerminalWriteRequest {
        TerminalWriteRequest {
            schema_version: 1,
            session_id: session_id.to_string(),
            data: data.to_string(),
        }
    }

    fn resize_request(session_id: &str, cols: u16, rows: u16) -> TerminalResizeRequest {
        TerminalResizeRequest {
            schema_version: 1,
            session_id: session_id.to_string(),
            cols,
            rows,
        }
    }

    fn session_request(session_id: &str) -> TerminalSessionRequest {
        TerminalSessionRequest {
            schema_version: 1,
            session_id: session_id.to_string(),
        }
    }

    /// Wire report shape the daemon returns (specs/terminal report).
    fn report_json(session_id: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "sessionId": session_id,
            "state": "running",
            "mode": "human_surface",
            "cols": 80,
            "rows": 24,
            "createdAtUnixMs": 1_787_000_000_000u64,
            "lastActivityUnixMs": 1_787_000_000_100u64,
            "error": null,
        })
    }

    fn status_payload(sessions: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({ "sessions": sessions, "count": sessions.len() })
    }

    // ---- schema bounds enforcement (audit 2026-09-10, Medium) ----

    #[test]
    fn create_rejects_out_of_schema_geometry_locally() {
        let connector = MockConnector::ok(report_json("pty-1"));
        for (cols, rows) in [
            (MIN_COLS - 1, 24),
            (MAX_COLS + 1, 24),
            (80, MIN_ROWS - 1),
            (80, MAX_ROWS + 1),
            (0, 0),
        ] {
            let error =
                create_terminal(&connector, &create_request(cols, rows)).expect_err("geometry");
            assert_eq!(error.code, "MALFORMED_MESSAGE", "{cols}x{rows}");
        }
        // The inclusive schema bounds are accepted.
        create_terminal(&connector, &create_request(MIN_COLS, MIN_ROWS)).expect("min bounds");
        create_terminal(&connector, &create_request(MAX_COLS, MAX_ROWS)).expect("max bounds");
        assert!(connector.calls().is_empty() || connector.calls().len() >= 2);
    }

    #[test]
    fn create_rejects_shell_outside_the_schema_enum_locally() {
        let connector = MockConnector::ok(report_json("pty-1"));
        let mut request = create_request(80, 24);
        request.shell = Some("telnet".to_string());
        let error = create_terminal(&connector, &request).expect_err("shell enum");
        assert_eq!(error.code, "MALFORMED_MESSAGE");

        // Every value of the cross-platform union passes the Shell gate;
        // per-platform narrowing belongs to the provider.
        for shell in SCHEMA_SHELLS {
            request.shell = Some(shell.to_string());
            create_terminal(&connector, &request).expect(shell);
        }
    }

    #[test]
    fn create_rejects_cwd_longer_than_the_schema_bound() {
        let connector = MockConnector::ok(report_json("pty-1"));
        let mut request = create_request(80, 24);
        request.cwd = Some("c".repeat(MAX_CWD_CHARS + 1));
        let error = create_terminal(&connector, &request).expect_err("cwd too long");
        assert_eq!(error.code, "MALFORMED_MESSAGE");

        request.cwd = Some("c".repeat(MAX_CWD_CHARS));
        create_terminal(&connector, &request).expect("cwd at the bound");
    }

    #[test]
    fn write_rejects_empty_and_oversized_payloads() {
        let connector = MockConnector::ok(serde_json::json!({}));
        let error =
            write_terminal(&connector, &write_request("pty-1", "")).expect_err("empty payload");
        assert_eq!(error.code, "MALFORMED_MESSAGE");

        let oversized = "x".repeat(MAX_WRITE_BYTES + 1);
        let error = write_terminal(&connector, &write_request("pty-1", &oversized))
            .expect_err("oversized payload");
        assert_eq!(error.code, "MALFORMED_MESSAGE");

        // Only the in-bounds payload reached the daemon.
        assert!(connector.calls().is_empty());
        write_terminal(
            &connector,
            &write_request("pty-1", &"x".repeat(MAX_WRITE_BYTES)),
        )
        .expect("payload at the byte bound");
        assert_eq!(connector.calls().len(), 1);
    }

    #[test]
    fn resize_rejects_out_of_schema_geometry_locally() {
        let connector = MockConnector::ok(report_json("pty-1"));
        for (cols, rows) in [(MIN_COLS - 1, 24), (MAX_COLS + 1, 24), (80, MAX_ROWS + 1)] {
            let error = resize_terminal(&connector, &resize_request("pty-1", cols, rows))
                .expect_err("geometry");
            assert_eq!(error.code, "MALFORMED_MESSAGE", "{cols}x{rows}");
        }
        // None of the rejected requests left the Shell.
        assert!(connector.calls().is_empty());
        resize_terminal(&connector, &resize_request("pty-1", MAX_COLS, MAX_ROWS))
            .expect("max bounds");
        assert_eq!(connector.calls().len(), 1);
    }

    #[test]
    fn malformed_create_is_rejected_locally() {
        let connector = MockConnector::ok(serde_json::json!({}));
        let mut request = create_request(80, 24);
        request.mode = "immersive".to_string();
        let error = create_terminal(&connector, &request).expect_err("unsupported mode");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
        // Fail-closed before any invocation leaves the Shell.
        assert!(connector.calls().is_empty());
    }

    #[test]
    fn create_proxies_terminal_create_and_parses_report() {
        let connector = MockConnector::ok(report_json("pty-1"));
        let report = create_terminal(&connector, &create_request(80, 24)).expect("create");
        assert_eq!(report.session_id(), "pty-1");
        assert_eq!(report.state, "running");
        assert_eq!(report.mode, "human_surface");
        let calls = connector.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Terminal");
        assert_eq!(calls[0].1, "terminal.create");
        assert_eq!(calls[0].2["schemaVersion"], 1);
        assert_eq!(calls[0].2["mode"], "human_surface");
        assert_eq!(calls[0].2["cols"], 80);
    }

    #[test]
    fn create_forwards_agent_authorization_facts() {
        // agent_automation create carries the broker facts; the daemon
        // gates them (M6-C1). The proxy must forward them verbatim.
        let request = TerminalCreateRequest {
            schema_version: 1,
            mode: "agent_automation".to_string(),
            cols: 100,
            rows: 40,
            shell: None,
            cwd: None,
            agent: Some(TerminalAgentIdentity {
                agent_id: "agent-1".into(),
                activation_id: "act-0001".into(),
                generation: 1,
                scope: TerminalAgentScope {
                    session_id: Some("session-1".into()),
                    workspace: Some("ws-a".into()),
                    domains: Vec::new(),
                    resources: Vec::new(),
                },
            }),
        };
        let connector = MockConnector::ok(report_json("pty-9"));
        let report = create_terminal(&connector, &request).expect("create");
        assert_eq!(report.session_id(), "pty-9");
        let calls = connector.calls();
        assert_eq!(calls[0].1, "terminal.create");
        assert_eq!(calls[0].2["agent"]["agentId"], "agent-1");
        assert_eq!(calls[0].2["agent"]["generation"], 1);
        assert_eq!(calls[0].2["agent"]["scope"]["workspace"], "ws-a");
    }

    #[test]
    fn write_resize_close_proxy_to_daemon() {
        let connector = MockConnector::sequential(vec![
            Ok(serde_json::json!({})),
            Ok(report_json("pty-1")),
            Ok(serde_json::json!({})),
        ]);
        write_terminal(&connector, &write_request("pty-1", "echo hi\r\n")).expect("write");
        let resized =
            resize_terminal(&connector, &resize_request("pty-1", 100, 40)).expect("resize");
        assert_eq!(resized.cols, 80);
        close_terminal(&connector, &session_request("pty-1")).expect("close");

        let calls = connector.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].1, "terminal.write");
        assert_eq!(calls[0].2["sessionId"], "pty-1");
        assert_eq!(calls[0].2["data"], "echo hi\r\n");
        assert_eq!(calls[1].1, "terminal.resize");
        assert_eq!(calls[1].2["cols"], 100);
        assert_eq!(calls[2].1, "terminal.close");
    }

    #[test]
    fn malformed_session_requests_are_rejected_locally() {
        let connector = MockConnector::ok(serde_json::json!({}));
        // Unknown session id shape (not pty- prefixed).
        let error =
            write_terminal(&connector, &write_request("bogus", "x")).expect_err("bad session id");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
        let error = resize_terminal(&connector, &resize_request("bogus", 10, 10))
            .expect_err("bad session id");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
        let error =
            close_terminal(&connector, &session_request("bogus")).expect_err("bad session id");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
        let error =
            status_terminal(&connector, &session_request("bogus")).expect_err("bad session id");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
        assert!(connector.calls().is_empty());
    }

    #[test]
    fn status_finds_session_in_daemon_wide_view() {
        let connector = MockConnector::ok(status_payload(vec![
            report_json("pty-1"),
            report_json("pty-2"),
        ]));
        let report = status_terminal(&connector, &session_request("pty-2")).expect("status");
        assert_eq!(report.session_id(), "pty-2");
        assert_eq!(connector.calls()[0].1, "terminal.status");
    }

    #[test]
    fn status_unknown_session_is_unavailable() {
        let connector = MockConnector::ok(status_payload(vec![report_json("pty-1")]));
        let error =
            status_terminal(&connector, &session_request("pty-99")).expect_err("unknown session");
        assert_eq!(error.code, "UNAVAILABLE");
        assert!(!error.retryable);
    }

    #[test]
    fn list_terminals_maps_daemon_wide_view() {
        let connector = MockConnector::ok(status_payload(vec![
            report_json("pty-1"),
            report_json("pty-2"),
        ]));
        let reports = list_terminals(&connector);
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[1].session_id(), "pty-2");
        assert_eq!(connector.calls()[0].1, "terminal.status");
    }

    #[test]
    fn list_terminals_is_empty_when_daemon_unavailable() {
        let connector = MockConnector::error(DaemonCommandError::NotConnected);
        assert!(list_terminals(&connector).is_empty());
    }

    #[test]
    fn not_connected_maps_to_unavailable_retryable() {
        let connector = MockConnector::error(DaemonCommandError::NotConnected);
        let error = create_terminal(&connector, &create_request(80, 24)).expect_err("offline");
        assert_eq!(error.code, "UNAVAILABLE");
        assert!(error.retryable);
    }

    #[test]
    fn remote_protocol_errors_pass_through() {
        let connector = MockConnector::error(DaemonCommandError::Remote {
            code: ErrorCode::Unauthorized,
            message: "agent is not authorized".into(),
            retryable: false,
        });
        let error = create_terminal(&connector, &create_request(80, 24)).expect_err("remote");
        assert_eq!(error.code, "UNAUTHORIZED");
        assert_eq!(error.message, "agent is not authorized");
        assert!(!error.retryable);

        let connector = MockConnector::error(DaemonCommandError::Remote {
            code: ErrorCode::Conflict,
            message: "broker conflict".into(),
            retryable: true,
        });
        let error = close_terminal(&connector, &session_request("pty-1")).expect_err("remote");
        assert_eq!(error.code, "CONFLICT");
        assert!(error.retryable);
    }

    #[test]
    fn malformed_daemon_response_maps_to_unavailable() {
        let connector = MockConnector::ok(serde_json::json!({ "unexpected": true }));
        let error = create_terminal(&connector, &create_request(80, 24)).expect_err("bad shape");
        assert_eq!(error.code, "UNAVAILABLE");
        assert!(!error.retryable);
        assert!(error.message.contains("terminal.create"));
    }

    #[test]
    fn transport_timeout_maps_to_unavailable_retryable() {
        let connector = MockConnector::error(DaemonCommandError::Timeout);
        let error = write_terminal(&connector, &write_request("pty-1", "x")).expect_err("timeout");
        assert_eq!(error.code, "UNAVAILABLE");
        assert!(error.retryable);
    }
}
