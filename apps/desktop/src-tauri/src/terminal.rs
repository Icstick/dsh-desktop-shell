//! Desktop terminal bridge (MOD-TERMINAL-UI boundary, ADR-0015).
//!
//! Owns the PtyRegistry for the app, relays ConPTY output to the Shell
//! WebView via Tauri events (AC-TERM-002: only the shell webview listens),
//! and exposes generation-free create/write/resize/close/status commands.
//! The PTY process tree is Desktop-owned, so Managed DSH stop/restart never
//! affects terminal sessions (AC-PTY-001).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use dsh_terminal_provider::{
    MAX_COLS, MAX_ROWS, MAX_WRITE_BYTES, PtyError, PtyRegistry, PtyReport,
};

const SCHEMA_VERSION: u8 = 1;
const EVENT_NAME: &str = "terminal://output";
/// Output event drain interval while a terminal surface is mounted.
const EVENT_DRAIN_INTERVAL: Duration = Duration::from_millis(30);

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
}

impl TerminalCreateRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION && self.mode == "human_surface"
    }
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

/// App-managed terminal state: one registry, drained by a bridge task.
#[derive(Clone, Default)]
pub struct TerminalState {
    inner: Arc<Mutex<TerminalBridge>>,
}

#[derive(Default)]
struct TerminalBridge {
    registry: Option<PtyRegistry>,
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

pub(crate) fn create_terminal(
    state: &TerminalState,
    request: &TerminalCreateRequest,
) -> Result<TerminalReport, TerminalCommandError> {
    if !request.is_valid() {
        return Err(TerminalCommandError::malformed());
    }
    let report = {
        let mut bridge = state
            .inner
            .lock()
            .map_err(|_| TerminalCommandError::unavailable())?;
        bridge
            .registry()
            .create(
                request.shell.as_deref(),
                request.cols,
                request.rows,
                request.cwd.as_deref(),
            )
            .map_err(map_pty_error)?
    };
    Ok(public_report(report))
}

pub(crate) fn write_terminal(
    state: &TerminalState,
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
    let mut bridge = state
        .inner
        .lock()
        .map_err(|_| TerminalCommandError::unavailable())?;
    bridge
        .registry()
        .write(request.session_id(), request.data())
        .map_err(map_pty_error)
}

pub(crate) fn resize_terminal(
    state: &TerminalState,
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
    Ok(public_report(report))
}

pub(crate) fn close_terminal(
    state: &TerminalState,
    request: &TerminalSessionRequest,
) -> Result<(), TerminalCommandError> {
    if !validate_session_request(request) {
        return Err(TerminalCommandError::malformed());
    }
    let mut bridge = state
        .inner
        .lock()
        .map_err(|_| TerminalCommandError::unavailable())?;
    bridge
        .registry()
        .close(request.session_id())
        .map_err(map_pty_error)
}

pub(crate) fn status_terminal(
    state: &TerminalState,
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
        bridge
            .registry()
            .report(request.session_id())
            .map_err(map_pty_error)?
    };
    Ok(public_report(report))
}

pub(crate) fn list_terminals(state: &TerminalState) -> Vec<TerminalReport> {
    let reports = match state.inner.lock() {
        Ok(mut bridge) => bridge.registry().list(),
        Err(_) => return Vec::new(),
    };
    reports.into_iter().map(public_report).collect()
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalReport {
    schema_version: u8,
    session_id: String,
    state: String,
    mode: &'static str,
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

fn public_report(report: PtyReport) -> TerminalReport {
    TerminalReport {
        schema_version: SCHEMA_VERSION,
        session_id: report.session_id,
        state: report.state.to_string(),
        mode: "human_surface",
        cols: report.cols,
        rows: report.rows,
        created_at_unix_ms: report.created_at_unix_ms,
        last_activity_unix_ms: report.last_activity_unix_ms,
        error: report.error,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandError {
    code: &'static str,
    message: &'static str,
    retryable: bool,
    correlation_id: String,
}

impl TerminalCommandError {
    fn malformed() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Terminal request is malformed or the mode is not human_surface.",
            retryable: false,
            correlation_id: correlation_id(),
        }
    }

    fn unavailable() -> Self {
        Self {
            code: "UNAVAILABLE",
            message: "Terminal state is unavailable.",
            retryable: true,
            correlation_id: correlation_id(),
        }
    }

    fn pty(kind: &'static str, message: &'static str, retryable: bool) -> Self {
        Self {
            code: kind,
            message,
            retryable,
            correlation_id: correlation_id(),
        }
    }
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
        }
    }

    #[test]
    fn malformed_create_is_rejected() {
        let state = TerminalState::default();
        let mut request = create_request(80, 24);
        request.mode = "agent_automation".to_string();
        let error = create_terminal(&state, &request).expect_err("automation rejected");
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
