//! Desktop terminal bridge (MOD-TERMINAL-UI boundary, ADR-0015).
//!
//! Owns the PtyRegistry for the app, relays ConPTY output to the Shell
//! WebView via Tauri events (AC-TERM-002: only the shell webview listens),
//! and exposes generation-free create/write/resize/close/status commands.
//! The PTY process tree is Desktop-owned, so Managed DSH stop/restart never
//! affects terminal sessions (AC-PTY-001).
//!
//! Agent automation (AC-TERM-001, ADR-0018 decision 7): mode
//! agent_automation is authorized at create through the capability broker
//! (grant + lease for the terminal capability, enforce_dispatch), and
//! every later mutation of an agent session dispatches through the broker
//! gate (owner = agent id, generation, scope, lease) into the terminal
//! provider registered in the broker. Human takeover
//! (dsh_supervisor::broker::Broker::revoke_agent_grants) revokes the lease,
//! after which agent sessions reject every mutation (fail-closed). Human
//! sessions never touch the broker (ADR-0015 decision 2: Surface/Automation
//! separation; Terminal Surface only reads/writes its own sessions).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use dsh_supervisor::broker::{
    Broker, BrokerError, CapabilityId, Clock, Invocation, InvocationResult, Provider, Scope,
    SystemClock,
};
use dsh_terminal_provider::{
    MAX_COLS, MAX_ROWS, MAX_WRITE_BYTES, PtyError, PtyRegistry, PtyReport,
};

const SCHEMA_VERSION: u8 = 1;
const EVENT_NAME: &str = "terminal://output";
/// Output event drain interval while a terminal surface is mounted.
const EVENT_DRAIN_INTERVAL: Duration = Duration::from_millis(30);
/// Broker provider id for the terminal capability (ADR-0018 decision 7).
const TERMINAL_PROVIDER_ID: &str = "terminal";

/// Terminal capability coordinate (IF-TERMINAL api_version, cf.
/// specs/protocol/fixtures/envelope.agreement.valid.json granted
/// coordinate terminal.dsh-desktop.local/v1alpha1 + Terminal). The broker
/// gate enforces against exactly this id.
fn terminal_capability() -> CapabilityId {
    CapabilityId::new("terminal.dsh-desktop.local/v1alpha1", "Terminal")
}

/// One output event forwarded to the surface (matches specs/terminal schema).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputEvent {
    schema_version: u8,
    session_id: String,
    seq: u64,
    data: String,
    timestamp_unix_ms: u64,
}

/// Terminal commands act on opaque session ids only (AC-TERM-002).
#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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
    pub(crate) fn is_valid(&self) -> bool {
        if self.schema_version != SCHEMA_VERSION {
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

    pub(crate) fn mode(&self) -> &str {
        &self.mode
    }
}

/// Agent authorization facts carried by an agent_automation create
/// (specs/terminal/terminal-create-request.schema.json agent object).
///
/// These mirror the broker grant facts the agent received in negotiation
/// (ADR-0018 decision 7). The broker gate validates them against the live
/// grant + lease at create; the session binding records them so every later
/// mutation of the session dispatches with the same owner/generation/scope.
#[derive(Debug, Clone, Deserialize)]
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

    fn to_broker_scope(&self) -> Scope {
        self.scope.to_broker_scope()
    }

    fn agent_id(&self) -> &str {
        &self.agent_id
    }

    fn activation_id(&self) -> &str {
        &self.activation_id
    }

    fn generation(&self) -> u64 {
        self.generation
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
/// scope, camelCase); converted to the broker Scope for enforcement.
#[derive(Debug, Clone, Deserialize)]
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

    fn to_broker_scope(&self) -> Scope {
        Scope {
            session_id: self.session_id.clone(),
            workspace: self.workspace.clone(),
            domains: self.domains.clone(),
            resources: self.resources.clone(),
        }
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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
}

/// Immutable agent ownership record of an agent_automation session.
///
/// ADR-0018 decision 1 (activation ownership): a session belongs to exactly
/// one agent activation; the recorded facts are the ones the broker gate
/// validates on every mutation of the session.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentSessionBinding {
    agent_id: String,
    activation_id: String,
    generation: u64,
    scope: Scope,
}

/// App-managed terminal state: one registry, drained by a bridge task, and
/// the shared capability broker that gates agent automation.
#[derive(Clone)]
pub struct TerminalState<C: Clock = SystemClock> {
    inner: Arc<Mutex<TerminalBridge>>,
    broker: Arc<Mutex<Broker<C>>>,
}

impl Default for TerminalState<SystemClock> {
    fn default() -> Self {
        Self::new(Arc::new(Mutex::new(Broker::new())))
    }
}

impl<C: Clock> TerminalState<C> {
    /// Builds the state and registers the terminal provider in the broker
    /// (idempotent: a duplicate registration is ignored).
    pub fn new(broker: Arc<Mutex<Broker<C>>>) -> Self {
        let inner = Arc::new(Mutex::new(TerminalBridge::default()));
        let provider = Provider::new(
            TERMINAL_PROVIDER_ID,
            terminal_capability(),
            terminal_provider_handler(Arc::clone(&inner)),
        );
        if let Ok(mut broker) = broker.lock() {
            let _ = broker.register_provider(provider);
        }
        Self { inner, broker }
    }
}

#[derive(Default)]
struct TerminalBridge {
    registry: Option<PtyRegistry>,
    /// Agent ownership of agent_automation sessions (opaque id -> binding).
    bindings: HashMap<String, AgentSessionBinding>,
}

impl TerminalBridge {
    fn registry(&mut self) -> &PtyRegistry {
        self.registry.get_or_insert_with(PtyRegistry::new)
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

pub(crate) fn create_terminal<C: Clock>(
    state: &TerminalState<C>,
    request: &TerminalCreateRequest,
) -> Result<TerminalReport, TerminalCommandError> {
    if !request.is_valid() {
        return Err(TerminalCommandError::malformed());
    }
    let binding = request.agent.as_ref().map(|agent| AgentSessionBinding {
        agent_id: agent.agent_id().to_string(),
        activation_id: agent.activation_id().to_string(),
        generation: agent.generation(),
        scope: agent.to_broker_scope(),
    });
    if let Some(binding) = &binding {
        // AC-TERM-001: agent_automation create requires a live terminal
        // grant + lease for this agent activation (fail-closed; an agent
        // with nothing granted is UNAUTHORIZED).
        let broker = state
            .broker
            .lock()
            .map_err(|_| TerminalCommandError::unavailable())?;
        broker
            .enforce_dispatch(
                &terminal_capability(),
                &binding.agent_id,
                binding.generation,
                &binding.scope,
            )
            .map_err(map_broker_error)?;
    }
    let mode = request.mode();
    let report = {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| TerminalCommandError::unavailable())?;
        let report = bridge
            .registry()
            .create(
                request.shell.as_deref(),
                request.cols,
                request.rows,
                request.cwd.as_deref(),
            )
            .map_err(map_pty_error)?;
        if let Some(binding) = binding {
            bridge.bindings.insert(report.session_id.clone(), binding);
        }
        report
    };
    Ok(public_report(report, mode))
}

pub(crate) fn write_terminal<C: Clock>(
    state: &TerminalState<C>,
    request: &TerminalWriteRequest,
) -> Result<(), TerminalCommandError> {
    if request.schema_version() != SCHEMA_VERSION
        || !validate_session_request(&TerminalSessionRequest {
            schema_version: request.schema_version(),
            session_id: request.session_id().to_string(),
        })
    {
        return Err(TerminalCommandError::malformed());
    }
    let binding = session_binding(state, request.session_id())?;
    if let Some(binding) = binding {
        let payload = serde_json::json!({
            "method": "write",
            "sessionId": request.session_id(),
            "data": request.data(),
        });
        dispatch_agent_mutation(state, &binding, "write", payload).map(|_| ())
    } else {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| TerminalCommandError::unavailable())?;
        bridge
            .registry()
            .write(request.session_id(), request.data())
            .map_err(map_pty_error)
    }
}

pub(crate) fn resize_terminal<C: Clock>(
    state: &TerminalState<C>,
    request: &TerminalResizeRequest,
) -> Result<TerminalReport, TerminalCommandError> {
    if request.schema_version() != SCHEMA_VERSION
        || !validate_session_request(&TerminalSessionRequest {
            schema_version: request.schema_version(),
            session_id: request.session_id().to_string(),
        })
    {
        return Err(TerminalCommandError::malformed());
    }
    let binding = session_binding(state, request.session_id())?;
    if let Some(binding) = binding {
        let payload = serde_json::json!({
            "method": "resize",
            "sessionId": request.session_id(),
            "cols": request.cols,
            "rows": request.rows,
        });
        let report = dispatch_agent_mutation(state, &binding, "resize", payload)?;
        report.ok_or_else(TerminalCommandError::unavailable)
    } else {
        let report = {
            let mut bridge = state
                .inner
                .lock()
                .map_err(|_| TerminalCommandError::unavailable())?;
            bridge
                .registry()
                .resize(request.session_id(), request.cols, request.rows)
                .map_err(map_pty_error)?
        };
        Ok(public_report(report, "human_surface"))
    }
}

pub(crate) fn close_terminal<C: Clock>(
    state: &TerminalState<C>,
    request: &TerminalSessionRequest,
) -> Result<(), TerminalCommandError> {
    if !validate_session_request(request) {
        return Err(TerminalCommandError::malformed());
    }
    let binding = session_binding(state, request.session_id())?;
    if let Some(binding) = binding {
        let payload = serde_json::json!({ "method": "close", "sessionId": request.session_id() });
        dispatch_agent_mutation(state, &binding, "close", payload).map(|_| ())
    } else {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| TerminalCommandError::unavailable())?;
        bridge
            .registry()
            .close(request.session_id())
            .map_err(map_pty_error)
    }
}

pub(crate) fn status_terminal<C: Clock>(
    state: &TerminalState<C>,
    request: &TerminalSessionRequest,
) -> Result<TerminalReport, TerminalCommandError> {
    if !validate_session_request(request) {
        return Err(TerminalCommandError::malformed());
    }
    let report = {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| TerminalCommandError::unavailable())?;
        let report = bridge
            .registry()
            .report(request.session_id())
            .map_err(map_pty_error)?;
        let mode = mode_for(&bridge, request.session_id());
        public_report(report, mode)
    };
    Ok(report)
}

pub(crate) fn list_terminals<C: Clock>(state: &TerminalState<C>) -> Vec<TerminalReport> {
    let reports = match state.inner.lock() {
        Ok(mut bridge) => bridge.registry().list(),
        Err(_) => return Vec::new(),
    };
    let mode_map = match state.inner.lock() {
        Ok(bridge) => bridge,
        Err(_) => return Vec::new(),
    };
    reports
        .into_iter()
        .map(|report| {
            let mode = mode_for(&mode_map, &report.session_id);
            public_report(report, mode)
        })
        .collect()
}

/// Drain pending output events into the Shell WebView (AC-TERM-002).
pub(crate) fn drain_events(app: &AppHandle, state: &TerminalState) {
    let event = {
        let mut bridge = match state.inner.lock() {
            Ok(bridge) => bridge,
            Err(_) => return,
        };
        bridge.registry().try_next_event()
    };
    let Some(event) = event else { return };
    {
        let payload = TerminalOutputEvent {
            schema_version: SCHEMA_VERSION,
            session_id: event.session_id,
            seq: event.seq,
            data: event.data,
            timestamp_unix_ms: now_ms(),
        };
        // Emit only to the Shell window; child webviews never receive it.
        let _ = app.emit(EVENT_NAME, payload);
    }
}

/// Background drain task; started once by the app.
pub(crate) fn start_event_drain(app: AppHandle, state: TerminalState) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(EVENT_DRAIN_INTERVAL);
            drain_events(&app, &state);
        }
    });
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

/// Mode of an existing session: agent sessions report agent_automation.
fn mode_for(bridge: &TerminalBridge, session_id: &str) -> &'static str {
    if bridge.bindings.contains_key(session_id) {
        "agent_automation"
    } else {
        "human_surface"
    }
}

/// Snapshot the agent binding of a session, if any. The bridge guard is
/// released before any broker interaction so the two locks never nest.
fn session_binding<C: Clock>(
    state: &TerminalState<C>,
    session_id: &str,
) -> Result<Option<AgentSessionBinding>, TerminalCommandError> {
    let bridge = state
        .inner
        .lock()
        .map_err(|_| TerminalCommandError::unavailable())?;
    Ok(bridge.bindings.get(session_id).cloned())
}

// ----------------------------------------------------------------------
// Broker gate (ADR-0018 decision 7, AC-TERM-001)
// ----------------------------------------------------------------------

/// Dispatch one agent-session mutation through the broker gate: owner =
/// binding agent, generation, scope and the live lease decide admission;
/// on success the registered terminal provider executes the mutation.
fn dispatch_agent_mutation<C: Clock>(
    state: &TerminalState<C>,
    binding: &AgentSessionBinding,
    method: &str,
    payload: serde_json::Value,
) -> Result<Option<TerminalReport>, TerminalCommandError> {
    let invocation = Invocation {
        capability: terminal_capability(),
        method: method.to_string(),
        owner: binding.agent_id.clone(),
        generation: binding.generation,
        scope: binding.scope.clone(),
        payload,
    };
    let broker = state
        .broker
        .lock()
        .map_err(|_| TerminalCommandError::unavailable())?;
    let result = broker
        .dispatch(TERMINAL_PROVIDER_ID, &invocation)
        .map_err(map_broker_error)?;
    drop(broker);
    let result: TerminalProviderResult =
        serde_json::from_value(result.payload).map_err(|_| TerminalCommandError::unavailable())?;
    if result.ok {
        Ok(result.report)
    } else {
        Err(result
            .error
            .unwrap_or_else(TerminalCommandError::unavailable))
    }
}

/// Broker-side terminal provider: executes write/resize/close on the PTY
/// registry after the dispatch gate passed. The invocation payload is the
/// mutation JSON below; the result payload is a TerminalProviderResult.
fn terminal_provider_handler(
    inner: Arc<Mutex<TerminalBridge>>,
) -> impl Fn(&Invocation) -> InvocationResult + 'static {
    move |invocation: &Invocation| {
        let result = run_terminal_provider_mutation(&inner, invocation);
        InvocationResult {
            payload: serde_json::to_value(result).unwrap_or_else(|_| {
                serde_json::to_value(TerminalProviderResult {
                    ok: false,
                    report: None,
                    error: Some(TerminalCommandError::unavailable()),
                })
                .expect("static result payload")
            }),
        }
    }
}

/// Provider mutation payload (method + session-scoped params).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TerminalMutation {
    method: String,
    session_id: String,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}

/// Provider result payload: ok with an optional public report (resize),
/// or a TerminalCommandError carrying the provider error contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalProviderResult {
    ok: bool,
    #[serde(default)]
    report: Option<TerminalReport>,
    #[serde(default)]
    error: Option<TerminalCommandError>,
}

fn run_terminal_provider_mutation(
    inner: &Arc<Mutex<TerminalBridge>>,
    invocation: &Invocation,
) -> TerminalProviderResult {
    let mutation: TerminalMutation = match serde_json::from_value(invocation.payload.clone()) {
        Ok(mutation) => mutation,
        Err(_) => return err_result(TerminalCommandError::malformed()),
    };
    let mut bridge = match inner.lock() {
        Ok(bridge) => bridge,
        Err(_) => return err_result(TerminalCommandError::unavailable()),
    };
    let mode = mode_for(&bridge, &mutation.session_id);
    let registry = bridge.registry();
    match mutation.method.as_str() {
        "write" => {
            let Some(data) = mutation.data.as_deref() else {
                return err_result(TerminalCommandError::malformed());
            };
            match registry.write(&mutation.session_id, data) {
                Ok(()) => ok_result(None),
                Err(error) => err_result(map_pty_error(error)),
            }
        }
        "resize" => {
            let (Some(cols), Some(rows)) = (mutation.cols, mutation.rows) else {
                return err_result(TerminalCommandError::malformed());
            };
            match registry.resize(&mutation.session_id, cols, rows) {
                Ok(report) => ok_result(Some(public_report(report, mode))),
                Err(error) => err_result(map_pty_error(error)),
            }
        }
        "close" => match registry.close(&mutation.session_id) {
            Ok(()) => ok_result(None),
            Err(error) => err_result(map_pty_error(error)),
        },
        _ => err_result(TerminalCommandError::malformed()),
    }
}

fn ok_result(report: Option<TerminalReport>) -> TerminalProviderResult {
    TerminalProviderResult {
        ok: true,
        report,
        error: None,
    }
}

fn err_result(error: TerminalCommandError) -> TerminalProviderResult {
    TerminalProviderResult {
        ok: false,
        report: None,
        error: Some(error),
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

    fn authorization(code: &'static str, message: &'static str, retryable: bool) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            retryable,
            correlation_id: correlation_id(),
        }
    }
}

/// Map a broker rejection to the terminal command error contract.
///
/// ERROR_MODEL.md: UNAUTHORIZED covers "未授权、lease 无效或 scope 不符"; an
/// agent with no terminal grant/lease at all is exactly that (the capability
/// itself always exists on the Desktop), so UnknownCapability surfaces as
/// UNAUTHORIZED, not UNAVAILABLE. UnknownProvider stays UNAVAILABLE
/// (registration failure is an app-level fault, retryable).
fn map_broker_error(error: BrokerError) -> TerminalCommandError {
    let (code, message, retryable) = match error {
        BrokerError::UnknownCapability
        | BrokerError::NotGranted
        | BrokerError::LeaseExpired
        | BrokerError::LeaseRevoked
        | BrokerError::ScopeMismatch => (
            "UNAUTHORIZED",
            "Agent is not authorized for terminal automation.",
            false,
        ),
        BrokerError::UnknownProvider => {
            ("UNAVAILABLE", "Terminal provider is not registered.", true)
        }
        BrokerError::GenerationMismatch => (
            "STALE_GENERATION",
            "Agent terminal request carries a stale generation.",
            false,
        ),
        BrokerError::Conflict => ("CONFLICT", "Terminal broker state conflict.", false),
    };
    TerminalCommandError::authorization(code, message, retryable)
}

fn map_pty_error(error: PtyError) -> TerminalCommandError {
    match error {
        PtyError::InvalidShell => TerminalCommandError::pty(
            "UNAUTHORIZED",
            "Shell selection is not supported for the human surface.",
            false,
        ),
        PtyError::SpawnUnavailable => {
            TerminalCommandError::pty("UNAVAILABLE", "Pseudo console or shell spawn failed.", true)
        }
        PtyError::UnknownSession => TerminalCommandError::pty(
            "UNAVAILABLE",
            "Terminal session is unknown or already closed.",
            false,
        ),
        PtyError::InvalidGeometry => TerminalCommandError::pty(
            "MALFORMED_MESSAGE",
            "Terminal geometry is outside schema bounds.",
            false,
        ),
        PtyError::WriteTooLarge => TerminalCommandError::pty(
            "MALFORMED_MESSAGE",
            "Terminal write exceeds the schema bound.",
            false,
        ),
        PtyError::Io => TerminalCommandError::pty("UNAVAILABLE", "Terminal I/O failed.", true),
        PtyError::StateUnavailable => TerminalCommandError::unavailable(),
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
    use crate::managed_runtime;

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

    #[test]
    fn malformed_create_is_rejected() {
        let state = TerminalState::default();
        let mut request = create_request(80, 24);
        request.mode = "immersive".to_string();
        let error = create_terminal(&state, &request).expect_err("unsupported mode rejected");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
    }

    #[test]
    fn terminal_echo_roundtrip_via_bridge() {
        let state = TerminalState::default();
        let report = create_terminal(&state, &create_request(80, 24)).expect("pty");
        assert!(report.session_id.starts_with("pty-"));
        assert_eq!(report.mode, "human_surface");
        assert_eq!(report.state, "running");
        let write = TerminalWriteRequest {
            schema_version: 1,
            session_id: report.session_id.clone(),
            data: "echo bridge-ok\r\n".to_string(),
        };
        write_terminal(&state, &write).expect("write");
        // The bridge drains events into the app channel; the registry itself
        // buffers them, so verify via the internal registry directly. The
        // guard must drop before the status/close commands below.
        let saw = {
            let mut bridge = state.inner.lock().unwrap();
            let registry = bridge.registry();
            let mut saw = String::new();
            for _ in 0..50 {
                if let Some(event) = registry.recv_event_timeout(Duration::from_millis(200)) {
                    saw.push_str(&event.data);
                    if saw.contains("bridge-ok") {
                        break;
                    }
                }
            }
            saw
        };
        assert!(saw.contains("bridge-ok"), "output: {saw:?}");
        let resize = TerminalResizeRequest {
            schema_version: 1,
            session_id: report.session_id.clone(),
            cols: 100,
            rows: 40,
        };
        let resized = resize_terminal(&state, &resize).expect("resize");
        assert_eq!(resized.cols, 100);
        let status = TerminalSessionRequest {
            schema_version: 1,
            session_id: report.session_id.clone(),
        };
        assert!(status_terminal(&state, &status).is_ok());
        close_terminal(&state, &status).expect("close");
        assert!(status_terminal(&state, &status).is_err());
    }

    #[test]
    fn pty_survives_managed_runtime_restart() {
        // AC-PTY-001: the terminal session is Desktop-owned and independent
        // of the Managed DSH process tree; stopping and restarting the DSH
        // runtime must not affect the PTY.
        let terminal = TerminalState::default();
        let report = create_terminal(&terminal, &create_request(80, 24)).expect("pty");

        let runtime = managed_runtime::ManagedRuntimeState::default();
        let environment = managed_runtime::test_managed_environment();
        let started = managed_runtime::test_start_with_spec(
            &runtime,
            managed_runtime::test_fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("managed start");
        assert_eq!(started.runtime_state(), "healthy");

        // Stop and restart the Managed DSH while the PTY stays live.
        managed_runtime::test_stop_managed(&runtime, &environment, started.generation())
            .expect("managed stop");
        let restarted = managed_runtime::test_start_with_spec(
            &runtime,
            managed_runtime::test_fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("managed restart");
        assert!(restarted.generation() > started.generation());

        // The PTY must still accept writes and echo output.
        let write = TerminalWriteRequest {
            schema_version: 1,
            session_id: report.session_id.clone(),
            data: "echo survived-restart\r\n".to_string(),
        };
        write_terminal(&terminal, &write).expect("pty write after restart");
        let saw = {
            let mut bridge = terminal.inner.lock().unwrap();
            let registry = bridge.registry();
            let mut saw = String::new();
            for _ in 0..50 {
                if let Some(event) = registry.recv_event_timeout(Duration::from_millis(200)) {
                    saw.push_str(&event.data);
                    if saw.contains("survived-restart") {
                        break;
                    }
                }
            }
            saw
        };
        assert!(saw.contains("survived-restart"), "output: {saw:?}");
        let status = TerminalSessionRequest {
            schema_version: 1,
            session_id: report.session_id.clone(),
        };
        assert!(status_terminal(&terminal, &status).is_ok());
        close_terminal(&terminal, &status).expect("pty close");
        managed_runtime::test_stop_managed(&runtime, &environment, restarted.generation())
            .expect("final stop");
    }
}

#[cfg(test)]
mod agent_tests {
    use super::*;
    use dsh_supervisor::broker::LeaseRevocationReason;
    use dsh_supervisor::{AgentConformanceState, AgentLeaseConstraints, AgentNegotiationResult};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    const AGENT: &str = "agent-1";
    const ACTIVATION: &str = "act-0001";
    const T0: u64 = 1_000_000;

    /// Deterministic clock. Arc<AtomicU64> keeps the broker Send + Sync so
    /// the shared-handle tests mirror production (same pattern as
    /// broker/agent/tests.rs, minus the Rc that would trip
    /// clippy::arc_with_non_send_sync).
    #[derive(Clone)]
    struct FakeClock {
        now: Arc<AtomicU64>,
    }

    impl FakeClock {
        fn new(start: u64) -> Self {
            Self {
                now: Arc::new(AtomicU64::new(start)),
            }
        }

        fn advance(&self, ms: u64) {
            self.now.fetch_add(ms, Ordering::Relaxed);
        }
    }

    impl Clock for FakeClock {
        fn now_unix_ms(&self) -> u64 {
            self.now.load(Ordering::Relaxed)
        }
    }

    fn grant_scope() -> Scope {
        Scope {
            session_id: Some("session-1".into()),
            workspace: Some("ws-a".into()),
            ..Default::default()
        }
    }

    /// Broker with a negotiated terminal grant + lease for AGENT
    /// (ADR-0018 chain: negotiation -> grant -> lease).
    fn authorized_broker(clock: FakeClock) -> Broker<FakeClock> {
        let mut broker = Broker::with_clock(clock);
        broker
            .broker_grant_from_negotiation(
                AGENT,
                AgentNegotiationResult {
                    activation_id: ACTIVATION.into(),
                    agreed: true,
                    granted: vec![terminal_capability()],
                    conformance: AgentConformanceState::Known,
                    lease_constraints: Some(AgentLeaseConstraints::new(60)),
                    scope: grant_scope(),
                },
            )
            .expect("negotiation grants terminal");
        broker
    }

    fn agent_identity() -> TerminalAgentIdentity {
        TerminalAgentIdentity {
            agent_id: AGENT.into(),
            activation_id: ACTIVATION.into(),
            generation: 1,
            scope: TerminalAgentScope {
                session_id: Some("session-1".into()),
                workspace: Some("ws-a".into()),
                domains: Vec::new(),
                resources: Vec::new(),
            },
        }
    }

    fn agent_create_request(cols: u16, rows: u16) -> TerminalCreateRequest {
        TerminalCreateRequest {
            schema_version: 1,
            mode: "agent_automation".to_string(),
            cols,
            rows,
            shell: None,
            cwd: None,
            agent: Some(agent_identity()),
        }
    }

    fn create_human_request(cols: u16, rows: u16) -> TerminalCreateRequest {
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

    fn state_with(broker: Broker<FakeClock>) -> TerminalState<FakeClock> {
        TerminalState::new(Arc::new(Mutex::new(broker)))
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

    /// Wait for the PTY to echo needle (both paths buffer output in the
    /// registry event queue).
    fn wait_for_output(state: &TerminalState<FakeClock>, needle: &str) -> String {
        let mut bridge = state.inner.lock().unwrap();
        let registry = bridge.registry();
        let mut saw = String::new();
        for _ in 0..50 {
            if let Some(event) = registry.recv_event_timeout(Duration::from_millis(200)) {
                saw.push_str(&event.data);
                if saw.contains(needle) {
                    break;
                }
            }
        }
        saw
    }

    // ------------------------------------------------------------------
    // Create authorization (AC-TERM-001)
    // ------------------------------------------------------------------

    #[test]
    fn agent_automation_create_without_grant_rejected() {
        let state = state_with(Broker::with_clock(FakeClock::new(T0)));
        let error = create_terminal(&state, &agent_create_request(80, 24))
            .expect_err("no grant -> unauthorized");
        assert_eq!(error.code, "UNAUTHORIZED");
    }

    #[test]
    fn agent_automation_create_with_grant_succeeds() {
        let state = state_with(authorized_broker(FakeClock::new(T0)));
        let report =
            create_terminal(&state, &agent_create_request(80, 24)).expect("authorized pty");
        assert_eq!(report.mode, "agent_automation");
        assert!(report.session_id.starts_with("pty-"));
        // The session is recorded as agent-owned.
        let bridge = state.inner.lock().unwrap();
        let binding = bridge
            .bindings
            .get(&report.session_id)
            .expect("binding recorded");
        assert_eq!(binding.agent_id, AGENT);
        assert_eq!(binding.activation_id, ACTIVATION);
        assert_eq!(binding.generation, 1);
    }

    #[test]
    fn agent_automation_create_stale_generation_rejected() {
        let state = state_with(authorized_broker(FakeClock::new(T0)));
        let mut request = agent_create_request(80, 24);
        request.agent.as_mut().expect("agent").generation = 2;
        let error = create_terminal(&state, &request).expect_err("stale generation");
        assert_eq!(error.code, "STALE_GENERATION");
    }

    #[test]
    fn agent_automation_create_scope_mismatch_rejected() {
        let state = state_with(authorized_broker(FakeClock::new(T0)));
        let mut request = agent_create_request(80, 24);
        request.agent.as_mut().expect("agent").scope.workspace = Some("ws-b".into());
        let error = create_terminal(&state, &request).expect_err("scope mismatch");
        assert_eq!(error.code, "UNAUTHORIZED");
    }

    #[test]
    fn agent_automation_create_lease_expired_rejected() {
        let clock = FakeClock::new(T0);
        let state = state_with(authorized_broker(clock.clone()));
        // 60s lease; after expiry the same agent facts must fail closed.
        clock.advance(61_000);
        let error =
            create_terminal(&state, &agent_create_request(80, 24)).expect_err("lease expired");
        assert_eq!(error.code, "UNAUTHORIZED");
    }

    #[test]
    fn agent_automation_create_shape_matrix() {
        let state = state_with(authorized_broker(FakeClock::new(T0)));

        // agent_automation without agent identity: malformed shape.
        let mut no_agent = agent_create_request(80, 24);
        no_agent.agent = None;
        let error = create_terminal(&state, &no_agent).expect_err("agent required");
        assert_eq!(error.code, "MALFORMED_MESSAGE");

        // human_surface carrying agent identity: cross-mode fail-closed.
        let mut human_with_agent = create_human_request(80, 24);
        human_with_agent.agent = Some(agent_identity());
        let error = create_terminal(&state, &human_with_agent).expect_err("cross-mode");
        assert_eq!(error.code, "MALFORMED_MESSAGE");

        // invalid agent token shapes.
        let long_token = "a".repeat(65);
        for bad in ["", "a b", long_token.as_str()] {
            let mut request = agent_create_request(80, 24);
            request.agent.as_mut().expect("agent").agent_id = bad.to_string();
            let error = create_terminal(&state, &request).expect_err("bad agent id");
            assert_eq!(error.code, "MALFORMED_MESSAGE");
        }

        // empty scope (no covered dimension) is malformed shape.
        let mut request = agent_create_request(80, 24);
        request.agent.as_mut().expect("agent").scope = TerminalAgentScope {
            session_id: None,
            workspace: None,
            domains: Vec::new(),
            resources: Vec::new(),
        };
        let error = create_terminal(&state, &request).expect_err("empty scope");
        assert_eq!(error.code, "MALFORMED_MESSAGE");

        // generation 0 is malformed shape.
        let mut request = agent_create_request(80, 24);
        request.agent.as_mut().expect("agent").generation = 0;
        let error = create_terminal(&state, &request).expect_err("generation 0");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
    }

    // ------------------------------------------------------------------
    // Mutation dispatch gate (ADR-0018 decision 7)
    // ------------------------------------------------------------------

    #[test]
    fn agent_write_dispatches_through_broker() {
        let state = state_with(authorized_broker(FakeClock::new(T0)));
        let report = create_terminal(&state, &agent_create_request(80, 24)).expect("pty");
        write_terminal(
            &state,
            &write_request(&report.session_id, "echo agent-dispatch-ok\r\n"),
        )
        .expect("agent write through dispatch");
        let saw = wait_for_output(&state, "agent-dispatch-ok");
        assert!(saw.contains("agent-dispatch-ok"), "output: {saw:?}");

        let resized = resize_terminal(&state, &resize_request(&report.session_id, 100, 40))
            .expect("agent resize through dispatch");
        assert_eq!(resized.cols, 100);
        assert_eq!(resized.mode, "agent_automation");

        close_terminal(&state, &session_request(&report.session_id)).expect("agent close");
    }

    #[test]
    fn agent_mutation_rejected_without_lease() {
        let clock = FakeClock::new(T0);
        let broker = Arc::new(Mutex::new(authorized_broker(clock)));
        let state = TerminalState::new(Arc::clone(&broker));
        let report = create_terminal(&state, &agent_create_request(80, 24)).expect("pty");

        // Revoke the only terminal lease (AC-LEASE-001): the grant stays,
        // the dispatch gate fails.
        let lease_id = broker
            .lock()
            .unwrap()
            .leases_for(&terminal_capability())
            .first()
            .expect("lease")
            .id
            .clone();
        broker
            .lock()
            .unwrap()
            .revoke(&lease_id, LeaseRevocationReason::HumanTakeover)
            .expect("revoke");

        let error = write_terminal(&state, &write_request(&report.session_id, "echo nope\r\n"))
            .expect_err("no lease -> unauthorized");
        assert_eq!(error.code, "UNAUTHORIZED");
        let error = resize_terminal(&state, &resize_request(&report.session_id, 100, 40))
            .expect_err("no lease -> unauthorized");
        assert_eq!(error.code, "UNAUTHORIZED");
        let error = close_terminal(&state, &session_request(&report.session_id))
            .expect_err("no lease -> unauthorized");
        assert_eq!(error.code, "UNAUTHORIZED");
    }

    #[test]
    fn agent_mutations_rejected_after_human_takeover() {
        let clock = FakeClock::new(T0);
        let broker = authorized_broker(clock);
        let state = state_with(broker);
        let report = create_terminal(&state, &agent_create_request(80, 24)).expect("pty");

        // Human takeover (AC-BRW-002 mechanism): revokes every lease of the
        // activation and durably marks it revoked.
        {
            let mut broker = state.broker.lock().unwrap();
            let revoked = broker.revoke_agent_grants(ACTIVATION);
            assert_eq!(revoked, 1);
        }

        let error = write_terminal(&state, &write_request(&report.session_id, "echo no\r\n"))
            .expect_err("takeover -> unauthorized");
        assert_eq!(error.code, "UNAUTHORIZED");
        let error = resize_terminal(&state, &resize_request(&report.session_id, 100, 40))
            .expect_err("takeover -> unauthorized");
        assert_eq!(error.code, "UNAUTHORIZED");
        let error = close_terminal(&state, &session_request(&report.session_id))
            .expect_err("takeover -> unauthorized");
        assert_eq!(error.code, "UNAUTHORIZED");
        // A new create with the same (revoked) activation facts is refused too.
        let error = create_terminal(&state, &agent_create_request(80, 24)).expect_err("takeover");
        assert_eq!(error.code, "UNAUTHORIZED");
    }

    #[test]
    fn agent_mutation_lease_expired_rejected() {
        let clock = FakeClock::new(T0);
        let state = state_with(authorized_broker(clock.clone()));
        let report = create_terminal(&state, &agent_create_request(80, 24)).expect("pty");
        clock.advance(61_000);
        let error = write_terminal(&state, &write_request(&report.session_id, "echo no\r\n"))
            .expect_err("expired -> unauthorized");
        assert_eq!(error.code, "UNAUTHORIZED");
    }

    #[test]
    fn human_sessions_do_not_touch_broker() {
        // A fully authorized agent in the broker must not change the human
        // path: human sessions never dispatch (ADR-0015 decision 2).
        let state = state_with(authorized_broker(FakeClock::new(T0)));
        let report = create_terminal(&state, &create_human_request(80, 24)).expect("human pty");
        assert_eq!(report.mode, "human_surface");
        write_terminal(
            &state,
            &write_request(&report.session_id, "echo human-ok\r\n"),
        )
        .expect("human write");
        let saw = wait_for_output(&state, "human-ok");
        assert!(saw.contains("human-ok"), "output: {saw:?}");
        let resized = resize_terminal(&state, &resize_request(&report.session_id, 100, 40))
            .expect("human resize");
        assert_eq!(resized.mode, "human_surface");
        let status = status_terminal(&state, &session_request(&report.session_id)).expect("status");
        assert_eq!(status.mode, "human_surface");
        close_terminal(&state, &session_request(&report.session_id)).expect("human close");
    }
}
