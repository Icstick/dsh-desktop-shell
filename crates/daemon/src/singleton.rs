//! Single-instance guard (ADR-0019 decision 4, M6-B1 minimal form).
//!
//! Two independent checks, both fail-closed:
//!
//! 1. **Claim port ownership** — bind the fixed loopback port
//!    [`CLAIM_PORT`](crate::credential::CLAIM_PORT) (37771) and hold it
//!    for the daemon lifetime. A second daemon (or any conflicting
//!    process) fails the bind with `AddrInUse` → the daemon exits with
//!    the "already running" code. The claim listener also accepts and
//!    drops connections, so the Shell can probe daemon presence with a
//!    plain TCP connect to 127.0.0.1:37771.
//! 2. **Start lock file** — create `daemon.lock` (pid payload) with
//!    `create_new`; an existing file whose port is *free* is a stale
//!    lock from a crashed daemon (the port check is authoritative), so
//!    it is removed and re-taken. Any other failure aborts startup.
//!
//! The named-mutex variant and split-brain tests are M6-D (ADR-0019
//! decision 4 risk: Windows mutex semantics + port race details).

use std::fs::{self, OpenOptions};
use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::credential::LOCK_FILE_NAME;

/// Exit code when another daemon (or a conflicting process) owns the
/// claim port.
pub const EXIT_ALREADY_RUNNING: u8 = 3;

/// Exit code when the lock file cannot be taken even though the port is
/// free (unexpected file-system state).
pub const EXIT_LOCK_CONFLICT: u8 = 4;

/// The single-instance guard: owns the claim port listener and the lock
/// file for the daemon lifetime. Dropping it releases both (the accept
/// thread is joined first so the port is actually free again; the lock
/// file removal is best-effort).
pub struct InstanceGuard {
    _claim_listener: TcpListener,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<thread::JoinHandle<()>>,
    lock_path: PathBuf,
    _lock_file: fs::File,
}

impl InstanceGuard {
    /// Acquire the guard: bind the claim port first (authoritative), then
    /// take the lock file (stale-tolerant).
    pub fn acquire(data_dir: &Path, claim_port: u16) -> Result<Self, InstanceGuardError> {
        // 1) Port ownership: authoritative single-instance check.
        let listener =
            TcpListener::bind(("127.0.0.1", claim_port)).map_err(InstanceGuardError::ClaimPort)?;

        // Accept-and-drop probe connections so the Shell can test daemon
        // presence without a protocol exchange. Non-blocking accept + a
        // shutdown flag so `Drop` can join the thread and release the port.
        let shutdown = Arc::new(AtomicBool::new(false));
        let accept_thread = {
            let listener = listener
                .try_clone()
                .map_err(InstanceGuardError::ClaimPort)?;
            let shutdown = Arc::clone(&shutdown);
            thread::spawn(move || {
                let _ = listener.set_nonblocking(true);
                loop {
                    if shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    match listener.accept() {
                        Ok((stream, _)) => drop(stream),
                        Err(_) => thread::sleep(Duration::from_millis(5)),
                    }
                }
            })
        };

        // 2) Lock file: fast-fail + stale recovery.
        fs::create_dir_all(data_dir).map_err(InstanceGuardError::Lock)?;
        let lock_path = data_dir.join(LOCK_FILE_NAME);
        let lock_file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                // The port is ours, so any existing lock is stale (the
                // previous daemon crashed without cleanup).
                fs::remove_file(&lock_path).map_err(InstanceGuardError::Lock)?;
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock_path)
                    .map_err(InstanceGuardError::Lock)?
            }
            Err(error) => return Err(InstanceGuardError::Lock(error)),
        };

        // Record the owning pid for diagnostics.
        let _ = fs::write(&lock_path, std::process::id().to_string());

        Ok(Self {
            _claim_listener: listener,
            shutdown,
            accept_thread: Some(accept_thread),
            lock_path,
            _lock_file: lock_file,
        })
    }

    /// Path of the held lock file (diagnostics).
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // Stop the accept thread first: its listener clone would keep the
        // claim port bound after this guard is gone.
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.accept_thread.take() {
            let _ = handle.join();
        }
        // Best-effort: a crashed daemon leaves a stale lock, which the
        // next start recovers via the port check.
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Single-instance acquisition failures.
#[derive(Debug)]
pub enum InstanceGuardError {
    /// Claim port already owned by another daemon or process.
    ClaimPort(io::Error),
    /// Lock file could not be taken.
    Lock(io::Error),
}

impl std::fmt::Display for InstanceGuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClaimPort(error) => write!(
                f,
                "claim port already in use (another daemon running?): {error}"
            ),
            Self::Lock(error) => write!(f, "cannot take daemon lock file: {error}"),
        }
    }
}

impl std::error::Error for InstanceGuardError {}

/// Probe daemon presence by connecting to the claim port. `true` means a
/// process owns the port (a daemon, or an unrelated listener).
pub fn probe_present(claim_port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", claim_port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dsh-daemon-singleton-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Bind a free loopback port for the test (claim port must not
    /// collide with a real daemon on 37771).
    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .expect("bind")
            .local_addr()
            .expect("addr")
            .port()
    }

    #[test]
    fn second_acquisition_is_rejected() {
        let dir = temp_dir("double");
        let port = free_port();
        let first = InstanceGuard::acquire(&dir, port).expect("first acquire");
        let second = InstanceGuard::acquire(&dir, port);
        assert!(matches!(second, Err(InstanceGuardError::ClaimPort(_))));
        drop(first);
        // After release the guard can be re-acquired.
        let again = InstanceGuard::acquire(&dir, port).expect("re-acquire");
        drop(again);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_lock_is_recovered_when_port_is_free() {
        let dir = temp_dir("stale");
        let port = free_port();
        // A crashed daemon left a lock file behind.
        fs::write(dir.join(LOCK_FILE_NAME), "99999").expect("stale lock");
        let guard = InstanceGuard::acquire(&dir, port).expect("stale recovery");
        let content = fs::read_to_string(guard.lock_path()).expect("lock content");
        assert_eq!(content, std::process::id().to_string());
        drop(guard);
        assert!(!dir.join(LOCK_FILE_NAME).exists(), "lock removed on drop");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_detects_present_guard() {
        let dir = temp_dir("probe");
        let port = free_port();
        let guard = InstanceGuard::acquire(&dir, port).expect("acquire");
        assert!(probe_present(port));
        drop(guard);
        // After release the port is free again.
        assert!(!probe_present(port));
        let _ = fs::remove_dir_all(&dir);
    }
}
