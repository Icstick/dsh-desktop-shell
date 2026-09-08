//! Single-instance guard (ADR-0019 decision 4, M6-B1 minimal form;
//! 0.2.1 M6-C rework: the envelope server owns the authoritative port).
//!
//! Since 0.2.1 (M6-C fixed-port envelope) the single-instance authority
//! is the envelope bind itself: the daemon binds the fixed loopback port
//! CLAIM_PORT (37771) in DaemonServer::bind and a second daemon (or any
//! conflicting process) fails with AddrInUse - the daemon then exits with
//! the "already running" code. The Shell probes daemon presence with a
//! plain TCP connect to 127.0.0.1:37771 (the envelope listener answers
//! the connect; an unauthenticated probe is dropped after the handshake
//! deadline).
//!
//! This module keeps only the **start lock file** half of the original
//! guard: create daemon.lock (pid payload) with create_new; an existing
//! file whose port is *free* is a stale lock from a crashed daemon (the
//! port check is authoritative), so it is removed and re-taken. Any other
//! failure aborts startup. The named-mutex variant stays an M6-D open
//! item (ADR-0019 decision 4 risk).

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use crate::credential::LOCK_FILE_NAME;

/// Exit code when another daemon (or a conflicting process) owns the
/// envelope port (mapped from the bind failure in main.rs).
pub const EXIT_ALREADY_RUNNING: u8 = 3;

/// Exit code when the lock file cannot be taken even though the port is
/// free (unexpected file-system state).
pub const EXIT_LOCK_CONFLICT: u8 = 4;

/// The single-instance lock file guard: owns daemon.lock for the daemon
/// lifetime. Dropping it removes the file (best-effort); a crashed daemon
/// leaves a stale lock which the next start recovers (the port bind is
/// authoritative and happens before the lock).
pub struct InstanceGuard {
    lock_path: PathBuf,
    _lock_file: fs::File,
}

impl InstanceGuard {
    /// Acquire the start lock file. Stale-tolerant by design: the caller
    /// (main.rs) has ALREADY proven the envelope port is free (the bind is
    /// the authoritative single-instance check), so any existing lock file
    /// can only be a crashed daemon's residue and is taken over. A live
    /// second daemon never reaches this call - its envelope bind failed.
    pub fn acquire(data_dir: &Path) -> Result<Self, InstanceGuardError> {
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
        // Best-effort: a crashed daemon leaves a stale lock, which the
        // next start recovers via the port bind.
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Single-instance acquisition failures.
#[derive(Debug)]
pub enum InstanceGuardError {
    /// Lock file could not be taken.
    Lock(io::Error),
}

impl std::fmt::Display for InstanceGuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lock(error) => write!(f, "cannot take daemon lock file: {error}"),
        }
    }
}

impl std::error::Error for InstanceGuardError {}

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

    #[test]
    fn second_acquisition_takes_over_stale_lock() {
        // The lock file is stale-tolerant: a live second daemon is
        // rejected at the envelope bind (split_brain integration test),
        // never here - an existing lock at this point is residue of a
        // crashed daemon and is taken over (pid rewritten).
        let dir = temp_dir("double");
        let first = InstanceGuard::acquire(&dir).expect("first acquire");
        let second = InstanceGuard::acquire(&dir).expect("takeover is stale-tolerant");
        let content = fs::read_to_string(second.lock_path()).expect("lock content");
        assert_eq!(content, std::process::id().to_string());
        drop(second);
        drop(first);
        assert!(!dir.join(LOCK_FILE_NAME).exists(), "lock removed on drop");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_lock_is_recovered() {
        let dir = temp_dir("stale");
        // A crashed daemon left a lock file behind.
        fs::write(dir.join(LOCK_FILE_NAME), "99999").expect("stale lock");
        let guard = InstanceGuard::acquire(&dir).expect("stale recovery");
        let content = fs::read_to_string(guard.lock_path()).expect("lock content");
        assert_eq!(content, std::process::id().to_string());
        drop(guard);
        assert!(!dir.join(LOCK_FILE_NAME).exists(), "lock removed on drop");
        let _ = fs::remove_dir_all(&dir);
    }
}
