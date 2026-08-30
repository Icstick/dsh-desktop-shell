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
//! - Popups, downloads and devtools are denied; `agent_automation` mode and
//!   `screenshot` snapshots fail closed (ADR-0017 decision 2; M5).
//!
//! Events are relayed to the Shell WebView on `browser://event`
//! (navigation_changed / load_failed / closed), mirroring terminal://output.

use std::collections::HashMap;
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

use dsh_browser_provider::{BrowserError, BrowserSession, SessionRegistry, UrlError, UrlPolicy};

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

// ----------------------------- Requests -----------------------------

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserReport {
    schema_version: u8,
    session_id: String,
    state: String,
    mode: &'static str,
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
        mode: "human_surface",
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

/// App-managed browser state: one registry + the live window handles.
#[derive(Clone, Default)]
pub struct BrowserState {
    inner: Arc<Mutex<BrowserBridge>>,
}

struct BrowserBridge {
    /// Session state machine from crates/browser-provider.
    registry: Option<SessionRegistry>,
    /// Live window per session. The WebviewWindow re-derives the native
    /// ICoreWebView2 controller on demand; no raw COM handle is persisted.
    windows: HashMap<String, WindowHandle>,
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

pub(crate) async fn create_browser(
    app: &AppHandle,
    state: &BrowserState,
    request: &BrowserCreateRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    let session = create_session(state, request)?;

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

/// Registry-only part of navigate (URL policy + state machine).
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
    state: &BrowserState,
    request: &BrowserCloseRequest,
) -> Result<BrowserReport, BrowserCommandError> {
    let closed = close_session(state, request)?;
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
        // is idempotent on an already-closed session, so the closed event is
        // emitted exactly once (by the registry).
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
    Ok(public_report(closed))
}

pub(crate) async fn list_browsers(state: &BrowserState) -> Vec<BrowserReport> {
    let sessions = match state.inner.lock() {
        Ok(mut bridge) => bridge.registry().list(),
        Err(_) => return Vec::new(),
    };
    sessions.into_iter().map(public_report).collect()
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
    code: &'static str,
    message: &'static str,
    retryable: bool,
    correlation_id: String,
}

impl BrowserCommandError {
    fn malformed() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Browser request is malformed or the mode is not human_surface.",
            retryable: false,
            correlation_id: correlation_id(),
        }
    }

    fn not_supported() -> Self {
        Self {
            code: "NOT_SUPPORTED",
            message: "This browser snapshot mode is not supported in M4.",
            retryable: false,
            correlation_id: correlation_id(),
        }
    }

    fn session_unavailable() -> Self {
        Self {
            code: "UNAVAILABLE",
            message: "Browser session is unknown or already closed.",
            retryable: false,
            correlation_id: correlation_id(),
        }
    }

    fn state_unavailable() -> Self {
        Self {
            code: "UNAVAILABLE",
            message: "Browser state is unavailable.",
            retryable: true,
            correlation_id: correlation_id(),
        }
    }

    fn unavailable(message: &'static str, retryable: bool) -> Self {
        Self {
            code: "UNAVAILABLE",
            message,
            retryable,
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
            mode: "human_surface",
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
                mode: "human_surface",
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
    fn browser_report_never_leaks_profile_paths() {
        // ADR-0017 decision 5: reports expose only id/state/currentUrl/created.
        let report = BrowserReport {
            schema_version: 1,
            session_id: "brw-1787000000000-1".to_string(),
            state: "loading".to_string(),
            mode: "human_surface",
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

    fn list_browsers_blocking(state: &BrowserState) -> Vec<BrowserReport> {
        let sessions = match state.inner.lock() {
            Ok(mut bridge) => bridge.registry().list(),
            Err(_) => return Vec::new(),
        };
        sessions.into_iter().map(public_report).collect()
    }
}
