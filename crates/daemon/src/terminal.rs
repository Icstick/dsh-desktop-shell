//! Terminal capability of the daemon (M6-C1): the daemon **really owns**
//! the PTY registry (ADR-0019 decision 3 — PTY registry migrates into the
//! daemon; cross-Shell-restart resource survival, ADR-0008).
//!
//! Wire contract: request/report/event shapes mirror
//! `specs/terminal/*.schema.json` (TerminalCreateRequest/Write/Resize/
//! Close, TerminalReport, TerminalOutputEvent) and the envelope methods
//! are the namespaced `terminal.create` / `terminal.write` /
//! `terminal.resize` / `terminal.close` / `terminal.status` form (the
//! envelope method pattern `^[a-z][a-z0-9._-]+$`).
//!
//! Authorization (M6-C1 decision, mirrors apps/desktop terminal.rs):
//!
//! - **human sessions** (`mode: human_surface`) never touch the broker:
//!   the authenticated local-transport connection (credential handshake
//!   + negotiated terminal capability) IS the authorization;
//! - **agent sessions** (`mode: agent_automation`) gate every operation
//!   through the broker dispatch gate (`Broker::enforce_dispatch`,
//!   ADR-0014/0018 decision 7): `terminal.create` validates the carried
//!   agent facts (owner/generation/scope/live lease), and every later
//!   mutation re-validates the recorded binding (human takeover revokes
//!   the lease → fail-closed).
//!
//! Sessions are additionally owned by the creating connection: write/
//! resize/close from another connection are rejected (NOT_PROCESS_OWNER)
//! — in the multi-connection daemon world an opaque session id alone is
//! not an access token (AC-TERM-002 is preserved for the surface).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use dsh_supervisor::{BrokerError, CapabilityId, Scope};
use dsh_terminal_provider::{PtyError, PtyRegistry, PtyReport};

use crate::capabilities::{
    CapabilityContext, DaemonMethodError, TERMINAL_API_VERSION, TERMINAL_KIND,
};
use crate::envelope::ErrorCode;

/// Schema version of every terminal wire payload (specs/terminal).
pub const SCHEMA_VERSION: u8 = 1;

/// Event method of the terminal output stream (envelope Event; the M5
/// surface event was `terminal://output`, which is not a valid envelope
/// method — the namespaced `terminal.output` is).
pub const TERMINAL_OUTPUT_EVENT: &str = "terminal.output";

/// Output drain interval of the terminal bridge (registry → router).
pub const EVENT_DRAIN_INTERVAL: Duration = Duration::from_millis(30);

/// The broker capability id the terminal gate enforces against
/// (terminal.dsh-desktop.local/v1alpha1 + Terminal).
pub fn terminal_capability() -> CapabilityId {
    CapabilityId::new(TERMINAL_API_VERSION, TERMINAL_KIND)
}

/// Current time in unix milliseconds (output event timestamps).
pub fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ----------------------------------------------------------------------
// Wire types (specs/terminal/*.schema.json)
// ----------------------------------------------------------------------

/// `terminal.create` request (terminal-create-request.schema.json).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalCreateRequest {
    pub schema_version: u8,
    pub mode: String,
    pub cols: u16,
    pub rows: u16,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub agent: Option<TerminalAgentIdentity>,
}

impl TerminalCreateRequest {
    /// Cross-mode is fail-closed: a human session never carries agent
    /// authorization facts, and an agent session must carry them.
    pub fn is_valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && match self.mode.as_str() {
                "human_surface" => self.agent.is_none(),
                "agent_automation" => self.agent.as_ref().is_some_and(|agent| agent.is_valid()),
                _ => false,
            }
    }
}

/// Agent authorization facts carried by an agent_automation create
/// (specs/terminal terminal-create-request.schema.json `agent` object).
///
/// They mirror the broker grant facts the agent received in negotiation
/// (M5-E1 bridge: the daemon Hello maps into broker grants owned by
/// `component-facet`, the wire-compatible owner form). The broker gate
/// validates them against the live
/// grant + lease at create; the session binding records them so every
/// later mutation dispatches with the same owner/generation/scope.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalAgentIdentity {
    pub agent_id: String,
    pub activation_id: String,
    pub generation: u64,
    pub scope: TerminalAgentScope,
}

impl TerminalAgentIdentity {
    fn is_valid(&self) -> bool {
        valid_agent_token(&self.agent_id)
            && valid_agent_token(&self.activation_id)
            && self.generation >= 1
            && self.scope.is_valid()
    }
}

/// Wire shape of the grant scope (schema `scope`; camelCase), converted
/// to the broker `Scope` for enforcement.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalAgentScope {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
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

    fn to_broker_scope(&self) -> Scope {
        Scope {
            session_id: self.session_id.clone(),
            workspace: self.workspace.clone(),
            domains: self.domains.clone(),
            resources: self.resources.clone(),
        }
    }
}

/// `terminal.write` request (terminal-write-request.schema.json).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalWriteRequest {
    pub schema_version: u8,
    pub session_id: String,
    pub data: String,
}

/// `terminal.resize` request (terminal-resize-request.schema.json).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalResizeRequest {
    pub schema_version: u8,
    pub session_id: String,
    pub cols: u16,
    pub rows: u16,
}

/// `terminal.close` request (terminal-close-request.schema.json).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalCloseRequest {
    pub schema_version: u8,
    pub session_id: String,
}

/// `terminal.status` / create / resize report entry
/// (terminal-report.schema.json).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalReport {
    pub schema_version: u8,
    pub session_id: String,
    pub state: String,
    pub mode: String,
    pub cols: u16,
    pub rows: u16,
    pub created_at_unix_ms: u64,
    pub last_activity_unix_ms: Option<u64>,
    pub error: Option<String>,
}

/// One output event pushed to the session subscriber
/// (terminal-output-event.schema.json; the envelope Event payload).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputEvent {
    pub schema_version: u8,
    pub session_id: String,
    pub seq: u64,
    pub data: String,
    pub timestamp_unix_ms: u64,
}

/// Immutable agent ownership record of an agent_automation session.
///
/// ADR-0018 decision 1 (activation ownership): a session belongs to
/// exactly one agent activation; the recorded facts are the ones the
/// broker gate validates on every mutation of the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionBinding {
    pub agent_id: String,
    pub activation_id: String,
    pub generation: u64,
    pub scope: Scope,
}

// ----------------------------------------------------------------------
// TerminalHost: PTY registry + session bookkeeping
// ----------------------------------------------------------------------

/// Daemon-owned terminal state: the PTY registry plus the session
/// bookkeeping (agent bindings, connection ownership). One host per
/// daemon, Arc-shared with the event bridge thread and the capability
/// handlers.
pub struct TerminalHost {
    registry: PtyRegistry,
    /// Agent ownership of agent_automation sessions (opaque id ->
    /// binding).
    bindings: Mutex<HashMap<String, AgentSessionBinding>>,
    /// Creating connection per session (mutation ownership).
    owners: Mutex<HashMap<String, u64>>,
}

impl Default for TerminalHost {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalHost {
    pub fn new() -> Self {
        Self {
            registry: PtyRegistry::new(),
            bindings: Mutex::new(HashMap::new()),
            owners: Mutex::new(HashMap::new()),
        }
    }

    /// The PTY registry (the event bridge drains it).
    pub fn registry(&self) -> &PtyRegistry {
        &self.registry
    }

    /// Create a PTY from a validated request and record the session
    /// bookkeeping. Geometry/shell errors surface as provider errors.
    pub fn create(
        &self,
        request: &TerminalCreateRequest,
        owner: u64,
        binding: Option<AgentSessionBinding>,
    ) -> Result<TerminalReport, PtyError> {
        let report = self.registry.create(
            request.shell.as_deref(),
            request.cols,
            request.rows,
            request.cwd.as_deref(),
        )?;
        self.record(&report.session_id, owner, binding);
        Ok(public_report(report, &request.mode))
    }

    fn record(&self, session_id: &str, owner: u64, binding: Option<AgentSessionBinding>) {
        if let Ok(mut owners) = self.owners.lock() {
            owners.insert(session_id.to_string(), owner);
        }
        if let Some(binding) = binding
            && let Ok(mut bindings) = self.bindings.lock()
        {
            bindings.insert(session_id.to_string(), binding);
        }
    }

    /// Drop the bookkeeping of a closed session.
    pub fn forget(&self, session_id: &str) {
        if let Ok(mut bindings) = self.bindings.lock() {
            bindings.remove(session_id);
        }
        if let Ok(mut owners) = self.owners.lock() {
            owners.remove(session_id);
        }
    }

    /// Creating connection of a session, if it is still live.
    pub fn owner(&self, session_id: &str) -> Option<u64> {
        self.owners.lock().ok()?.get(session_id).copied()
    }

    /// Agent binding of a session (agent_automation only).
    pub fn binding(&self, session_id: &str) -> Option<AgentSessionBinding> {
        self.bindings.lock().ok()?.get(session_id).cloned()
    }

    /// Mode of an existing session.
    pub fn mode_for(&self, session_id: &str) -> &'static str {
        let is_agent = self
            .bindings
            .lock()
            .map(|bindings| bindings.contains_key(session_id))
            .unwrap_or(false);
        if is_agent {
            "agent_automation"
        } else {
            "human_surface"
        }
    }

    /// Live session count (daemon.status resources.terminals).
    pub fn session_count(&self) -> usize {
        self.registry.list().len()
    }

    /// All live session reports (terminal.status; daemon-wide view — the
    /// daemon is the resource authority, and the Shell restore flow
    /// needs to see every surviving session).
    pub fn reports(&self) -> Vec<TerminalReport> {
        self.registry
            .list()
            .into_iter()
            .map(|report| {
                let mode = self.mode_for(&report.session_id);
                public_report(report, mode)
            })
            .collect()
    }
}

// ----------------------------------------------------------------------
// Envelope handlers
// ----------------------------------------------------------------------

/// `terminal.create`: spawn the PTY (human: credential-authorized;
/// agent: broker gate on the carried facts), subscribe the creating
/// connection to the session output, and return the report.
pub fn handle_create(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: TerminalCreateRequest = parse_request("terminal.create", payload)?;
    if !request.is_valid() {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "terminal.create request is malformed or the mode is not supported (human_surface must not carry agent facts; agent_automation requires them)",
            false,
        ));
    }
    let binding = request.agent.as_ref().map(agent_binding_from);
    if let Some(binding) = &binding {
        // AC-TERM-001: agent_automation create requires a live terminal
        // grant + lease for this agent activation (fail-closed; an agent
        // with nothing granted is UNAUTHORIZED).
        gate_agent(ctx, binding)?;
    }
    let report = ctx
        .terminal
        .create(&request, ctx.connection_id, binding)
        .map_err(map_pty_error)?;
    // Session established → the creating connection subscribes; output
    // events route to it by session id.
    ctx.events.subscribe(ctx.connection_id, &report.session_id);
    Ok(serde_json::to_value(report).expect("terminal report serializes"))
}

/// `terminal.write`: validate, gate (owner + agent binding), forward to
/// the PTY stdin.
pub fn handle_write(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: TerminalWriteRequest = parse_request("terminal.write", payload)?;
    if request.schema_version != SCHEMA_VERSION
        || !valid_session_id(&request.session_id)
        || request.data.is_empty()
    {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "terminal.write request is malformed",
            false,
        ));
    }
    check_owner(ctx, &request.session_id)?;
    if let Some(binding) = ctx.terminal.binding(&request.session_id) {
        gate_agent(ctx, &binding)?;
    }
    ctx.terminal
        .registry()
        .write(&request.session_id, &request.data)
        .map_err(map_pty_error)?;
    Ok(serde_json::json!({}))
}

/// `terminal.resize`: validate, gate, resize the pseudo console, return
/// the updated report.
pub fn handle_resize(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: TerminalResizeRequest = parse_request("terminal.resize", payload)?;
    if request.schema_version != SCHEMA_VERSION || !valid_session_id(&request.session_id) {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "terminal.resize request is malformed",
            false,
        ));
    }
    check_owner(ctx, &request.session_id)?;
    if let Some(binding) = ctx.terminal.binding(&request.session_id) {
        gate_agent(ctx, &binding)?;
    }
    let report = ctx
        .terminal
        .registry()
        .resize(&request.session_id, request.cols, request.rows)
        .map_err(map_pty_error)?;
    let mode = ctx.terminal.mode_for(&request.session_id);
    Ok(serde_json::to_value(public_report(report, mode)).expect("terminal report serializes"))
}

/// `terminal.close`: validate, gate, terminate the session and drop the
/// subscription.
pub fn handle_close(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: TerminalCloseRequest = parse_request("terminal.close", payload)?;
    if request.schema_version != SCHEMA_VERSION || !valid_session_id(&request.session_id) {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "terminal.close request is malformed",
            false,
        ));
    }
    check_owner(ctx, &request.session_id)?;
    if let Some(binding) = ctx.terminal.binding(&request.session_id) {
        gate_agent(ctx, &binding)?;
    }
    // Provider teardown workaround (M6-C1): `PtyRegistry::close` can
    // deadlock when the provider output reader holds a pending
    // synchronous `ReadFile` with no data in flight — Windows
    // `CloseHandle` waits for the pending I/O while the read waits for
    // data (child idle at the prompt). A same-geometry resize forces the
    // ConPTY to flush, unblocking the reader (verified 10/10 in a
    // standalone repro). The root-cause fix belongs in
    // `crates/terminal-provider` (close the child stdin end before the
    // output handle); this daemon-side workaround keeps the provider
    // untouched.
    if let Ok(report) = ctx.terminal.registry().report(&request.session_id) {
        let _ = ctx
            .terminal
            .registry()
            .resize(&request.session_id, report.cols, report.rows);
    }
    ctx.terminal
        .registry()
        .close(&request.session_id)
        .map_err(map_pty_error)?;
    ctx.terminal.forget(&request.session_id);
    ctx.events
        .unsubscribe(ctx.connection_id, &request.session_id);
    Ok(serde_json::json!({}))
}

/// `terminal.status`: all live sessions (daemon-wide resource view).
pub fn handle_status(ctx: &CapabilityContext) -> Result<serde_json::Value, DaemonMethodError> {
    let sessions = ctx.terminal.reports();
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

/// Opaque session ids (AC-TERM-002): `^pty-[a-z0-9-]+$`, ≤ 64 chars.
pub fn valid_session_id(session_id: &str) -> bool {
    session_id.starts_with("pty-")
        && session_id.len() <= 64
        && session_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_agent_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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

fn agent_binding_from(identity: &TerminalAgentIdentity) -> AgentSessionBinding {
    AgentSessionBinding {
        agent_id: identity.agent_id.clone(),
        activation_id: identity.activation_id.clone(),
        generation: identity.generation,
        scope: identity.scope.to_broker_scope(),
    }
}

/// The ADR-0014 dispatch gate for agent facts (owner, generation, scope
/// coverage, live lease).
fn gate_agent(
    ctx: &CapabilityContext,
    binding: &AgentSessionBinding,
) -> Result<(), DaemonMethodError> {
    let broker = ctx
        .broker
        .lock()
        .map_err(|_| failed(ErrorCode::Unavailable, "broker state is unavailable", true))?;
    broker
        .enforce_dispatch(
            &terminal_capability(),
            &binding.agent_id,
            binding.generation,
            &binding.scope,
        )
        .map_err(map_broker_error)
}

/// Session mutations require the invoking connection to be the creator
/// (the session is owned by the connection that established it).
fn check_owner(ctx: &CapabilityContext, session_id: &str) -> Result<(), DaemonMethodError> {
    match ctx.terminal.owner(session_id) {
        None => Err(failed(
            ErrorCode::Unavailable,
            "terminal session is unknown or already closed",
            false,
        )),
        Some(owner) if owner != ctx.connection_id => Err(failed(
            ErrorCode::NotProcessOwner,
            "terminal session is owned by another connection",
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

/// Provider error → envelope error contract.
fn map_pty_error(error: PtyError) -> DaemonMethodError {
    match error {
        PtyError::InvalidShell => failed(
            ErrorCode::MalformedMessage,
            "shell selection is not supported",
            false,
        ),
        PtyError::SpawnUnavailable => failed(
            ErrorCode::Unavailable,
            "pseudo console or shell spawn failed",
            true,
        ),
        PtyError::UnknownSession => failed(
            ErrorCode::Unavailable,
            "terminal session is unknown or already closed",
            false,
        ),
        PtyError::InvalidGeometry => failed(
            ErrorCode::MalformedMessage,
            "terminal geometry is outside schema bounds",
            false,
        ),
        PtyError::WriteTooLarge => failed(
            ErrorCode::MalformedMessage,
            "terminal write exceeds the schema bound",
            false,
        ),
        PtyError::Io => failed(ErrorCode::Unavailable, "terminal I/O failed", true),
        PtyError::StateUnavailable => failed(
            ErrorCode::Unavailable,
            "terminal state is unavailable",
            true,
        ),
    }
}

/// Broker gate rejection → envelope error contract (mirrors the M5
/// ERROR_MODEL.md mapping: an agent with no terminal grant/lease at all
/// is UNAUTHORIZED, not UNAVAILABLE).
fn map_broker_error(error: BrokerError) -> DaemonMethodError {
    match error {
        BrokerError::UnknownCapability
        | BrokerError::NotGranted
        | BrokerError::LeaseExpired
        | BrokerError::LeaseRevoked
        | BrokerError::ScopeMismatch => failed(
            ErrorCode::Unauthorized,
            "agent is not authorized for terminal automation",
            false,
        ),
        BrokerError::UnknownProvider => failed(
            ErrorCode::Unavailable,
            "terminal provider is not registered",
            true,
        ),
        BrokerError::GenerationMismatch => failed(
            ErrorCode::StaleGeneration,
            "agent terminal request carries a stale generation",
            false,
        ),
        BrokerError::Conflict => {
            failed(ErrorCode::Conflict, "terminal broker state conflict", false)
        }
    }
}

fn public_report(report: PtyReport, mode: &str) -> TerminalReport {
    TerminalReport {
        schema_version: SCHEMA_VERSION,
        session_id: report.session_id,
        state: report.state.to_string(),
        mode: mode.to_string(),
        cols: report.cols,
        rows: report.rows,
        created_at_unix_ms: report.created_at_unix_ms,
        last_activity_unix_ms: report.last_activity_unix_ms,
        error: report.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_request(mode: &str, cols: u16, rows: u16) -> TerminalCreateRequest {
        TerminalCreateRequest {
            schema_version: SCHEMA_VERSION,
            mode: mode.to_string(),
            cols,
            rows,
            shell: None,
            cwd: None,
            agent: None,
        }
    }

    fn agent_identity() -> TerminalAgentIdentity {
        TerminalAgentIdentity {
            agent_id: "agent-1".into(),
            activation_id: "act-0001".into(),
            generation: 1,
            scope: TerminalAgentScope {
                session_id: Some("session-1".into()),
                workspace: Some("ws-a".into()),
                domains: Vec::new(),
                resources: Vec::new(),
            },
        }
    }

    #[test]
    fn create_request_validation_matrix() {
        assert!(create_request("human_surface", 80, 24).is_valid());

        // Unsupported mode.
        assert!(!create_request("immersive", 80, 24).is_valid());
        // Wrong schema version.
        let mut bad = create_request("human_surface", 80, 24);
        bad.schema_version = 2;
        assert!(!bad.is_valid());
        // agent_automation without identity.
        assert!(!create_request("agent_automation", 80, 24).is_valid());
        // human_surface carrying agent facts (cross-mode fail-closed).
        let mut human = create_request("human_surface", 80, 24);
        human.agent = Some(agent_identity());
        assert!(!human.is_valid());
        // agent_automation with valid identity.
        let mut agent = create_request("agent_automation", 80, 24);
        agent.agent = Some(agent_identity());
        assert!(agent.is_valid());
        // generation 0 is malformed.
        let mut agent = create_request("agent_automation", 80, 24);
        agent.agent = Some(agent_identity());
        agent.agent.as_mut().expect("agent").generation = 0;
        assert!(!agent.is_valid());
        // empty scope is malformed.
        let mut agent = create_request("agent_automation", 80, 24);
        let mut identity = agent_identity();
        identity.scope = TerminalAgentScope {
            session_id: None,
            workspace: None,
            domains: Vec::new(),
            resources: Vec::new(),
        };
        agent.agent = Some(identity);
        assert!(!agent.is_valid());
    }

    #[test]
    fn session_id_validation() {
        assert!(valid_session_id("pty-1234567890"));
        assert!(valid_session_id(&format!("pty-{}-1", now_unix_ms())));
        assert!(!valid_session_id("abc"));
        assert!(!valid_session_id("pty-ABC"));
        assert!(!valid_session_id(&"pty-a".repeat(40)));
        assert!(!valid_session_id("pty-a b"));
    }

    #[test]
    fn payload_shapes_are_deny_unknown_fields() {
        // terminal-create-request.valid.json shape parses.
        let parsed: TerminalCreateRequest = serde_json::from_value(json!({
            "schemaVersion": 1,
            "mode": "human_surface",
            "cols": 120,
            "rows": 32,
            "shell": "powershell",
        }))
        .expect("fixture shape");
        assert!(parsed.is_valid());

        // Unknown fields are rejected (additionalProperties: false).
        assert!(
            serde_json::from_value::<TerminalCreateRequest>(json!({
                "schemaVersion": 1,
                "mode": "human_surface",
                "cols": 120,
                "rows": 32,
                "sneaky": true,
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<TerminalWriteRequest>(json!({
                "schemaVersion": 1,
                "sessionId": "pty-abc123",
                "data": "dir\r\n",
                "extra": 1,
            }))
            .is_err()
        );
    }
}
