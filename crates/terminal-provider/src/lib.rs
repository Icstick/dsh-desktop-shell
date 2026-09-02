//! Desktop-owned PTY sessions (MOD-TERMINAL-PROVIDER, ADR-0015).
//!
//! The provider owns the PTY lifecycle: it creates a pseudo console /
//! pseudo terminal (Windows ConPTY or Unix openpty), spawns the user's
//! shell as a child of THIS process (never of the Managed DSH process
//! tree), relays output as events, and cleans up on close or Drop.
//! Session ids are opaque; callers never see PIDs or paths (AC-TERM-002).

mod platform;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use platform::PlatformSession;

/// Upper bounds enforced by the provider (mirror specs/terminal schemas).
pub const MIN_COLS: u16 = 20;
pub const MAX_COLS: u16 = 500;
pub const MIN_ROWS: u16 = 5;
pub const MAX_ROWS: u16 = 300;
pub const MAX_WRITE_BYTES: usize = 8192;
pub const MAX_OUTPUT_EVENT_BYTES: usize = 65536;

/// Errors returned by the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtyError {
    InvalidShell,
    SpawnUnavailable,
    UnknownSession,
    InvalidGeometry,
    WriteTooLarge,
    Io,
    StateUnavailable,
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidShell => "shell selection is not supported",
            Self::SpawnUnavailable => "pseudo console or shell spawn failed",
            Self::UnknownSession => "unknown or closed PTY session",
            Self::InvalidGeometry => "geometry outside schema bounds",
            Self::WriteTooLarge => "write payload exceeds the schema bound",
            Self::Io => "PTY I/O failed",
            Self::StateUnavailable => "PTY registry state unavailable",
        };
        write!(f, "{message}")
    }
}

impl std::error::Error for PtyError {}

/// One chunk of terminal output delivered to the surface (AC-TERM-002).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputEvent {
    pub session_id: String,
    pub seq: u64,
    pub data: String,
}

/// Snapshot of one session's public state (no pid/path exposure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyReport {
    pub session_id: String,
    pub state: &'static str,
    pub cols: u16,
    pub rows: u16,
    pub created_at_unix_ms: u64,
    pub last_activity_unix_ms: Option<u64>,
    pub error: Option<String>,
}
impl PtyRegistry {
    /// Enqueue one write to the session's stdin.
    ///
    /// # Errors
    ///
    /// Returns `PtyError::UnknownSession`, `PtyError::WriteTooLarge` or
    /// `PtyError::Io` when the OS write fails.
    pub fn write(&self, session_id: &str, data: &str) -> Result<(), PtyError> {
        if data.len() > MAX_WRITE_BYTES {
            return Err(PtyError::WriteTooLarge);
        }
        let session = self.session(session_id)?;
        let mut guard = session.lock().map_err(|_| PtyError::StateUnavailable)?;
        if guard.closed {
            return Err(PtyError::UnknownSession);
        }
        guard.platform.write(data)?;
        guard.last_activity_unix_ms = Some(unix_ms());
        Ok(())
    }

    /// Resize the pseudo console.
    ///
    /// # Errors
    ///
    /// Returns `PtyError::InvalidGeometry` outside schema bounds, or
    /// `PtyError::UnknownSession` for unknown ids.
    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<PtyReport, PtyError> {
        if !(MIN_COLS..=MAX_COLS).contains(&cols) || !(MIN_ROWS..=MAX_ROWS).contains(&rows) {
            return Err(PtyError::InvalidGeometry);
        }
        let session = self.session(session_id)?;
        let mut guard = session.lock().map_err(|_| PtyError::StateUnavailable)?;
        if guard.closed {
            return Err(PtyError::UnknownSession);
        }
        guard.platform.resize(cols, rows)?;
        guard.cols = cols;
        guard.rows = rows;
        guard.last_activity_unix_ms = Some(unix_ms());
        Ok(guard.report())
    }

    /// Close a session idempotently; the child and console are terminated.
    pub fn close(&self, session_id: &str) -> Result<(), PtyError> {
        let removed = self
            .sessions
            .lock()
            .map_err(|_| PtyError::StateUnavailable)?
            .remove(session_id);
        match removed {
            Some(session) => {
                let mut guard = session.lock().map_err(|_| PtyError::StateUnavailable)?;
                guard.terminate();
                Ok(())
            }
            None => Err(PtyError::UnknownSession),
        }
    }

    /// Latest report for one session.
    ///
    /// # Errors
    ///
    /// Returns `PtyError::UnknownSession` for unknown ids.
    pub fn report(&self, session_id: &str) -> Result<PtyReport, PtyError> {
        let session = self.session(session_id)?;
        let guard = session.lock().map_err(|_| PtyError::StateUnavailable)?;
        Ok(guard.report())
    }

    /// All live session reports (surface restore/list).
    pub fn list(&self) -> Vec<PtyReport> {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::with_capacity(sessions.len());
        for session in sessions.values() {
            if let Ok(guard) = session.lock() {
                out.push(guard.report());
            }
        }
        out
    }

    /// Receive the next output event, if any (non-blocking).
    pub fn try_next_event(&self) -> Option<OutputEvent> {
        self.events.lock().ok()?.try_recv().ok()
    }

    /// Receive the next output event with a deadline.
    pub fn recv_event_timeout(&self, timeout: Duration) -> Option<OutputEvent> {
        self.events.lock().ok()?.recv_timeout(timeout).ok()
    }

    fn session(&self, session_id: &str) -> Result<Arc<Mutex<Session>>, PtyError> {
        self.sessions
            .lock()
            .map_err(|_| PtyError::StateUnavailable)?
            .get(session_id)
            .cloned()
            .ok_or(PtyError::UnknownSession)
    }
}

impl Drop for PtyRegistry {
    fn drop(&mut self) {
        // Drop the sender so reader threads unblock, then terminate all.
        if let Ok(mut sink) = self.event_sink.lock() {
            *sink = None;
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            for session in sessions.drain() {
                if let Ok(mut guard) = session.1.lock() {
                    guard.terminate();
                }
            }
        }
    }
}
#[derive(Debug)]
struct Session {
    id: String,
    cols: u16,
    rows: u16,
    created_at_unix_ms: u64,
    last_activity_unix_ms: Option<u64>,
    closed: bool,
    error: Option<String>,
    reader: Option<JoinHandle<()>>,
    /// Set by terminate() so the polling reader loop exits promptly; the
    /// reader never blocks indefinitely (poll/PeekNamedPipe).
    reader_stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Platform PTY state (ConPTY on Windows, openpty on Unix).
    platform: PlatformSession,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.terminate();
    }
}

impl Session {
    fn terminate(&mut self) {
        // Teardown order (M6-C1 deadlock finding): the reader polls
        // (PeekNamedPipe / poll) and never blocks indefinitely, so
        // stopping it is just a flag. Sequence: signal stop -> platform
        // teardown (close write end, kill the child, close the console /
        // reap the child) -> join the reader (exits within one poll tick)
        // -> platform closes the read side.
        self.reader_stop
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if !self.closed {
            self.closed = true;
            // Teardown phase 1: stop the child and release the writer side.
            // The read side (Windows read pipe / Unix master fd) stays open
            // until the reader is joined: closing it early either corrupts
            // the heap on Windows (0xC0000374) or lets the fd be reused and
            // a stale reader poll reads another session's output on Unix.
            self.platform.terminate_io();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        // Teardown phase 2: the reader is gone; close the read side.
        self.platform.close_read();
    }

    fn report(&self) -> PtyReport {
        PtyReport {
            session_id: self.id.clone(),
            state: if self.closed { "closed" } else { "running" },
            cols: self.cols,
            rows: self.rows,
            created_at_unix_ms: self.created_at_unix_ms,
            last_activity_unix_ms: self.last_activity_unix_ms,
            error: self.error.clone(),
        }
    }
}

/// PTY session registry. Owns every session; Drop terminates all of them.
pub struct PtyRegistry {
    next_id: AtomicU64,
    sessions: Mutex<HashMap<String, Arc<Mutex<Session>>>>,
    events: Mutex<mpsc::Receiver<OutputEvent>>,
    event_sink: Mutex<Option<mpsc::SyncSender<OutputEvent>>>,
}

impl Default for PtyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyRegistry {
    /// Create a new registry with a bounded event queue.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel::<OutputEvent>(256);
        Self {
            next_id: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
            events: Mutex::new(rx),
            event_sink: Mutex::new(Some(tx)),
        }
    }

    /// Create a PTY running the selected shell (default: cmd.exe).
    ///
    /// # Errors
    ///
    /// Returns PtyError::InvalidGeometry when cols/rows are outside the schema
    /// bounds, PtyError::InvalidShell for unsupported shells, and
    /// PtyError::SpawnUnavailable when ConPTY/process creation fails.
    pub fn create(
        &self,
        shell: Option<&str>,
        cols: u16,
        rows: u16,
        cwd: Option<&str>,
    ) -> Result<PtyReport, PtyError> {
        if !(MIN_COLS..=MAX_COLS).contains(&cols) || !(MIN_ROWS..=MAX_ROWS).contains(&rows) {
            return Err(PtyError::InvalidGeometry);
        }
        let shell_path = platform::resolve_shell(shell)?;
        let id = format!(
            "pty-{}-{}",
            unix_ms(),
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let now = unix_ms();
        let event_tx = self
            .event_sink
            .lock()
            .map_err(|_| PtyError::StateUnavailable)?
            .clone()
            .ok_or(PtyError::StateUnavailable)?;
        let platform_session = PlatformSession::spawn(&shell_path, cols, rows, cwd)
            .map_err(|_| PtyError::SpawnUnavailable)?;
        let reader_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = PlatformSession::spawn_reader(
            id.clone(),
            platform_session.reader_token(),
            event_tx,
            Arc::clone(&reader_stop),
        );
        let session = Arc::new(Mutex::new(Session {
            id: id.clone(),
            cols,
            rows,
            created_at_unix_ms: now,
            last_activity_unix_ms: None,
            closed: false,
            error: None,
            reader: Some(reader),
            reader_stop,
            platform: platform_session,
        }));
        self.sessions
            .lock()
            .map_err(|_| PtyError::StateUnavailable)?
            .insert(id.clone(), Arc::clone(&session));
        let guard = session.lock().map_err(|_| PtyError::StateUnavailable)?;
        Ok(guard.report())
    }
}
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression (M6-C1): closing a session with no output in flight used
    /// to deadlock — CloseHandle on the read pipe blocked on the reader's
    /// pending ReadFile. Terminate now closes the child stdin first, joins
    /// the reader, then closes the read handle. This test fails (hangs)
    /// on the old order; the channel timeout bounds it.
    #[test]
    fn close_with_no_pending_output_does_not_deadlock() {
        let registry = PtyRegistry::new();
        let report = registry.create(None, 80, 24, None).expect("create pty");
        let id = report.session_id.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), PtyError>>();
        std::thread::spawn(move || {
            let _ = tx.send(registry.close(&id));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => panic!("close returned error: {e}"),
            Err(_) => panic!("close deadlocked (did not return within 5s)"),
        }
    }

    #[test]
    fn geometry_bounds_are_enforced() {
        let registry = PtyRegistry::new();
        assert_eq!(
            registry
                .create(None, 10, 30, None)
                .expect_err("cols too small"),
            PtyError::InvalidGeometry
        );
        assert_eq!(
            registry
                .create(None, 120, 400, None)
                .expect_err("rows too large"),
            PtyError::InvalidGeometry
        );
        assert_eq!(
            // "bash" is a valid shell on unix; use a platform-neutral
            // unknown name for the unsupported-shell assertion.
            registry
                .create(Some("not-a-shell"), 120, 30, None)
                .expect_err("unsupported shell"),
            PtyError::InvalidShell
        );
    }

    #[test]
    fn unknown_session_errors() {
        let registry = PtyRegistry::new();
        assert_eq!(
            registry.write("pty-nope", "echo hi").expect_err("unknown"),
            PtyError::UnknownSession
        );
        assert_eq!(
            registry.close("pty-nope").expect_err("unknown"),
            PtyError::UnknownSession
        );
    }

    #[test]
    fn write_size_is_bounded() {
        let registry = PtyRegistry::new();
        let report = registry.create(None, 80, 24, None).expect("pty");
        let big = "x".repeat(MAX_WRITE_BYTES + 1);
        assert_eq!(
            registry
                .write(&report.session_id, &big)
                .expect_err("too large"),
            PtyError::WriteTooLarge
        );
        let _ = registry.close(&report.session_id);
    }

    #[test]
    fn shell_roundtrip_echoes_output() {
        let registry = PtyRegistry::new();
        let report = registry.create(None, 80, 24, None).expect("cmd pty");
        assert_eq!(report.state, "running");
        assert!(report.session_id.starts_with("pty-"));
        registry
            .write(&report.session_id, "echo pty-roundtrip-ok\r\n")
            .expect("write");
        let mut saw = String::new();
        for _ in 0..50 {
            if let Some(event) = registry.recv_event_timeout(Duration::from_millis(200)) {
                saw.push_str(&event.data);
                if saw.contains("pty-roundtrip-ok") {
                    break;
                }
            }
        }
        assert!(saw.contains("pty-roundtrip-ok"), "output: {saw:?}");
        let resized = registry
            .resize(&report.session_id, 100, 40)
            .expect("resize");
        assert_eq!(resized.cols, 100);
        assert_eq!(resized.rows, 40);
        registry.close(&report.session_id).expect("close");
        assert_eq!(
            registry.report(&report.session_id).expect_err("closed"),
            PtyError::UnknownSession
        );
    }

    #[test]
    fn registry_drop_cleans_up_sessions() {
        let registry = PtyRegistry::new();
        let report = registry.create(None, 80, 24, None).expect("pty");
        drop(registry);
        // Drop terminated the child and released every handle; the report is
        // consumed so no unused-variable lint fires.
        let _ = report;
    }
}
