//! Desktop browser bridge (MOD-BROWSER boundary, ADR-0017).
//!
//! Owns the browser [`SessionRegistry`] (crates/browser-provider) and the
//! runtime [`WebviewWindow`] handles for every browser session. Sessions are
//! Desktop-owned, opaque (`brw-<ms>-<seq>`), and each session runs in its own
//! per-webview WebView2 user-data folder (AppData/browser-profiles/<id>,
//! ADR-0017 decision 3: profile isolation, no DSH/default WebView2 data).
//!
//! Security posture (AC-BRW-001, RISK-BROWSER):
//! - The browser window label always starts with "browser-" and therefore
//!   never matches the Shell capability (`webviews: ["shell"]`) — the
//!   browser page has no privileged Desktop IPC (asserted in unit tests).
//! - Navigation policy is enforced twice: the Tauri `on_navigation` gate
//!   (http(s) only, no userinfo, len <= 2048) and WebView2 deny hooks
//!   installed before any remote document loads (ADR-0011 deny-order
//!   invariant): PermissionRequested deny, password autosave/autofill off.
//! - Popups, downloads and devtools are denied; `screenshot` snapshots
//!   fail closed. M5 (ADR-0017 decision 2 rev): agent_automation
//!   `interact` carries the agent authorization facts (agentId/
//!   activationId/generation/scope, same shape as the terminal create
//!   agent object) and passes the ADR-0014 broker dispatch gate against
//!   the shared capability broker (crate::broker, ADR-0018 decision 7);
//!   the mutation then executes as DOM-event dispatch via ExecuteScript —
//!   WebView2 exposes no CDP input API. Human `take_over` marks the
//!   session human-controlled and revokes the bound agent leases
//!   (Broker::revoke_agent_grants, AC-BRW-002).
//!
//! Events are relayed to the Shell WebView on `browser://event`
//! (navigation_changed / load_failed / closed), mirroring terminal://output.

use std::collections::{HashMap, HashSet};
#[cfg(all(windows, not(test)))]
use std::path::PathBuf;
#[cfg(all(windows, not(test)))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(windows, not(test)))]
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
#[cfg(all(windows, not(test)))]
use tauri::Manager;
use tauri::{AppHandle, Emitter, Url};

#[cfg(all(windows, not(test)))]
use tauri::WebviewUrl;
#[cfg(all(windows, not(test)))]
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewWindowBuilder};
#[cfg(all(windows, not(test)))]
use webview2_com::{
    Microsoft::Web::WebView2::Win32::*, NavigationCompletedEventHandler,
    PermissionRequestedEventHandler,
};
#[cfg(all(windows, not(test)))]
use windows_core::Interface;

use dsh_browser_provider::{
    BrowserError, BrowserSession, SessionRegistry, SessionState, UrlError, UrlPolicy,
};
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_CLOSE_METHOD, BROWSER_CREATE_METHOD, BROWSER_KIND,
    BROWSER_LIST_METHOD,
};
use dsh_daemon::envelope::ProtocolCoordinate;

use crate::daemon_client::{DaemonCommandError, DaemonConnector};

const SCHEMA_VERSION: u8 = 1;
const EVENT_NAME: &str = "browser://event";
/// Every runtime browser window label starts with this prefix; the Shell
/// capability targets only `shell`, so browser pages match no capability
/// (AC-BRW-001). The tauri window label charset is [a-z0-9-_], and the
/// session id is `brw-<ms>-<seq>`, so `browser-<session_id>` stays valid.
const BROWSER_WINDOW_LABEL_PREFIX: &str = "browser-";
/// Initial page loaded right after the deny hooks are installed.
#[cfg(all(windows, not(test)))]
const INITIAL_URL: &str = "https://example.com/";
/// URL length bound mirrored from specs/browser (navigate request maxLength).
const MAX_URL_LEN: usize = 2048;
/// Output event drain interval while a browser surface is mounted.
const EVENT_DRAIN_INTERVAL: Duration = Duration::from_millis(30);
/// Bounded wait for WebView2 native hook installation.
#[cfg(all(windows, not(test)))]
const NATIVE_HOOK_TIMEOUT: Duration = Duration::from_secs(2);
/// Bounded wait for an ExecuteScript snapshot result.
#[cfg(all(windows, not(test)))]
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(2);
/// Bounded wait for an ExecuteScript interact result (M5-E3).
#[cfg(all(windows, not(test)))]
const INTERACT_TIMEOUT: Duration = Duration::from_secs(2);
/// Browser capability coordinate: the coordinate the IF-NEGOTIATION
/// Agreement grants (cf. specs/protocol/fixtures/envelope.agreement.valid.json
/// granted coordinate browser.dsh-desktop.local/v1alpha1 + Browser, and
/// terminal.rs terminal_capability()); the broker gate enforces against
/// exactly this id.
fn browser_capability() -> dsh_supervisor::CapabilityId {
    dsh_supervisor::CapabilityId::new("browser.dsh-desktop.local/v1alpha1", "Browser")
}

/// M5-E3 bounds mirrored from specs/browser interact schema.
const MAX_SELECTOR_LEN: usize = 512;
const MAX_INTERACT_TEXT_LEN: usize = 4096;
const MAX_KEY_LEN: usize = 64;
const MAX_SCROLL_DELTA: i64 = 100_000;

// ----------------------------- Requests -----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserCreateRequest {
    schema_version: u8,
    mode: String,
}

impl BrowserCreateRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION && self.mode == "human_surface"
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserNavigateRequest {
    schema_version: u8,
    session_id: String,
    url: String,
}

impl BrowserNavigateRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserSnapshotRequest {
    schema_version: u8,
    session_id: String,
    snapshot_mode: String,
}

impl BrowserSnapshotRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn snapshot_mode(&self) -> &str {
        &self.snapshot_mode
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION && snapshot_mode_supported(self.snapshot_mode())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserCloseRequest {
    schema_version: u8,
    session_id: String,
}

impl BrowserCloseRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }
}

// --------------------- Agent interact / human takeover (M5-E3) ---------------------

/// Agent-only browser mutation (ADR-0017 decision 2 rev): the interact
/// request is valid only in `agent_automation` mode — humans drive the
/// browser themselves. The `agent` facts (agentId/activationId/generation/
/// scope) mirror the broker grant facts the agent received in negotiation
/// (ADR-0018 decision 7); the broker gate validates them against the live
/// grant + lease. Payload view of
/// specs/browser/browser-interact-request.schema.json.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserInteractRequest {
    schema_version: u8,
    session_id: String,
    mode: String,
    action: String,
    selector: Option<String>,
    text: Option<String>,
    key: Option<String>,
    delta_x: Option<i64>,
    delta_y: Option<i64>,
    agent: BrowserAgentIdentity,
}

impl BrowserInteractRequest {
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn action(&self) -> &str {
        &self.action
    }

    pub(crate) fn selector(&self) -> Option<&str> {
        self.selector.as_deref()
    }

    pub(crate) fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub(crate) fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    pub(crate) fn delta_x(&self) -> Option<i64> {
        self.delta_x
    }

    pub(crate) fn delta_y(&self) -> Option<i64> {
        self.delta_y
    }

    pub(crate) fn agent(&self) -> &BrowserAgentIdentity {
        &self.agent
    }

    /// Schema + per-action parameter validation (canonical enforcement;
    /// specs/browser/browser-interact-request.schema.json).
    pub(crate) fn is_valid(&self) -> bool {
        if self.schema_version != SCHEMA_VERSION
            || self.mode != "agent_automation"
            || !validate_session_request(self.schema_version, &self.session_id)
            || !self.agent.is_valid()
        {
            return false;
        }
        match self.action.as_str() {
            "click" => {
                in_bound(self.selector.as_deref(), MAX_SELECTOR_LEN)
                    && self.text.is_none()
                    && self.key.is_none()
                    && self.delta_x.is_none()
                    && self.delta_y.is_none()
            }
            "type" => {
                in_bound(self.selector.as_deref(), MAX_SELECTOR_LEN)
                    && in_bound(self.text.as_deref(), MAX_INTERACT_TEXT_LEN)
                    && self.key.is_none()
                    && self.delta_x.is_none()
                    && self.delta_y.is_none()
            }
            "scroll" => {
                self.selector.is_none()
                    && self.text.is_none()
                    && self.key.is_none()
                    && (self.delta_x.is_some() || self.delta_y.is_some())
                    && in_delta_bounds(self.delta_x)
                    && in_delta_bounds(self.delta_y)
            }
            "key" => {
                in_bound(self.key.as_deref(), MAX_KEY_LEN)
                    && self.selector.is_none()
                    && self.text.is_none()
                    && self.delta_x.is_none()
                    && self.delta_y.is_none()
            }
            _ => false,
        }
    }
}

/// Bound helper: present, non-empty, at most `max` characters.
fn in_bound(value: Option<&str>, max: usize) -> bool {
    value.is_some_and(|value| !value.is_empty() && value.chars().count() <= max)
}

fn in_delta_bounds(delta: Option<i64>) -> bool {
    delta.is_none_or(|delta| (-MAX_SCROLL_DELTA..=MAX_SCROLL_DELTA).contains(&delta))
}

/// Agent authorization facts carried by an agent_automation interact
/// (specs/browser/browser-interact-request.schema.json agent object;
/// same shape as the terminal create agent, ADR-0018 decision 7).
///
/// The broker gate validates these against the live grant + lease at
/// dispatch; the session binding records them so a human takeover can
/// revoke the activation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserAgentIdentity {
    agent_id: String,
    activation_id: String,
    generation: u64,
    scope: BrowserAgentScope,
}

impl BrowserAgentIdentity {
    fn is_valid(&self) -> bool {
        valid_agent_token(&self.agent_id)
            && valid_agent_token(&self.activation_id)
            && self.generation >= 1
            && self.scope.is_valid()
    }

    fn to_broker_scope(&self) -> dsh_supervisor::Scope {
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
pub struct BrowserAgentScope {
    session_id: Option<String>,
    workspace: Option<String>,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    resources: Vec<String>,
}

impl BrowserAgentScope {
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

    fn to_broker_scope(&self) -> dsh_supervisor::Scope {
        dsh_supervisor::Scope {
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
    let mut seen = HashSet::new();
    items
        .iter()
        .all(|item| !item.is_empty() && item.len() <= 128 && seen.insert(item))
}

/// Immutable agent ownership record of a session under agent interact.
///
/// ADR-0018 decision 1 (activation ownership): the recorded facts are the
/// ones the broker gate validated when the interact was authorized; a
/// human takeover revokes exactly the bound activations.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentSessionBinding {
    agent_id: String,
    activation_id: String,
    generation: u64,
    scope: dsh_supervisor::Scope,
}

/// Human takeover request (AC-BRW-002): the operation itself is the
/// semantic — no mode field, the human acts directly (not through
/// agent_automation). Payload view of
/// specs/browser/browser-takeover-request.schema.json.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserTakeoverRequest {
    schema_version: u8,
    session_id: String,
    target: String,
}

impl BrowserTakeoverRequest {
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && self.target == "human"
            && validate_session_request(self.schema_version, &self.session_id)
    }
}

/// Validate a session request against schema rules (^brw-[a-z0-9-]+$, <= 64).
pub(crate) fn validate_session_request(schema_version: u8, session_id: &str) -> bool {
    schema_version == SCHEMA_VERSION
        && session_id.starts_with("brw-")
        && session_id.len() > 4
        && session_id.len() <= 64
        && session_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Only the text snapshot is implemented in M4-C; screenshot fails closed.
fn snapshot_mode_supported(mode: &str) -> bool {
    mode == "text"
}

// ----------------------------- Reports -----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserReport {
    schema_version: u8,
    session_id: String,
    state: String,
    mode: String,
    current_url: Option<String>,
    created_at_unix_ms: u64,
    last_activity_unix_ms: Option<u64>,
    error: Option<String>,
}

impl BrowserReport {
    /// Used by the frontend-facing list path and unit tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Snapshot result: the standard report plus the extracted page text
/// (contract: `snapshot_browser -> BrowserReport { text }`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshotReport {
    #[serde(flatten)]
    report: BrowserReport,
    text: String,
}

/// One event forwarded to the surface (matches specs/browser schema).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEventPayload {
    schema_version: u8,
    session_id: String,
    kind: String,
    occurred_at_unix_ms: u64,
    url: Option<String>,
}

fn public_report(session: BrowserSession) -> BrowserReport {
    BrowserReport {
        schema_version: SCHEMA_VERSION,
        session_id: session.session_id,
        state: session.state.as_str().to_string(),
        mode: "human_surface".to_string(),
        current_url: session.current_url,
        created_at_unix_ms: session.created_at_unix_ms,
        last_activity_unix_ms: session.last_activity_unix_ms,
        error: session.error,
    }
}

// ----------------------------- State -----------------------------

/// Live window handle. Test builds erase the handle so the wry/tao GUI
/// chain never links into the unit-test binary: tauri-winres only manifests
/// the app exe, so a test exe importing comctl32 v6 functions would abort at
/// load with STATUS_ENTRYPOINT_NOT_FOUND (no SxS activation context). The
/// erased map stays empty in tests; every runtime path is Windows-only.
#[cfg(all(windows, not(test)))]
type WindowHandle = tauri::WebviewWindow;
#[cfg(any(not(windows), test))]
type WindowHandle = ();

/// App-managed browser state: one registry + the live window handles,
/// plus the shared capability broker that gates agent interact (M5-E3,
/// ADR-0018 decision 7 — same shape as terminal::TerminalState).
#[derive(Clone)]
pub struct BrowserState<C: dsh_supervisor::Clock = dsh_supervisor::SystemClock> {
    inner: Arc<Mutex<BrowserBridge>>,
    broker: Arc<Mutex<dsh_supervisor::Broker<C>>>,
}

impl Default for BrowserState<dsh_supervisor::SystemClock> {
    fn default() -> Self {
        Self::new(Arc::new(Mutex::new(dsh_supervisor::Broker::new())))
    }
}

impl<C: dsh_supervisor::Clock> BrowserState<C> {
    /// Builds the state sharing the app-level broker handle (lib.rs wires
    /// the same broker into every agent_automation surface).
    pub fn new(broker: Arc<Mutex<dsh_supervisor::Broker<C>>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BrowserBridge::default())),
            broker,
        }
    }
}

struct BrowserBridge {
    /// Session state machine from crates/browser-provider.
    registry: Option<SessionRegistry>,
    /// Live window per session. The WebviewWindow re-derives the native
    /// ICoreWebView2 controller on demand; no raw COM handle is persisted.
    windows: HashMap<String, WindowHandle>,
    /// M5-E3 (AC-BRW-002): agent ownership of sessions an interact was
    /// authorized on (opaque session id -> binding); consumed by human
    /// takeover.
    bindings: HashMap<String, AgentSessionBinding>,
    /// M5-E3 (AC-BRW-002): sessions taken over by the human; agent
    /// interact is rejected fail-closed even before the broker gate.
    human_controlled: HashSet<String>,
}

// Manual impl: tauri::WebviewWindow (the production WindowHandle) does not
// implement Default, so derive would fail even though the fields are all
// defaultable.
#[allow(clippy::derivable_impls)]
impl Default for BrowserBridge {
    fn default() -> Self {
        Self {
            registry: None,
            windows: HashMap::new(),
            bindings: HashMap::new(),
            human_controlled: HashSet::new(),
        }
    }
}

impl BrowserBridge {
    fn registry(&mut self) -> &SessionRegistry {
        self.registry.get_or_insert_with(SessionRegistry::new)
    }
}

// ----------------------------- Bridge operations -----------------------------

/// Registry-only part of create (no window yet); used by tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn create_session(
    state: &BrowserState,
    request: &BrowserCreateRequest,
) -> Result<BrowserSession, BrowserCommandError> {
    if !request.is_valid() {
        return Err(BrowserCommandError::malformed());
    }
    let mut bridge = state
        .inner
        .lock()
        .map_err(|_| BrowserCommandError::state_unavailable())?;
    bridge.registry().create().map_err(map_browser_error)
}

/// Browser capability coordinate (browser.dsh-desktop.local/v1alpha1 +
/// Browser; the daemon is the session authority since M6-C3).
fn browser_coordinate() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: BROWSER_API_VERSION.into(),
        kind: BROWSER_KIND.into(),
    }
}

/// M6-C4: the daemon registers the session (browser.create, M6-C3 state
/// authority) and returns the opaque session id; the Shell adopts it
/// locally so the render-side navigate/snapshot/interact paths (which stay
/// Shell-side, ADR-0019 decision 2) can operate on it, then spawns the
/// WebView window. TODO(M6-C4): report local navigation state back to the
/// daemon once a browser.navigate envelope method exists daemon-side.
pub(crate) async fn create_browser(
    app: &AppHandle,
    connector: &dyn DaemonConnector,
    state: &BrowserState,
    request: &BrowserCreateRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    if !request.is_valid() {
        return Err(BrowserCommandError::malformed());
    }
    let report = invoke_browser_create(connector, request)?;
    let session = attach_daemon_session(state, &report)?;

    #[cfg(all(windows, not(test)))]
    return spawn_browser_window(app, state, session);

    #[cfg(any(not(windows), test))]
    {
        let _ = app;
        // Fail closed: native WebView2 surfaces are Windows-only, and the
        // unit-test binary must not link the GUI chain (WindowHandle is
        // erased there, so no window can exist anyway).
        let failed = mark_browser_load_failed(state, &session.session_id, "unsupported_platform")?;
        Ok(public_report(failed))
    }
}

/// Daemon-side create (envelope browser.create; the daemon validates the
/// human_surface mode and owns the session).
pub(crate) fn invoke_browser_create(
    connector: &dyn DaemonConnector,
    request: &BrowserCreateRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    if !request.is_valid() {
        return Err(BrowserCommandError::malformed());
    }
    let payload =
        serde_json::to_value(request).map_err(|_| BrowserCommandError::state_unavailable())?;
    let value = connector
        .invoke(browser_coordinate(), BROWSER_CREATE_METHOD, payload)
        .map_err(BrowserCommandError::from_daemon)?;
    serde_json::from_value(value)
        .map_err(|_| BrowserCommandError::unavailable_response("browser.create"))
}

/// Adopt a daemon-created session into the local render registry (attach
/// protocol; the session id is the single opaque id on both sides).
fn attach_daemon_session(
    state: &BrowserState,
    report: &BrowserReport,
) -> Result<BrowserSession, BrowserCommandError> {
    let session_state = match report.state.as_str() {
        "created" => SessionState::Created,
        "loading" => SessionState::Loading,
        "ready" => SessionState::Ready,
        "closed" => SessionState::Closed,
        "error" => SessionState::Error,
        _ => return Err(BrowserCommandError::state_unavailable()),
    };
    let mut bridge = state
        .inner
        .lock()
        .map_err(|_| BrowserCommandError::state_unavailable())?;
    bridge
        .registry()
        .attach(
            &report.session_id,
            session_state,
            report.current_url.as_deref(),
            report.created_at_unix_ms,
            report.last_activity_unix_ms,
            report.error.as_deref(),
        )
        .map_err(|_| BrowserCommandError::state_unavailable())
}

/// Registry-only part of navigate (URL policy + state machine).
/// TODO(M6-C4): once a browser.navigate envelope method exists daemon-side,
/// report the accepted navigation (session id + url) back to the daemon so
/// the daemon state authority stays current with render-side navigation.
pub(crate) fn navigate_session(
    state: &BrowserState,
    request: &BrowserNavigateRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    if !validate_session_request(request.schema_version(), request.session_id()) {
        return Err(BrowserCommandError::malformed());
    }
    let url = UrlPolicy::validate(request.url()).map_err(map_url_error)?;
    let session = {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        bridge
            .registry()
            .navigate(request.session_id(), &url)
            .map_err(map_browser_error)?
    };
    Ok(public_report(session))
}

pub(crate) async fn navigate_browser(
    app: &AppHandle,
    state: &BrowserState,
    request: &BrowserNavigateRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    if !validate_session_request(request.schema_version(), request.session_id()) {
        return Err(BrowserCommandError::malformed());
    }
    // Resolve the live window BEFORE mutating registry state so a desynced
    // session never leaves a dangling "loading" transition behind.
    #[cfg(all(windows, not(test)))]
    let window = {
        let bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        bridge
            .windows
            .get(request.session_id())
            .cloned()
            .ok_or_else(BrowserCommandError::session_unavailable)?
    };
    #[cfg(any(not(windows), test))]
    {
        // Erased window map (the unit-test binary never links the GUI
        // chain): the lookup can never succeed; mirror the production miss.
        let bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        bridge
            .windows
            .get(request.session_id())
            .cloned()
            .ok_or_else(BrowserCommandError::session_unavailable)?;
    }
    let report = navigate_session(state, request)?;
    #[cfg(all(windows, not(test)))]
    {
        let url = Url::parse(request.url()).map_err(|_| BrowserCommandError::malformed())?;
        window.navigate(url).map_err(|_| {
            let _ = mark_browser_load_failed(state, request.session_id(), "navigation failed");
            BrowserCommandError::unavailable("Browser navigation failed.", true)
        })?;
    }
    let _ = app;
    Ok(report)
}

pub(crate) async fn snapshot_browser(
    app: &AppHandle,
    state: &BrowserState,
    request: &BrowserSnapshotRequest,
) -> Result<BrowserSnapshotReport, BrowserCommandError> {
    if !request.is_valid() {
        if request.schema_version() == SCHEMA_VERSION
            && validate_session_request(request.schema_version(), request.session_id())
        {
            // Well-formed request for an unsupported snapshot mode.
            return Err(BrowserCommandError::not_supported());
        }
        return Err(BrowserCommandError::malformed());
    }
    #[cfg(all(windows, not(test)))]
    {
        let window = {
            let bridge = state
                .inner
                .lock()
                .map_err(|_| BrowserCommandError::state_unavailable())?;
            bridge
                .windows
                .get(request.session_id())
                .cloned()
                .ok_or_else(BrowserCommandError::session_unavailable)?
        };
        let report = {
            let mut bridge = state
                .inner
                .lock()
                .map_err(|_| BrowserCommandError::state_unavailable())?;
            bridge
                .registry()
                .get(request.session_id())
                .map(public_report)
                .map_err(map_browser_error)?
        };
        // ExecuteScript callback runs on the UI thread; wait for it off the
        // async runtime so a slow page never stalls a runtime worker.
        let text = tauri::async_runtime::spawn_blocking(move || snapshot_text(&window))
            .await
            .map_err(|_| {
                BrowserCommandError::unavailable("Browser snapshot task is unavailable.", true)
            })?
            .map_err(|_| {
                BrowserCommandError::unavailable("Browser snapshot is unavailable.", true)
            })?;
        // Best-effort cache so future agent_automation reads (M5) see the text.
        if let Ok(mut bridge) = state.inner.lock() {
            let _ = bridge.registry().set_snapshot(request.session_id(), &text);
        }
        let _ = app;
        Ok(BrowserSnapshotReport { report, text })
    }
    #[cfg(any(not(windows), test))]
    {
        // The unit-test binary never links the GUI chain (WindowHandle is
        // erased), so no window can exist here; fail closed like the
        // production miss path.
        let bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        bridge
            .windows
            .get(request.session_id())
            .cloned()
            .ok_or_else(BrowserCommandError::session_unavailable)?;
        let _ = app;
        Err(BrowserCommandError::unavailable(
            "Browser snapshot is unavailable.",
            true,
        ))
    }
}

/// Registry-only part of close (state machine); the closed event is enqueued
/// by the registry and drained on `browser://event`.
pub(crate) fn close_session(
    state: &BrowserState,
    request: &BrowserCloseRequest,
) -> Result<BrowserSession, BrowserCommandError> {
    if !validate_session_request(request.schema_version(), request.session_id()) {
        return Err(BrowserCommandError::malformed());
    }
    let mut bridge = state
        .inner
        .lock()
        .map_err(|_| BrowserCommandError::state_unavailable())?;
    bridge
        .registry()
        .close(request.session_id())
        .map_err(map_browser_error)
}

pub(crate) async fn close_browser(
    app: &AppHandle,
    connector: &dyn DaemonConnector,
    state: &BrowserState,
    request: &BrowserCloseRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    if !validate_session_request(request.schema_version(), request.session_id()) {
        return Err(BrowserCommandError::malformed());
    }
    // Daemon authority first: the session closes daemon-side and the daemon
    // publishes the closed lifecycle event (bridged to the frontend). Then
    // the local mirror closes (its closed event is idempotent for the
    // frontend) and the window is destroyed.
    let closed = invoke_browser_close(connector, request)?;
    // Local mirror close (state machine + local closed event; idempotent).
    close_session(state, request)?;
    #[cfg(all(windows, not(test)))]
    {
        let window = {
            let mut bridge = state
                .inner
                .lock()
                .map_err(|_| BrowserCommandError::state_unavailable())?;
            bridge.windows.remove(request.session_id())
        };
        // Destroying the window fires the Destroyed handler; registry close
        // is idempotent on an already-closed session, so the local closed
        // event is emitted exactly once (by the registry).
        if let Some(window) = window {
            let _ = window.close();
        }
    }
    #[cfg(any(not(windows), test))]
    {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        let _ = bridge.windows.remove(request.session_id());
    }
    let _ = app;
    // The daemon report is the authority view (the local close already
    // moved the mirror to closed).
    Ok(closed)
}

/// Daemon-side close (envelope browser.close).
pub(crate) fn invoke_browser_close(
    connector: &dyn DaemonConnector,
    request: &BrowserCloseRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    if !validate_session_request(request.schema_version(), request.session_id()) {
        return Err(BrowserCommandError::malformed());
    }
    let payload =
        serde_json::to_value(request).map_err(|_| BrowserCommandError::state_unavailable())?;
    let value = connector
        .invoke(browser_coordinate(), BROWSER_CLOSE_METHOD, payload)
        .map_err(BrowserCommandError::from_daemon)?;
    serde_json::from_value(value)
        .map_err(|_| BrowserCommandError::unavailable_response("browser.close"))
}

/// `browser.list` wire payload (`{ browsers: [...] }`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserListPayload {
    browsers: Vec<BrowserReport>,
}

/// M6-C4: the daemon is the browser session authority, so the list comes
/// from the daemon (browser.list).
pub(crate) async fn list_browsers(
    connector: &dyn DaemonConnector,
) -> Result<Vec<BrowserReport>, BrowserCommandError> {
    let value = connector
        .invoke(
            browser_coordinate(),
            BROWSER_LIST_METHOD,
            serde_json::json!({}),
        )
        .map_err(BrowserCommandError::from_daemon)?;
    let payload: BrowserListPayload = serde_json::from_value(value)
        .map_err(|_| BrowserCommandError::unavailable_response("browser.list"))?;
    Ok(payload.browsers)
}

// --------------------- Agent interact / human takeover (M5-E3, AC-BRW-002) ---------------------

/// M5-E3: authorize one agent interact against the broker dispatch gate
/// (ADR-0018 decision 7): browser capability + interact method + valid
/// agent lease covering the request scope. Fail-closed order: request
/// shape, live session, human-controlled flag, scope-vs-target
/// consistency, broker gate.
///
/// Registry-only part (no window handle); unit tests exercise the whole
/// authorization chain without the GUI. On success the session binding is
/// recorded so a human takeover revokes the activation.
pub(crate) fn authorize_interact<C: dsh_supervisor::Clock>(
    state: &BrowserState<C>,
    request: &BrowserInteractRequest,
) -> Result<(), BrowserCommandError> {
    if !request.is_valid() {
        return Err(BrowserCommandError::malformed());
    }
    let session_id = request.session_id().to_string();
    let agent = request.agent();
    // A session-scoped lease must target exactly the session the mutation
    // executes on (scope confusion guard; workspace-scoped leases are
    // validated by the broker coverage rule instead).
    if agent
        .scope
        .session_id
        .as_deref()
        .is_some_and(|scope_session| scope_session != session_id)
    {
        return Err(BrowserCommandError::unauthorized(
            "Agent scope targets a different session.",
            false,
        ));
    }
    {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        bridge
            .registry()
            .get(&session_id)
            .map_err(map_browser_error)?;
        if bridge.human_controlled.contains(&session_id) {
            return Err(BrowserCommandError::unauthorized(
                "Browser session is human-controlled.",
                false,
            ));
        }
    }
    // ADR-0014 dispatch gate: capability granted, owner matches, generation
    // matches, request scope covered by the grant scope, valid lease.
    let broker = state
        .broker
        .lock()
        .map_err(|_| BrowserCommandError::state_unavailable())?;
    broker
        .enforce_dispatch(
            &browser_capability(),
            agent.agent_id(),
            agent.generation(),
            &agent.to_broker_scope(),
        )
        .map_err(map_broker_error)?;
    drop(broker);
    let mut bridge = state
        .inner
        .lock()
        .map_err(|_| BrowserCommandError::state_unavailable())?;
    bridge.bindings.insert(
        session_id,
        AgentSessionBinding {
            agent_id: agent.agent_id().to_string(),
            activation_id: agent.activation_id().to_string(),
            generation: agent.generation(),
            scope: agent.to_broker_scope(),
        },
    );
    Ok(())
}

/// M5-E3: agent interact command. Authorization first, then the DOM-event
/// dispatch in the live browser page (Windows); non-Windows and the
/// unit-test binary fail closed like snapshot.
pub(crate) async fn interact_browser<C: dsh_supervisor::Clock>(
    app: &AppHandle,
    state: &BrowserState<C>,
    request: &BrowserInteractRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    authorize_interact(state, request)?;
    #[cfg(all(windows, not(test)))]
    {
        let window = {
            let bridge = state
                .inner
                .lock()
                .map_err(|_| BrowserCommandError::state_unavailable())?;
            bridge
                .windows
                .get(request.session_id())
                .cloned()
                .ok_or_else(BrowserCommandError::session_unavailable)?
        };
        let script = interact_script(request)?;
        // ExecuteScript callback runs on the UI thread; wait for it off the
        // async runtime so a slow page never stalls a runtime worker.
        let outcome =
            tauri::async_runtime::spawn_blocking(move || execute_interact(&window, &script))
                .await
                .map_err(|_| {
                    BrowserCommandError::unavailable("Browser interact task is unavailable.", true)
                })??;
        if !outcome.ok {
            return Err(BrowserCommandError::unavailable(
                "Browser interact target was not found.",
                false,
            ));
        }
        let report = {
            let mut bridge = state
                .inner
                .lock()
                .map_err(|_| BrowserCommandError::state_unavailable())?;
            bridge
                .registry()
                .get(request.session_id())
                .map(public_report)
                .map_err(map_browser_error)?
        };
        let _ = app;
        Ok(report)
    }
    #[cfg(any(not(windows), test))]
    {
        // The unit-test binary never links the GUI chain (WindowHandle is
        // erased), so no window can exist here; fail closed like the
        // production miss path.
        let bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        bridge
            .windows
            .get(request.session_id())
            .cloned()
            .ok_or_else(BrowserCommandError::session_unavailable)?;
        let _ = app;
        Err(BrowserCommandError::unavailable(
            "Browser interact is unavailable.",
            true,
        ))
    }
}

/// M5-E3: human takeover (AC-BRW-002). Marks the session human-controlled
/// (subsequent agent interact is rejected fail-closed) and revokes every
/// agent lease bound to the session through the broker
/// (Broker::revoke_agent_grants, durable revocation). Idempotent; unknown
/// sessions fail closed.
pub(crate) fn take_over_browser<C: dsh_supervisor::Clock>(
    state: &BrowserState<C>,
    request: &BrowserTakeoverRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    if !request.is_valid() {
        return Err(BrowserCommandError::malformed());
    }
    let session_id = request.session_id().to_string();
    // Mark the session human-controlled and drain its agent bindings under
    // one bridge lock (fail-closed: later interact is rejected even before
    // the broker gate).
    let (session, bound_activations) = {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        let session = bridge
            .registry()
            .get(&session_id)
            .map_err(map_browser_error)?;
        bridge.human_controlled.insert(session_id.clone());
        let bound_activations: Vec<String> = bridge
            .bindings
            .remove(&session_id)
            .map(|binding| vec![binding.activation_id])
            .unwrap_or_default();
        (session, bound_activations)
    };
    // AC-BRW-002: revoke every agent lease of the bound activations
    // (durable revocation — the same activation can never be re-issued).
    let mut broker = state
        .broker
        .lock()
        .map_err(|_| BrowserCommandError::state_unavailable())?;
    for activation_id in &bound_activations {
        let _ = broker.revoke_agent_grants(activation_id);
    }
    Ok(public_report(session))
}

/// Build the DOM-event dispatch script for one validated interact action
/// (M5-E3 minimal implementation): WebView2 exposes no CDP input API, so
/// clicks/typing are synthesized as DOM events via ExecuteScript.
///
/// Injection safety: every caller-supplied parameter is embedded as a JSON
/// string literal (serde_json) — page-controlled strings can never escape
/// the script literal. The script only touches the page DOM; the browser
/// webview holds no privileged Desktop IPC (AC-BRW-001).
fn interact_script(request: &BrowserInteractRequest) -> Result<String, BrowserCommandError> {
    let json = |value: &str| serde_json::to_string(value).expect("JSON string encoding");
    match request.action() {
        "click" => {
            let selector = json(
                request
                    .selector()
                    .ok_or_else(BrowserCommandError::malformed)?,
            );
            Ok(format!(
                "(()=>{{const el=document.querySelector({selector});if(!el)return{{ok:false,error:'not_found'}};const o={{bubbles:true,cancelable:true,view:window}};el.dispatchEvent(new MouseEvent('mousedown',o));el.dispatchEvent(new MouseEvent('mouseup',o));el.dispatchEvent(new MouseEvent('click',o));return{{ok:true}};}})()"
            ))
        }
        "type" => {
            let selector = json(
                request
                    .selector()
                    .ok_or_else(BrowserCommandError::malformed)?,
            );
            let text = json(request.text().ok_or_else(BrowserCommandError::malformed)?);
            Ok(format!(
                "(()=>{{const el=document.querySelector({selector});if(!el)return{{ok:false,error:'not_found'}};if(!(el instanceof HTMLInputElement||el instanceof HTMLTextAreaElement))return{{ok:false,error:'not_editable'}};const set=Object.getOwnPropertyDescriptor(el instanceof HTMLTextAreaElement?HTMLTextAreaElement.prototype:HTMLInputElement.prototype,'value').set;set.call(el,{text});el.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText',data:{text}}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true}};}})()"
            ))
        }
        "scroll" => {
            let delta_x = request.delta_x().unwrap_or(0);
            let delta_y = request.delta_y().unwrap_or(0);
            Ok(format!(
                "(()=>{{window.scrollBy({delta_x},{delta_y});return{{ok:true}};}})()"
            ))
        }
        "key" => {
            let key = json(request.key().ok_or_else(BrowserCommandError::malformed)?);
            Ok(format!(
                "(()=>{{const el=document.activeElement||document.body;const o={{bubbles:true,cancelable:true,key:{key},code:{key}}};el.dispatchEvent(new KeyboardEvent('keydown',o));el.dispatchEvent(new KeyboardEvent('keyup',o));return{{ok:true}};}})()"
            ))
        }
        _ => Err(BrowserCommandError::malformed()),
    }
}

/// ExecuteScript result of one interact script (page-controlled).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct InteractOutcome {
    ok: bool,
    error: Option<String>,
}

/// Decode the WebView2 ExecuteScript result into the outcome. The result
/// arrives JSON-encoded (a string result is double-encoded, POC-M4B); an
/// object result is decoded once from its JSON text.
fn decode_interact_outcome(encoded: &str) -> Result<InteractOutcome, BrowserCommandError> {
    let inner = serde_json::from_str::<String>(encoded).unwrap_or_else(|_| encoded.to_string());
    let value: serde_json::Value = serde_json::from_str(&inner).map_err(|_| {
        BrowserCommandError::unavailable("Browser interact result is unreadable.", false)
    })?;
    Ok(InteractOutcome {
        ok: value
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        error: value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
}

/// Run one interact script in the live browser page (Windows only;
/// ExecuteScript callback runs on the UI thread — callers wait off the
/// async runtime).
#[cfg(all(windows, not(test)))]
fn execute_interact(
    webview: &tauri::WebviewWindow,
    script: &str,
) -> Result<InteractOutcome, BrowserCommandError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    webview
        .eval_with_callback(script, move |result| {
            let _ = sender.send(result);
        })
        .map_err(|_| BrowserCommandError::unavailable("Browser interact is unavailable.", true))?;
    let encoded = receiver
        .recv_timeout(INTERACT_TIMEOUT)
        .map_err(|_| BrowserCommandError::unavailable("Browser interact timed out.", true))?;
    decode_interact_outcome(&encoded)
}

/// Record a finished page load (state ready). The bootstrap about:blank
/// load is skipped by the caller so the create-time navigation intent wins.
pub(crate) fn mark_browser_ready(
    state: &BrowserState,
    session_id: &str,
) -> Result<BrowserSession, BrowserCommandError> {
    let mut bridge = state
        .inner
        .lock()
        .map_err(|_| BrowserCommandError::state_unavailable())?;
    bridge
        .registry()
        .mark_ready(session_id)
        .map_err(map_browser_error)
}

/// Record a failed load (state error + load_failed event).
pub(crate) fn mark_browser_load_failed(
    state: &BrowserState,
    session_id: &str,
    message: &str,
) -> Result<BrowserSession, BrowserCommandError> {
    let mut bridge = state
        .inner
        .lock()
        .map_err(|_| BrowserCommandError::state_unavailable())?;
    bridge
        .registry()
        .mark_load_failed(session_id, message)
        .map_err(map_browser_error)
}

// ----------------------------- Native surface (Windows) -----------------------------

/// Create the runtime browser window for a fresh session and navigate it to
/// the initial page. The per-webview user-data folder is supported by tauri
/// 2.11 (WebviewWindowBuilder::data_directory -> wry -> WebView2
/// CreateCoreWebView2EnvironmentWithOptions user_data_folder), so every
/// browser session gets an isolated profile (ADR-0017 decision 3).
#[cfg(all(windows, not(test)))]
fn spawn_browser_window(
    app: &AppHandle,
    state: &BrowserState,
    session: BrowserSession,
) -> Result<BrowserReport, BrowserCommandError> {
    let session_id = session.session_id.clone();
    let label = format!("{BROWSER_WINDOW_LABEL_PREFIX}{session_id}");
    let profile_dir = browser_profile_dir(app, &session_id)?;
    let page_state = state.clone();
    let page_session_id = session_id.clone();
    let blocked_navigation = Arc::new(AtomicBool::new(false));
    let navigation_gate = blocked_navigation.clone();

    let builder = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::External(Url::parse("about:blank").expect("valid bootstrap URL")),
    )
    .data_directory(profile_dir)
    .title("DSH Browser")
    .inner_size(960.0, 600.0)
    .zoom_hotkeys_enabled(false)
    .browser_extensions_enabled(false)
    .general_autofill_enabled(false)
    .devtools(false)
    .on_navigation(move |url| {
        // Browser navigation policy (ADR-0017 decision 3): http(s) only, no
        // userinfo, bounded length; about:blank stays the bootstrap target.
        let allowed = is_allowed_navigation_url(url);
        navigation_gate.store(!allowed, Ordering::SeqCst);
        allowed
    })
    .on_new_window(|_, _| NewWindowResponse::Deny)
    .on_download(|_, _| false)
    .on_page_load(move |_, payload| {
        let url = payload.url().as_str().to_string();
        if payload.event() == PageLoadEvent::Finished && url != "about:blank" {
            let _ = mark_browser_ready(&page_state, &page_session_id);
        }
    });

    let webview = builder.build().map_err(|_| {
        let _ = mark_browser_load_failed(state, &session_id, "window creation failed");
        BrowserCommandError::unavailable("Browser window creation failed.", false)
    })?;

    install_windows_deny_hooks(
        &webview,
        state.clone(),
        session_id.clone(),
        blocked_navigation,
    )
    .inspect_err(|_| {
        let _ = webview.close();
        let _ = mark_browser_load_failed(state, &session_id, "deny hooks failed");
    })?;

    {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| BrowserCommandError::state_unavailable())?;
        bridge.windows.insert(session_id.clone(), webview.clone());
    }

    // User-closed windows tear the session down: remove the handle and let
    // the registry close (enqueues the `closed` event, drained later).
    let teardown_state = state.clone();
    let teardown_session = session_id.clone();
    webview.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            cleanup_browser(&teardown_state, &teardown_session);
        }
    });

    // Deny-order invariant holds: the hooks are installed and the window is
    // registered before any remote navigation starts.
    let initial = BrowserNavigateRequest {
        schema_version: SCHEMA_VERSION,
        session_id: session_id.clone(),
        url: INITIAL_URL.to_string(),
    };
    let report = navigate_session(state, &initial)?;
    let url = Url::parse(INITIAL_URL).expect("static initial URL");
    webview.navigate(url).map_err(|_| {
        let _ = mark_browser_load_failed(state, &session_id, "navigation failed");
        BrowserCommandError::unavailable("Browser navigation failed.", true)
    })?;
    Ok(report)
}

/// Remove the window handle and close the registry session (idempotent).
#[cfg(all(windows, not(test)))]
fn cleanup_browser(state: &BrowserState, session_id: &str) {
    {
        let Ok(mut bridge) = state.inner.lock() else {
            return;
        };
        bridge.windows.remove(session_id);
    }
    let Ok(mut bridge) = state.inner.lock() else {
        return;
    };
    let _ = bridge.registry().close(session_id);
}

#[cfg(all(windows, not(test)))]
// Deny-order invariant (ADR-0011/0017, AC-BRW-001): all WebView2 deny hooks
// (permission, password save, autofill) are installed BEFORE any remote
// document loads; the window starts at about:blank and only navigates to
// policy-approved http(s) URLs afterwards.
fn install_windows_deny_hooks(
    webview: &tauri::WebviewWindow,
    state: BrowserState,
    session_id: String,
    blocked_navigation: Arc<AtomicBool>,
) -> Result<(), BrowserCommandError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    webview
        .with_webview(move |platform| {
            let result = (|| -> Result<(), ()> {
                let controller = platform.controller();
                let native = unsafe { controller.CoreWebView2().map_err(|_| ())? };
                let native13: ICoreWebView2_13 = native.cast().map_err(|_| ())?;
                let profile = unsafe { native13.Profile().map_err(|_| ())? };
                let profile6: ICoreWebView2Profile6 = profile.cast().map_err(|_| ())?;
                unsafe {
                    profile6
                        .SetIsPasswordAutosaveEnabled(false)
                        .map_err(|_| ())?;
                    profile6
                        .SetIsGeneralAutofillEnabled(false)
                        .map_err(|_| ())?;
                }

                let mut token = 0;
                let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
                    if let Some(args) = args {
                        unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)? };
                    }
                    Ok(())
                }));
                unsafe {
                    native
                        .add_PermissionRequested(&handler, &mut token)
                        .map_err(|_| ())?;
                }

                let navigation_handler =
                    NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                        if let Some(args) = args {
                            let mut succeeded = windows_core::BOOL(0);
                            unsafe { args.IsSuccess(&mut succeeded)? };
                            let policy_denied = blocked_navigation.swap(false, Ordering::SeqCst);
                            if !succeeded.as_bool() && !policy_denied {
                                let _ = mark_browser_load_failed(
                                    &state,
                                    &session_id,
                                    "navigation failed",
                                );
                            }
                        }
                        Ok(())
                    }));
                let mut navigation_token = 0;
                unsafe {
                    native
                        .add_NavigationCompleted(&navigation_handler, &mut navigation_token)
                        .map_err(|_| ())?;
                }
                Ok(())
            })();
            let _ = sender.send(result);
        })
        .map_err(|_| {
            BrowserCommandError::unavailable("Browser native hook dispatch failed.", true)
        })?;

    receiver
        .recv_timeout(NATIVE_HOOK_TIMEOUT)
        .map_err(|_| BrowserCommandError::unavailable("Browser native hook timed out.", true))?
        .map_err(|_| {
            BrowserCommandError::unavailable("Browser native hook installation failed.", true)
        })
}

#[cfg(all(windows, not(test)))]
fn browser_profile_dir(app: &AppHandle, session_id: &str) -> Result<PathBuf, BrowserCommandError> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join("browser-profiles").join(session_id))
        .map_err(|_| {
            BrowserCommandError::unavailable("Application data directory is unavailable.", true)
        })
}

/// Browser navigation policy shared by the Tauri gate and unit tests.
/// Mirrors specs/browser navigate constraints; userinfo is rejected because
/// URL credentials are a credential-exposure risk (RISK-BROWSER).
pub(crate) fn is_allowed_navigation_url(url: &Url) -> bool {
    if url.as_str().len() > MAX_URL_LEN {
        return false;
    }
    if url.scheme() == "about" {
        return url.as_str() == "about:blank";
    }
    (url.scheme() == "http" || url.scheme() == "https")
        && url.username().is_empty()
        && url.password().is_none()
}

// ----------------------------- Events -----------------------------

/// Drain pending browser events into the Shell WebView (AC-BRW-001: only the
/// Shell listens; browser pages have no IPC at all).
pub(crate) fn drain_events(app: &AppHandle, state: &BrowserState) {
    let events = {
        let Ok(mut bridge) = state.inner.lock() else {
            return;
        };
        bridge.registry().drain_events()
    };
    for event in events {
        let payload = BrowserEventPayload {
            schema_version: SCHEMA_VERSION,
            session_id: event.session_id,
            kind: event.kind.as_str().to_string(),
            occurred_at_unix_ms: event.occurred_at_unix_ms,
            url: event.url,
        };
        let _ = app.emit(EVENT_NAME, payload);
    }
}

/// Background drain task; started once by the app.
pub(crate) fn start_event_drain(app: AppHandle, state: BrowserState) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(EVENT_DRAIN_INTERVAL);
            drain_events(&app, &state);
        }
    });
}

// ----------------------------- Snapshot -----------------------------

/// Extract the visible page text via ExecuteScript. WebView2 returns the JS
/// result as a JSON-encoded string, so `document.body.innerText` arrives as
/// `"...text..."` — the double encoding must be decoded once (POC-M4B).
#[cfg(all(windows, not(test)))]
fn snapshot_text(webview: &tauri::WebviewWindow) -> Result<String, BrowserCommandError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    webview
        .eval_with_callback("document.body.innerText", move |result| {
            let _ = sender.send(result);
        })
        .map_err(|_| BrowserCommandError::unavailable("Browser snapshot is unavailable.", true))?;
    let encoded = receiver
        .recv_timeout(SNAPSHOT_TIMEOUT)
        .map_err(|_| BrowserCommandError::unavailable("Browser snapshot timed out.", true))?;
    Ok(decode_snapshot(&encoded))
}

fn decode_snapshot(encoded: &str) -> String {
    serde_json::from_str::<String>(encoded).unwrap_or_else(|_| encoded.to_string())
}

// ----------------------------- Errors -----------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCommandError {
    code: String,
    message: String,
    retryable: bool,
    correlation_id: String,
}

impl BrowserCommandError {
    fn malformed() -> Self {
        Self {
            code: "MALFORMED_MESSAGE".into(),
            message: "Browser request is malformed or the mode is not human_surface.".into(),
            retryable: false,
            correlation_id: correlation_id(),
        }
    }

    fn not_supported() -> Self {
        Self {
            code: "NOT_SUPPORTED".into(),
            message: "This browser snapshot mode is not supported in M4.".into(),
            retryable: false,
            correlation_id: correlation_id(),
        }
    }

    fn session_unavailable() -> Self {
        Self {
            code: "UNAVAILABLE".into(),
            message: "Browser session is unknown or already closed.".into(),
            retryable: false,
            correlation_id: correlation_id(),
        }
    }

    fn state_unavailable() -> Self {
        Self {
            code: "UNAVAILABLE".into(),
            message: "Browser state is unavailable.".into(),
            retryable: true,
            correlation_id: correlation_id(),
        }
    }

    /// Agent authorization failure (M5-E3, AC-BRW-002).
    fn unauthorized(message: &'static str, retryable: bool) -> Self {
        Self {
            code: "UNAUTHORIZED".into(),
            message: message.into(),
            retryable,
            correlation_id: correlation_id(),
        }
    }

    fn authorization(code: &'static str, message: &'static str, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            correlation_id: correlation_id(),
        }
    }

    fn unavailable(message: &'static str, retryable: bool) -> Self {
        Self {
            code: "UNAVAILABLE".into(),
            message: message.into(),
            retryable,
            correlation_id: correlation_id(),
        }
    }

    /// Command entry with no daemon connection installed (fail-closed).
    pub(crate) fn daemon_unavailable() -> Self {
        Self::from_daemon(DaemonCommandError::NotConnected)
    }

    /// Map a daemon invocation failure onto the browser command error
    /// contract (M6-C4): the daemon's protocol code/message/retryable pass
    /// through; connection-level failures are UNAVAILABLE + retryable.
    fn from_daemon(error: DaemonCommandError) -> Self {
        Self {
            code: error.wire_code(),
            message: error.message(),
            retryable: error.retryable(),
            correlation_id: correlation_id(),
        }
    }

    /// The daemon answered with a payload that does not match the
    /// expected wire shape (a shell/daemon contract mismatch).
    fn unavailable_response(method: &'static str) -> Self {
        Self {
            code: "UNAVAILABLE".into(),
            message: format!("The daemon returned an unexpected {method} response."),
            retryable: false,
            correlation_id: correlation_id(),
        }
    }
}

fn map_browser_error(error: BrowserError) -> BrowserCommandError {
    match error {
        BrowserError::NotFound => BrowserCommandError::session_unavailable(),
        BrowserError::InvalidUrl(_) => BrowserCommandError::malformed(),
        BrowserError::Closed => BrowserCommandError::session_unavailable(),
        BrowserError::Other => BrowserCommandError::state_unavailable(),
    }
}

fn map_url_error(error: UrlError) -> BrowserCommandError {
    match error {
        UrlError::Empty
        | UrlError::UnsupportedScheme
        | UrlError::UserinfoNotAllowed
        | UrlError::TooLong => BrowserCommandError::malformed(),
    }
}

/// Map a broker dispatch-gate rejection to the browser command error
/// contract (M5-E3, ADR-0018 decision 7).
///
/// ERROR_MODEL.md: UNAUTHORIZED covers "未授权、lease 无效或 scope 不符"; an
/// agent with no browser grant/lease at all is exactly that (the
/// capability itself always exists on the Desktop), so UnknownCapability
/// surfaces as UNAUTHORIZED, not UNAVAILABLE.
fn map_broker_error(error: dsh_supervisor::BrokerError) -> BrowserCommandError {
    let (code, message, retryable) = match error {
        dsh_supervisor::BrokerError::UnknownCapability
        | dsh_supervisor::BrokerError::NotGranted
        | dsh_supervisor::BrokerError::LeaseExpired
        | dsh_supervisor::BrokerError::LeaseRevoked
        | dsh_supervisor::BrokerError::ScopeMismatch => (
            "UNAUTHORIZED",
            "Agent is not authorized for browser automation.",
            false,
        ),
        dsh_supervisor::BrokerError::GenerationMismatch => (
            "STALE_GENERATION",
            "Agent browser request carries a stale generation.",
            false,
        ),
        dsh_supervisor::BrokerError::Conflict => {
            ("CONFLICT", "Browser broker state conflict.", false)
        }
        dsh_supervisor::BrokerError::UnknownProvider => {
            ("UNAVAILABLE", "Browser provider is not registered.", true)
        }
    };
    BrowserCommandError::authorization(code, message, retryable)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_request(mode: &str) -> BrowserCreateRequest {
        BrowserCreateRequest {
            schema_version: 1,
            mode: mode.to_string(),
        }
    }

    fn close_request(session_id: &str) -> BrowserCloseRequest {
        BrowserCloseRequest {
            schema_version: 1,
            session_id: session_id.to_string(),
        }
    }

    fn navigate_request(session_id: &str, url: &str) -> BrowserNavigateRequest {
        BrowserNavigateRequest {
            schema_version: 1,
            session_id: session_id.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn ac_brw_001_browser_label_never_matches_shell_capability() {
        // AC-BRW-001: the Browser page has no Desktop IPC. The Shell
        // capability targets only the trusted `shell` webview; runtime
        // browser windows are labeled browser-<session> and match nothing.
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/shell.json"))
                .expect("shell capability parses");
        let webviews = capability["webviews"]
            .as_array()
            .expect("capability webviews");
        assert_eq!(webviews.len(), 1);
        assert_eq!(webviews[0].as_str(), Some("shell"));
        for label in webviews {
            let label = label.as_str().expect("label string");
            assert!(
                !label.starts_with(BROWSER_WINDOW_LABEL_PREFIX),
                "capability must never target browser labels"
            );
            assert_ne!(label, "browser");
        }
        // The browser surface must not inherit any privileged command; the
        // Shell-only permissions for the bridge commands exist, so the
        // frontend can drive sessions without a browser-side grant.
        let permissions = capability["permissions"]
            .as_array()
            .expect("capability permissions");
        for expected in [
            "allow-create-browser",
            "allow-navigate-browser",
            "allow-snapshot-browser",
            "allow-close-browser",
            "allow-list-browsers",
        ] {
            assert!(
                permissions
                    .iter()
                    .any(|permission| permission.as_str() == Some(expected)),
                "missing Shell permission {expected}"
            );
        }
    }

    #[test]
    fn create_request_rejects_automation_mode() {
        // The schema-level const is enforced by validate-specs; the bridge
        // rejects the mode at the command boundary (ADR-0017 decision 2).
        let request = create_request("agent_automation");
        assert!(!request.is_valid());
        let state = BrowserState::default();
        let error = create_session(&state, &request).expect_err("automation rejected");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
    }

    #[test]
    fn session_request_validation_accepts_only_opaque_ids() {
        assert!(validate_session_request(1, "brw-1787000000000-1"));
        assert!(validate_session_request(1, "brw-1"));
        for invalid in [
            "browser-1",
            "brw-",
            "brw-1/2",
            "BRW-1",
            "brw-1_2",
            &"b".repeat(65),
        ] {
            assert!(
                !validate_session_request(1, invalid),
                "session id {invalid} must be rejected"
            );
        }
        assert!(!validate_session_request(2, "brw-1"));
    }

    #[test]
    fn navigation_policy_accepts_http_https_and_blank_only() {
        for url in [
            "https://example.com/",
            "https://example.com/path?q=1#fragment",
            "http://example.com/",
            "about:blank",
        ] {
            assert!(
                is_allowed_navigation_url(&Url::parse(url).expect("parse")),
                "{url}"
            );
        }
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,hello",
            "https://user@example.com/",
            "https://user:pass@example.com/",
            &format!("https://example.com/{}", "a".repeat(2048)),
        ] {
            assert!(
                !is_allowed_navigation_url(&Url::parse(url).expect("parse")),
                "{url}"
            );
        }
    }

    #[test]
    fn snapshot_modes_fail_closed_except_text() {
        let text = BrowserSnapshotRequest {
            schema_version: 1,
            session_id: "brw-1".to_string(),
            snapshot_mode: "text".to_string(),
        };
        assert!(text.is_valid());
        let screenshot = BrowserSnapshotRequest {
            schema_version: 1,
            session_id: "brw-1".to_string(),
            snapshot_mode: "screenshot".to_string(),
        };
        assert!(!screenshot.is_valid());
        assert!(!snapshot_mode_supported("accessibility"));
    }

    #[test]
    fn report_serialization_matches_schema_fixture() {
        let report = BrowserReport {
            schema_version: 1,
            session_id: "brw-1787000000000-1".to_string(),
            state: "ready".to_string(),
            mode: "human_surface".to_string(),
            current_url: Some("https://example.com/".to_string()),
            created_at_unix_ms: 1787000000000,
            last_activity_unix_ms: Some(1787000001000),
            error: None,
        };
        let actual = serde_json::to_value(&report).expect("serialize");
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../specs/browser/fixtures/browser-report.valid.json"
        ))
        .expect("fixture");
        assert_eq!(actual, fixture);
    }

    #[test]
    fn event_serialization_matches_schema_fixture() {
        let event = BrowserEventPayload {
            schema_version: 1,
            session_id: "brw-1787000000000-1".to_string(),
            kind: "navigation_changed".to_string(),
            occurred_at_unix_ms: 1787000001000,
            url: Some("https://example.com/".to_string()),
        };
        let actual = serde_json::to_value(&event).expect("serialize");
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../specs/browser/fixtures/browser-event.valid.json"
        ))
        .expect("fixture");
        assert_eq!(actual, fixture);
    }

    #[test]
    fn snapshot_report_flattens_report_and_text() {
        let snapshot = BrowserSnapshotReport {
            report: BrowserReport {
                schema_version: 1,
                session_id: "brw-1787000000000-1".to_string(),
                state: "ready".to_string(),
                mode: "human_surface".to_string(),
                current_url: Some("https://example.com/".to_string()),
                created_at_unix_ms: 1787000000000,
                last_activity_unix_ms: Some(1787000001000),
                error: None,
            },
            text: "Example Domain".to_string(),
        };
        let value = serde_json::to_value(&snapshot).expect("serialize");
        assert_eq!(value["sessionId"], "brw-1787000000000-1");
        assert_eq!(value["currentUrl"], "https://example.com/");
        assert_eq!(value["text"], "Example Domain");
        assert!(value.get("report").is_none(), "report must flatten");
    }

    #[test]
    fn decode_snapshot_handles_webview2_double_encoding() {
        // WebView2 ExecuteScript returns the JS result JSON-encoded, so the
        // innerText string arrives as a JSON string literal.
        assert_eq!(decode_snapshot("\"Example Domain\""), "Example Domain");
        // WebView2 JSON-escapes newlines inside the result string.
        assert_eq!(decode_snapshot(r#""line1\nline2""#), "line1\nline2");
        // Non-string results (null) fall back to the raw payload.
        assert_eq!(decode_snapshot("null"), "null");
    }

    #[test]
    fn bridge_session_lifecycle_matches_schema_states() {
        let state = BrowserState::default();
        let created = create_session(&state, &create_request("human_surface")).expect("create");
        assert!(created.session_id.starts_with("brw-"));
        assert_eq!(created.state.as_str(), "created");

        let navigating = navigate_request(&created.session_id, "https://example.com/");
        let report = navigate_session(&state, &navigating).expect("navigate");
        assert_eq!(report.state, "loading");
        assert_eq!(report.current_url.as_deref(), Some("https://example.com/"));

        let ready = mark_browser_ready(&state, &created.session_id).expect("ready");
        assert_eq!(ready.state.as_str(), "ready");

        let unknown = navigate_request("brw-1-999", "https://example.com/");
        let error = navigate_session(&state, &unknown).expect_err("unknown session");
        assert_eq!(error.code, "UNAVAILABLE");

        let closed = close_session(&state, &close_request(&created.session_id)).expect("close");
        assert_eq!(closed.state.as_str(), "closed");
        let reports = list_browsers_blocking(&state);
        let closed_report = reports
            .iter()
            .find(|report| report.session_id() == created.session_id)
            .expect("closed session remains listed");
        assert_eq!(closed_report.state, "closed");

        // Events drained in order: navigate enqueues navigation_changed,
        // close enqueues closed; ready transitions enqueue nothing.
        let events = {
            let mut bridge = state.inner.lock().unwrap();
            bridge.registry().drain_events()
        };
        let kinds: Vec<&str> = events.iter().map(|event| event.kind.as_str()).collect();
        assert_eq!(kinds, ["navigation_changed", "closed"]);
    }

    #[test]
    fn browser_capability_matches_agreement_fixture() {
        // 坐标真源是 specs/protocol/fixtures/envelope.agreement.valid.json 的 granted 坐标；
        // 门禁按 browser_capability() 判定，漂移会静默破坏 M6 协商→grant 接线（REVIEW-M5-INTEROP HIGH-1）。
        let fixture =
            include_str!("../../../../specs/protocol/fixtures/envelope.agreement.valid.json");
        assert!(
            fixture.contains("browser.dsh-desktop.local/v1alpha1"),
            "agreement fixture must grant the browser coordinate"
        );
        assert_eq!(
            browser_capability().to_string(),
            "browser.dsh-desktop.local/v1alpha1/Browser"
        );
    }

    #[test]
    fn browser_report_never_leaks_profile_paths() {
        // ADR-0017 decision 5: reports expose only id/state/currentUrl/created.
        let report = BrowserReport {
            schema_version: 1,
            session_id: "brw-1787000000000-1".to_string(),
            state: "loading".to_string(),
            mode: "human_surface".to_string(),
            current_url: Some("https://example.com/".to_string()),
            created_at_unix_ms: 1787000000000,
            last_activity_unix_ms: None,
            error: None,
        };
        let serialized = serde_json::to_string(&report).expect("serialize");
        assert!(!serialized.contains("browser-profiles"));
        assert!(!serialized.contains("AppData"));
        assert!(!serialized.contains("user-data"));
    }

    // --------------------- M5-E3 interact / take_over (AC-BRW-002) ---------------------

    fn agent_identity(session_id: &str) -> BrowserAgentIdentity {
        BrowserAgentIdentity {
            agent_id: "agent-1".to_string(),
            activation_id: "act-1".to_string(),
            generation: 1,
            scope: BrowserAgentScope {
                session_id: Some(session_id.to_string()),
                workspace: Some("ws-a".to_string()),
                domains: Vec::new(),
                resources: Vec::new(),
            },
        }
    }

    fn interact_request(session_id: &str, action: &str) -> BrowserInteractRequest {
        BrowserInteractRequest {
            schema_version: 1,
            session_id: session_id.to_string(),
            mode: "agent_automation".to_string(),
            action: action.to_string(),
            selector: None,
            text: None,
            key: None,
            delta_x: None,
            delta_y: None,
            agent: agent_identity(session_id),
        }
    }

    fn takeover_request(session_id: &str) -> BrowserTakeoverRequest {
        BrowserTakeoverRequest {
            schema_version: 1,
            session_id: session_id.to_string(),
            target: "human".to_string(),
        }
    }

    /// Negotiate a browser grant + lease into the state's shared broker
    /// (ADR-0018 chain: negotiation -> grant -> lease, same facts the
    /// interact request carries).
    fn grant_browser_lease(
        state: &BrowserState,
        agent_id: &str,
        activation_id: &str,
        session_id: &str,
    ) {
        let mut broker = state.broker.lock().unwrap();
        broker
            .broker_grant_from_negotiation(
                agent_id,
                dsh_supervisor::AgentNegotiationResult {
                    activation_id: activation_id.to_string(),
                    agreed: true,
                    granted: vec![browser_capability()],
                    conformance: dsh_supervisor::AgentConformanceState::Known,
                    lease_constraints: Some(dsh_supervisor::AgentLeaseConstraints::new(3600)),
                    scope: dsh_supervisor::Scope {
                        session_id: Some(session_id.to_string()),
                        workspace: Some("ws-a".to_string()),
                        domains: Vec::new(),
                        resources: Vec::new(),
                    },
                },
            )
            .expect("grant");
    }

    #[test]
    fn interact_request_validation_fails_closed() {
        let mut request = interact_request("brw-1", "click");
        request.selector = Some("#submit".to_string());
        assert!(request.is_valid());

        let mut bad_mode = request.clone();
        bad_mode.mode = "human_surface".to_string();
        assert!(!bad_mode.is_valid());

        let mut bad_action = request.clone();
        bad_action.action = "hover".to_string();
        assert!(!bad_action.is_valid());

        let mut no_action = request.clone();
        no_action.action = String::new();
        assert!(!no_action.is_valid());

        let mut bad_session = request.clone();
        bad_session.session_id = "browser-1".to_string();
        assert!(!bad_session.is_valid());

        let click_without_selector = interact_request("brw-1", "click");
        assert!(!click_without_selector.is_valid());

        let mut typed = interact_request("brw-1", "type");
        typed.selector = Some("#q".to_string());
        assert!(!typed.is_valid()); // missing text
        typed.text = Some("hi".to_string());
        assert!(typed.is_valid());
        typed.text = Some(String::new());
        assert!(!typed.is_valid()); // empty text
        typed.text = Some("x".repeat(4097));
        assert!(!typed.is_valid()); // over bound

        let mut scroll = interact_request("brw-1", "scroll");
        assert!(!scroll.is_valid()); // no delta
        scroll.delta_y = Some(400);
        assert!(scroll.is_valid());
        scroll.delta_y = Some(200_000);
        assert!(!scroll.is_valid()); // over bound

        let mut key = interact_request("brw-1", "key");
        assert!(!key.is_valid()); // missing key
        key.key = Some("Enter".to_string());
        assert!(key.is_valid());

        // Strict per-action params: click with text is malformed.
        let mut strict = request.clone();
        strict.text = Some("nope".to_string());
        assert!(!strict.is_valid());

        // Agent facts are required and validated (ADR-0018 decision 7).
        let mut bad_token = request.clone();
        bad_token.agent.agent_id = "bad id".to_string();
        assert!(!bad_token.is_valid());
        let mut bad_generation = request.clone();
        bad_generation.agent.generation = 0;
        assert!(!bad_generation.is_valid());
        let mut empty_scope = request.clone();
        empty_scope.agent.scope = BrowserAgentScope {
            session_id: None,
            workspace: None,
            domains: Vec::new(),
            resources: Vec::new(),
        };
        assert!(!empty_scope.is_valid());
    }

    #[test]
    fn takeover_request_validation_fails_closed() {
        let request = takeover_request("brw-1");
        assert!(request.is_valid());
        let mut bad_target = request.clone();
        bad_target.target = "agent".to_string();
        assert!(!bad_target.is_valid());
        let mut bad_session = request.clone();
        bad_session.session_id = "nope".to_string();
        assert!(!bad_session.is_valid());
    }

    #[test]
    fn interact_requires_agent_authorization() {
        // No grant at all: the broker gate rejects with UNAUTHORIZED
        // (fail-closed; AC-BRW-002) — a human session has no agent lease.
        let state = BrowserState::default();
        let created = create_session(&state, &create_request("human_surface")).expect("create");
        let mut request = interact_request(&created.session_id, "click");
        request.selector = Some("#submit".to_string());
        let error = authorize_interact(&state, &request).expect_err("no grant");
        assert_eq!(error.code, "UNAUTHORIZED");
        // Nothing was bound: a takeover of the session revokes nothing.
        assert_eq!(
            take_over_browser(&state, &takeover_request(&created.session_id))
                .expect("takeover")
                .state,
            "created"
        );
    }

    #[test]
    fn interact_passes_with_valid_agent_lease() {
        let state = BrowserState::default();
        let created = create_session(&state, &create_request("human_surface")).expect("create");
        grant_browser_lease(&state, "agent-1", "act-1", &created.session_id);
        let mut request = interact_request(&created.session_id, "click");
        request.selector = Some("#submit".to_string());
        authorize_interact(&state, &request).expect("authorized interact");
        // The session binding is recorded so a human takeover can revoke
        // exactly this activation.
        let bridge = state.inner.lock().unwrap();
        let binding = bridge
            .bindings
            .get(&created.session_id)
            .expect("binding recorded");
        assert_eq!(binding.agent_id, "agent-1");
        assert_eq!(binding.activation_id, "act-1");
        assert_eq!(binding.generation, 1);
    }

    #[test]
    fn interact_rejected_for_session_outside_grant_scope() {
        let state = BrowserState::default();
        let created = create_session(&state, &create_request("human_surface")).expect("create");
        grant_browser_lease(&state, "agent-1", "act-1", &created.session_id);
        // A second human session is outside the grant scope: the agent
        // scope targets the other session while the grant covers only the
        // created one — both the scope-confusion guard and the broker
        // coverage rule fail closed (UNAUTHORIZED).
        let other = create_session(&state, &create_request("human_surface")).expect("create");
        let mut request = interact_request(&other.session_id, "click");
        request.selector = Some("#submit".to_string());
        let error = authorize_interact(&state, &request).expect_err("scope mismatch");
        assert_eq!(error.code, "UNAUTHORIZED");
    }

    #[test]
    fn interact_scope_confusion_guard_rejects_mismatched_target() {
        // The agent scope pins session A but the mutation targets session B:
        // rejected before the broker gate (scope confusion guard).
        let state = BrowserState::default();
        let created = create_session(&state, &create_request("human_surface")).expect("create");
        grant_browser_lease(&state, "agent-1", "act-1", &created.session_id);
        let mut request = interact_request(&created.session_id, "click");
        request.selector = Some("#submit".to_string());
        request.agent.scope.session_id = Some("brw-other".to_string());
        let error = authorize_interact(&state, &request).expect_err("scope confusion");
        assert_eq!(error.code, "UNAUTHORIZED");
        assert_eq!(error.message, "Agent scope targets a different session.");
    }

    #[test]
    fn takeover_revokes_agent_lease_and_blocks_later_interact() {
        // AC-BRW-002 end-to-end at the bridge level: authorized interact,
        // then human takeover — the broker durably revokes the activation
        // and the same activation can never interact again.
        let state = BrowserState::default();
        let created = create_session(&state, &create_request("human_surface")).expect("create");
        grant_browser_lease(&state, "agent-1", "act-1", &created.session_id);
        let mut request = interact_request(&created.session_id, "click");
        request.selector = Some("#submit".to_string());
        authorize_interact(&state, &request).expect("authorized");

        let report =
            take_over_browser(&state, &takeover_request(&created.session_id)).expect("takeover");
        assert_eq!(report.state, "created");

        // The broker durably revoked the activation (AC-LEASE-001
        // HumanTakeover), and the binding is drained.
        assert!(
            state
                .broker
                .lock()
                .unwrap()
                .agent_activation_revoked("agent-1", "act-1")
        );
        let bridge = state.inner.lock().unwrap();
        assert!(!bridge.bindings.contains_key(&created.session_id));
        drop(bridge);
        // Later interact with the same activation is rejected.
        let error = authorize_interact(&state, &request).expect_err("interact after takeover");
        assert_eq!(error.code, "UNAUTHORIZED");
    }

    #[test]
    fn takeover_marks_session_human_controlled() {
        // Even a fresh activation granted after the takeover cannot interact
        // with the taken-over session: the session stays human-controlled
        // (fail-closed) until an explicit agent handover exists (M5+).
        let state = BrowserState::default();
        let created = create_session(&state, &create_request("human_surface")).expect("create");
        take_over_browser(&state, &takeover_request(&created.session_id)).expect("takeover");
        grant_browser_lease(&state, "agent-2", "act-2", &created.session_id);
        let mut request = interact_request(&created.session_id, "click");
        request.selector = Some("#submit".to_string());
        request.agent.agent_id = "agent-2".to_string();
        request.agent.activation_id = "act-2".to_string();
        let error = authorize_interact(&state, &request).expect_err("human-controlled session");
        assert_eq!(error.code, "UNAUTHORIZED");
        assert_eq!(error.message, "Browser session is human-controlled.");
    }

    #[test]
    fn takeover_rejects_unknown_session() {
        let state = BrowserState::default();
        let error =
            take_over_browser(&state, &takeover_request("brw-1-999")).expect_err("unknown session");
        assert_eq!(error.code, "UNAVAILABLE");
    }

    #[test]
    fn takeover_is_idempotent() {
        let state = BrowserState::default();
        let created = create_session(&state, &create_request("human_surface")).expect("create");
        take_over_browser(&state, &takeover_request(&created.session_id)).expect("first");
        take_over_browser(&state, &takeover_request(&created.session_id)).expect("second");
    }

    #[test]
    fn interact_script_embeds_params_json_encoded() {
        // Injection safety: caller-supplied strings are embedded as JSON
        // string literals; a hostile selector can never escape the script.
        let mut request = interact_request("brw-1", "click");
        request.selector = Some("x\");alert(1);//".to_string());
        let script = interact_script(&request).expect("script");
        assert!(
            script.contains("\"x\\\");alert(1);//\""),
            "script: {script}"
        );
        assert!(
            !script.contains("x\");alert(1);//"),
            "raw injection must not appear"
        );

        let mut typed = interact_request("brw-1", "type");
        typed.selector = Some("#q".to_string());
        typed.text = Some("a\";alert(1);//".to_string());
        let script = interact_script(&typed).expect("script");
        assert!(script.contains("\"a\\\";alert(1);//\""), "script: {script}");
        assert!(script.contains("InputEvent"), "input event dispatch");

        let mut key = interact_request("brw-1", "key");
        key.key = Some("Enter".to_string());
        let script = interact_script(&key).expect("script");
        assert!(script.contains("KeyboardEvent"), "keyboard event dispatch");

        let mut scroll = interact_request("brw-1", "scroll");
        scroll.delta_x = Some(10);
        scroll.delta_y = Some(-20);
        let script = interact_script(&scroll).expect("script");
        assert!(
            script.contains("window.scrollBy(10,-20)"),
            "script: {script}"
        );
    }

    #[test]
    fn decode_interact_outcome_handles_webview2_encoding() {
        // ExecuteScript returns the JS object as JSON text (a string result
        // would arrive double-encoded, POC-M4B); both decode into the
        // outcome.
        let ok = decode_interact_outcome("{\"ok\":true,\"error\":null}").expect("object result");
        assert!(ok.ok);
        assert_eq!(ok.error, None);
        let missing =
            decode_interact_outcome("\"{\\\"ok\\\":false,\\\"error\\\":\\\"not_found\\\"}\"")
                .expect("string-encoded object result");
        assert!(!missing.ok);
        assert_eq!(missing.error.as_deref(), Some("not_found"));
        // Non-JSON results fail closed.
        assert!(decode_interact_outcome("garbage").is_err());
    }

    fn list_browsers_blocking(state: &BrowserState) -> Vec<BrowserReport> {
        let sessions = match state.inner.lock() {
            Ok(mut bridge) => bridge.registry().list(),
            Err(_) => return Vec::new(),
        };
        sessions.into_iter().map(public_report).collect()
    }

    // ------------------------------------------------------------------
    // M6-C4 daemon proxy (mock connector)
    // ------------------------------------------------------------------

    use crate::daemon_client::DaemonCommandError;
    use crate::daemon_client::tests::MockConnector;
    use dsh_daemon::envelope::ErrorCode;

    fn daemon_report_json(session_id: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "sessionId": session_id,
            "state": "created",
            "mode": "human_surface",
            "currentUrl": null,
            "createdAtUnixMs": 1_787_000_000_000u64,
            "lastActivityUnixMs": null,
            "error": null,
        })
    }

    #[test]
    fn create_proxies_to_daemon_and_attaches_locally() {
        let connector = MockConnector::ok(daemon_report_json("brw-1787000000000-1"));
        let state = BrowserState::default();
        let report = invoke_browser_create(&connector, &create_request("human_surface"))
            .expect("daemon create");
        assert_eq!(report.session_id(), "brw-1787000000000-1");
        assert_eq!(report.state, "created");
        let calls = connector.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Browser");
        assert_eq!(calls[0].1, "browser.create");
        assert_eq!(calls[0].2["mode"], "human_surface");

        // The daemon session is adopted into the local render registry so
        // the local navigate/snapshot paths see it.
        let attached = attach_daemon_session(&state, &report).expect("attach");
        assert_eq!(attached.session_id, "brw-1787000000000-1");
        assert_eq!(attached.state, SessionState::Created);
        let navigated = navigate_session(
            &state,
            &BrowserNavigateRequest {
                schema_version: 1,
                session_id: "brw-1787000000000-1".into(),
                url: "https://example.com".into(),
            },
        )
        .expect("local navigate on the attached session");
        assert_eq!(navigated.state, "loading");
    }

    #[test]
    fn create_rejects_unsupported_mode_before_invocation() {
        let connector = MockConnector::ok(daemon_report_json("brw-1"));
        let error = invoke_browser_create(&connector, &create_request("agent_automation"))
            .expect_err("automation rejected");
        assert_eq!(error.code, "MALFORMED_MESSAGE");
        assert!(connector.calls().is_empty());
    }

    #[test]
    fn create_daemon_failure_maps_to_command_error() {
        let connector = MockConnector::error(DaemonCommandError::Remote {
            code: ErrorCode::Unavailable,
            message: "browser host unavailable".into(),
            retryable: true,
        });
        let error = invoke_browser_create(&connector, &create_request("human_surface"))
            .expect_err("remote");
        assert_eq!(error.code, "UNAVAILABLE");
        assert!(error.retryable);

        let connector = MockConnector::error(DaemonCommandError::NotConnected);
        let error = invoke_browser_create(&connector, &create_request("human_surface"))
            .expect_err("offline");
        assert_eq!(error.code, "UNAVAILABLE");
        assert!(error.retryable);
    }

    #[test]
    fn close_proxies_to_daemon() {
        let connector = MockConnector::ok(daemon_report_json("brw-1787000000000-1"));
        let closed = invoke_browser_close(
            &connector,
            &BrowserCloseRequest {
                schema_version: 1,
                session_id: "brw-1787000000000-1".into(),
            },
        )
        .expect("daemon close");
        assert_eq!(closed.session_id(), "brw-1787000000000-1");
        assert_eq!(closed.state, "created");
        let calls = connector.calls();
        assert_eq!(calls[0].1, "browser.close");
        assert_eq!(calls[0].2["sessionId"], "brw-1787000000000-1");
    }

    #[test]
    fn list_proxies_to_daemon() {
        let connector = MockConnector::ok(serde_json::json!({
            "browsers": [
                daemon_report_json("brw-1"),
                daemon_report_json("brw-2"),
            ]
        }));
        let listed = futures_block_on(list_browsers(&connector)).expect("daemon list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].session_id(), "brw-2");
        assert_eq!(connector.calls()[0].1, "browser.list");
    }

    #[test]
    fn list_fails_closed_when_daemon_unavailable() {
        let connector = MockConnector::error(DaemonCommandError::NotConnected);
        let error = futures_block_on(list_browsers(&connector)).expect_err("offline");
        assert_eq!(error.code, "UNAVAILABLE");
    }

    fn futures_block_on<F: std::future::Future>(future: F) -> F::Output {
        tauri::async_runtime::block_on(future)
    }
}
