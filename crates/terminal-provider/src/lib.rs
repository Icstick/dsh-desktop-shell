//! Desktop-owned PTY sessions on Windows ConPTY (MOD-TERMINAL-PROVIDER, ADR-0015).
//!
//! The provider owns the PTY lifecycle: it creates a pseudo console, spawns
//! the user's shell as a child of THIS process (never of the Managed DSH
//! process tree), relays output as events, and cleans up on close or Drop.
//! Session ids are opaque; callers never see PIDs or paths (AC-TERM-002).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
    InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute,
};

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
        let handle = guard.write_handle.ok_or(PtyError::Io)?.0;
        let bytes = data.as_bytes();
        let mut written = 0u32;
        let result = unsafe {
            WriteFile(
                handle,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            return Err(PtyError::Io);
        }
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
        let size = COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        unsafe {
            let _ = ResizePseudoConsole(guard.pseudo_console.0 as isize, size);
        }
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
/// Send+Sync wrapper for Win32 kernel handles. A handle value is safe to
/// move between threads as long as a single owner closes it exactly once;
/// Session guarantees that invariant (the reader thread only carries the
/// raw value as usize for the session lifetime).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawHandle(*mut core::ffi::c_void);

unsafe impl Send for RawHandle {}
unsafe impl Sync for RawHandle {}

impl RawHandle {
    fn is_null(self) -> bool {
        self.0.is_null()
    }
}

#[derive(Debug)]
struct Session {
    id: String,
    pseudo_console: RawHandle,
    process: RawHandle,
    cols: u16,
    rows: u16,
    created_at_unix_ms: u64,
    last_activity_unix_ms: Option<u64>,
    closed: bool,
    error: Option<String>,
    read_handle: Option<RawHandle>,
    write_handle: Option<RawHandle>,
    reader: Option<JoinHandle<()>>,
    /// Set by terminate() so the polling reader loop exits promptly; the
    /// reader never blocks indefinitely on ReadFile (see spawn_output_reader).
    reader_stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.terminate();
    }
}

impl Session {
    fn terminate(&mut self) {
        // Teardown order (M6-C1 deadlock finding): the reader polls with
        // PeekNamedPipe and never blocks indefinitely on ReadFile, so
        // stopping it is just a flag. Sequence: signal stop -> close the
        // child's stdin write end -> kill the child (bounded backstop) ->
        // join the reader (exits within one poll tick) -> close the read
        // handle (no pending I/O, CloseHandle cannot block) -> close the
        // pseudo console.
        self.reader_stop
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(handle) = self.write_handle.take() {
            unsafe {
                let _ = CloseHandle(handle.0);
            }
        }
        if !self.closed {
            self.closed = true;
            unsafe {
                if !self.process.is_null() {
                    let _ =
                        windows_sys::Win32::System::Threading::TerminateProcess(self.process.0, 1);
                    let _ = CloseHandle(self.process.0);
                }
                ClosePseudoConsole(self.pseudo_console.0 as isize);
            }
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(handle) = self.read_handle.take() {
            unsafe {
                let _ = CloseHandle(handle.0);
            }
        }
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
        let shell_path = match shell.unwrap_or("default") {
            "default" | "cmd" => "%COMSPEC%".to_string(),
            "powershell" => "powershell.exe".to_string(),
            _ => return Err(PtyError::InvalidShell),
        };
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
        let session = spawn_conpty(&id, &shell_path, cols, rows, now, cwd, event_tx)?;
        let session = Arc::new(Mutex::new(session));
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

/// Size in bytes of a thread attribute list with one attribute.
fn attribute_size() -> usize {
    let mut size = 0usize;
    unsafe {
        // First call with a null list only returns the required size.
        let _ = InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
    }
    size
}

/// Zeroed allocation large enough for the attribute list.
fn allocate_zeroed(size_bytes: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size_bytes.max(1), 8).expect("layout");
    unsafe { std::alloc::alloc_zeroed(layout) }
}

/// Read the ConPTY output pipe and forward chunks as events.
///
/// The bridge drains the bounded event channel; when it is gone, reading
/// stops and the thread exits (the session is being torn down anyway).
fn spawn_output_reader(
    session_id: String,
    handle: usize,
    event_tx: std::sync::mpsc::SyncSender<OutputEvent>,
    started_at_unix_ms: u64,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> JoinHandle<()> {
    // HANDLE is a raw pointer; carrying it as usize keeps the closure
    // Send (the handle stays valid for the session lifetime).
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut seq = 0u64;
        loop {
            if stop.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            // Poll instead of blocking on ReadFile: a synchronous ReadFile
            // with no data in flight cannot be cancelled, and CloseHandle on
            // a handle with pending I/O blocks indefinitely (M6-C1
            // deadlock). PeekNamedPipe tells us whether a ReadFile would
            // return immediately.
            let mut available = 0u32;
            let peek_ok = unsafe {
                windows_sys::Win32::System::Pipes::PeekNamedPipe(
                    handle as *mut core::ffi::c_void,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if peek_ok == 0 {
                // Pipe error / broken: the session is being torn down.
                break;
            }
            if available == 0 {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            let mut read = 0u32;
            let result = unsafe {
                ReadFile(
                    handle as *mut core::ffi::c_void,
                    buffer.as_mut_ptr(),
                    buffer.len() as u32,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            if result == 0 || read == 0 {
                break;
            }
            seq += 1;
            if event_tx
                .try_send(OutputEvent {
                    session_id: session_id.clone(),
                    seq,
                    data: String::from_utf8_lossy(&buffer[..read as usize]).into_owned(),
                })
                .is_err()
            {
                // Queue full or bridge gone: drop the chunk rather than stall
                // the reader (bounded event queue, AC-IPC-002 pattern).
                break;
            }
            let _ = started_at_unix_ms;
        }
    })
}

/// Spawn a shell under a fresh ConPTY and return the owned session.
///
/// # Errors
///
/// Returns `PtyError::SpawnUnavailable` when pipe/console/process creation
/// fails; all acquired handles are released on the error path.
fn spawn_conpty(
    id: &str,
    shell_path: &str,
    cols: u16,
    rows: u16,
    created_at_unix_ms: u64,
    cwd: Option<&str>,
    event_tx: std::sync::mpsc::SyncSender<OutputEvent>,
) -> Result<Session, PtyError> {
    unsafe {
        let mut input_read: HANDLE = std::ptr::null_mut();
        let mut input_write: HANDLE = std::ptr::null_mut();
        let mut output_read: HANDLE = std::ptr::null_mut();
        let mut output_write: HANDLE = std::ptr::null_mut();
        if CreatePipe(&mut input_read, &mut input_write, std::ptr::null(), 0) == 0 {
            return Err(PtyError::SpawnUnavailable);
        }
        if CreatePipe(&mut output_read, &mut output_write, std::ptr::null(), 0) == 0 {
            let _ = CloseHandle(input_read);
            let _ = CloseHandle(input_write);
            return Err(PtyError::SpawnUnavailable);
        }
        let mut pseudo_console: HPCON = 0;
        let size = COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        let created = CreatePseudoConsole(size, input_read, output_write, 0, &mut pseudo_console);
        if created != 0 {
            let _ = CloseHandle(input_read);
            let _ = CloseHandle(input_write);
            let _ = CloseHandle(output_read);
            let _ = CloseHandle(output_write);
            return Err(PtyError::SpawnUnavailable);
        }

        // Extended startup info with the pseudoconsole attribute list.
        let mut startup_info: STARTUPINFOEXW = std::mem::zeroed();
        startup_info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup_info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;

        let mut size_bytes = attribute_size();
        let attribute_list = allocate_zeroed(size_bytes);
        let init =
            InitializeProcThreadAttributeList(attribute_list as *mut _, 1, 0, &mut size_bytes);
        if init == 0 {
            ClosePseudoConsole(pseudo_console);
            let _ = CloseHandle(output_read);
            let _ = CloseHandle(input_write);
            return Err(PtyError::SpawnUnavailable);
        }
        let updated = UpdateProcThreadAttribute(
            attribute_list as *mut _,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            pseudo_console as *const _,
            std::mem::size_of::<HPCON>(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if updated == 0 {
            DeleteProcThreadAttributeList(attribute_list as *mut _);
            ClosePseudoConsole(pseudo_console);
            let _ = CloseHandle(output_read);
            let _ = CloseHandle(input_write);
            return Err(PtyError::SpawnUnavailable);
        }
        startup_info.lpAttributeList = attribute_list as *mut _;

        // Resolve the shell command line (cmd via %COMSPEC%, or powershell).
        let command_line = if shell_path == "%COMSPEC%" {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            shell_path.to_string()
        };
        let mut command_line_utf16: Vec<u16> = command_line.encode_utf16().collect();
        command_line_utf16.push(0);

        let mut cwd_utf16: Option<Vec<u16>> = cwd.map(|value| {
            let mut encoded: Vec<u16> = value.encode_utf16().collect();
            encoded.push(0);
            encoded
        });

        let mut process_info: PROCESS_INFORMATION = std::mem::zeroed();
        let created_process = CreateProcessW(
            std::ptr::null(),
            command_line_utf16.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT,
            std::ptr::null(),
            cwd_utf16
                .as_mut()
                .map(|cwd| cwd.as_mut_ptr())
                .unwrap_or(std::ptr::null_mut()),
            &startup_info.StartupInfo,
            &mut process_info,
        );
        DeleteProcThreadAttributeList(attribute_list as *mut _);
        if created_process == 0 {
            ClosePseudoConsole(pseudo_console);
            let _ = CloseHandle(output_read);
            let _ = CloseHandle(input_write);
            return Err(PtyError::SpawnUnavailable);
        }

        let reader_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = spawn_output_reader(
            id.to_string(),
            output_read as usize,
            event_tx,
            unix_ms(),
            reader_stop.clone(),
        );

        Ok(Session {
            id: id.to_string(),
            pseudo_console: RawHandle(pseudo_console as *mut core::ffi::c_void),
            process: RawHandle(process_info.hProcess),
            cols,
            rows,
            created_at_unix_ms,
            last_activity_unix_ms: None,
            closed: false,
            error: None,
            read_handle: Some(RawHandle(output_read)),
            write_handle: Some(RawHandle(input_write)),
            reader: Some(reader),
            reader_stop,
        })
    }
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
            registry
                .create(Some("bash"), 120, 30, None)
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
