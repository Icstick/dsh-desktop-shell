//! Split-brain contention tests (ADR-0019 decision 4, M6-D).
//!
//! Real-process tests: spawn the compiled dsh-desktop-daemon binary and
//! exercise the single-instance contract end to end:
//!
//! - instance A starts (claim port + lock file) -> instance B exits with
//!   code 3 (already running) and a diagnostic on stderr;
//! - after A is killed (simulated crash: no cleanup), B can start and
//!   takes over (stale-lock recovery through the authoritative port
//!   check);
//! - a stale lock file from a crashed daemon (port free) is recovered on
//!   the next start, which also rewrites the pid and the credential file;
//! - a claim port held by an unrelated process is a clear failure: exit
//!   code 3 + message (the daemon cannot distinguish an external listener
//!   from a daemon it does not share a data dir with; the lock file is the
//!   diagnostic that disambiguates the common case);
//! - a lock file that cannot be recovered (here: a directory) exits with
//!   code 4 (lock conflict).
//!
//! Hermeticity: every test uses its own claim port (--claim-port, added
//! in M6-D for exactly this) and its own data dir, so the tests are
//! parallel-safe and never touch the production 37771 port. The binary
//! path comes from CARGO_BIN_EXE_dsh_desktop_daemon (Cargo sets it for
//! integration tests of the package owning the bin target). If the binary
//! is not available (e.g. the bin was not built), the tests skip by
//! returning early — the pure single-instance logic is still covered by
//! the unit tests in src/singleton.rs.

use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use dsh_daemon::credential::{CREDENTIAL_FILE_NAME, LOCK_FILE_NAME};
use dsh_daemon::singleton::{EXIT_ALREADY_RUNNING, EXIT_LOCK_CONFLICT};

/// Path of the compiled daemon binary (None when not built; tests skip).
const DAEMON_EXE: Option<&str> = option_env!("CARGO_BIN_EXE_dsh_desktop_daemon");

/// Unique temp dir per test (tests run in parallel threads).
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn daemon_exe() -> Option<PathBuf> {
    let exe = DAEMON_EXE.map(PathBuf::from)?;
    exe.exists().then_some(exe)
}

fn temp_dir(tag: &str) -> PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "dsh-daemon-splitbrain-{tag}-{}-{seq}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Grab a free loopback port for one test (the daemon takes it over via
/// --claim-port; the tiny probe->spawn window is the standard TOCTOU of
/// ephemeral-port tests and is negligible here).
fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// A spawned daemon process. Drop kills and reaps it so a panicking test
/// never leaks a live daemon holding its claim port.
struct DaemonProc {
    child: Child,
    stderr: Option<std::process::ChildStderr>,
}

impl DaemonProc {
    fn spawn(exe: &Path, data_dir: &Path, claim_port: u16) -> Self {
        let mut child = Command::new(exe)
            .arg("--data-dir")
            .arg(data_dir)
            .arg("--claim-port")
            .arg(claim_port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn daemon");
        let stderr = child.stderr.take().expect("stderr piped");
        Self {
            child,
            stderr: Some(stderr),
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Drain the captured stderr (call after the process has exited).
    fn stderr_text(&mut self) -> String {
        let mut text = String::new();
        if let Some(mut err) = self.stderr.take() {
            let _ = err.read_to_string(&mut text);
        }
        text
    }

    /// Wait for exit, or None on timeout.
    fn wait_until_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Hard-kill (TerminateProcess on Windows): simulates a crash — no
    /// lock cleanup, exactly the stale-lock scenario.
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for DaemonProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// True once something listens on the claim port (the guard binds it
/// before the daemon goes any further).
fn probe_ready(claim_port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", claim_port)).is_ok()
}

fn wait_ready(claim_port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if probe_ready(claim_port) {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    probe_ready(claim_port)
}

fn wait_not_ready(claim_port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !probe_ready(claim_port) {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    !probe_ready(claim_port)
}

/// A + B: the second instance is rejected with exit code 3 while the
/// first keeps running; the rejection carries a diagnostic.
#[test]
fn second_instance_is_rejected_while_first_runs() {
    let Some(exe) = daemon_exe() else {
        eprintln!("daemon binary not built; skipping split-brain process test");
        return;
    };
    let dir = temp_dir("second-rejected");
    let port = free_port();

    let mut first = DaemonProc::spawn(&exe, &dir, port);
    assert!(
        wait_ready(port, Duration::from_secs(15)),
        "instance A must claim the port"
    );
    assert!(
        dir.join(LOCK_FILE_NAME).exists(),
        "instance A holds the lock file"
    );

    let mut second = DaemonProc::spawn(&exe, &dir, port);
    let status = second
        .wait_until_exit(Duration::from_secs(15))
        .expect("instance B must exit on its own");
    assert_eq!(
        status.code(),
        Some(i32::from(EXIT_ALREADY_RUNNING)),
        "instance B rejected with the already-running exit code"
    );
    let stderr = second.stderr_text();
    assert!(
        stderr.contains("already running") || stderr.contains("claim port"),
        "rejection must carry a diagnostic, got: {stderr}"
    );

    // A is untouched by the failed takeover.
    assert!(probe_ready(port), "instance A still owns the claim port");
    assert!(
        dir.join(LOCK_FILE_NAME).exists(),
        "instance A still holds the lock file"
    );

    first.kill();
    let _ = fs::remove_dir_all(&dir);
}

/// Kill A (simulated crash: the lock file stays behind) -> B can start and
/// takes over via the stale-lock recovery path; B rewrites the lock pid
/// and writes the credential file.
#[test]
fn instance_starts_after_first_is_killed_and_recovers_stale_lock() {
    let Some(exe) = daemon_exe() else {
        eprintln!("daemon binary not built; skipping split-brain process test");
        return;
    };
    let dir = temp_dir("crash-recover");
    let port = free_port();

    let mut first = DaemonProc::spawn(&exe, &dir, port);
    assert!(wait_ready(port, Duration::from_secs(15)), "A must start");
    first.kill();

    assert!(
        wait_not_ready(port, Duration::from_secs(5)),
        "port must be released after A is killed"
    );
    assert!(
        dir.join(LOCK_FILE_NAME).exists(),
        "crash leaves the lock file behind (stale)"
    );

    // B takes over: the port is free, so the stale lock is recovered.
    let mut second = DaemonProc::spawn(&exe, &dir, port);
    assert!(
        wait_ready(port, Duration::from_secs(15)),
        "B must start after A's crash"
    );
    let lock = fs::read_to_string(dir.join(LOCK_FILE_NAME)).expect("lock content");
    assert_eq!(
        lock,
        second.pid().to_string(),
        "B rewrote the lock file with its own pid"
    );
    assert!(
        dir.join(CREDENTIAL_FILE_NAME).exists(),
        "B wrote the credential file on startup"
    );

    second.kill();
    assert!(
        wait_not_ready(port, Duration::from_secs(5)),
        "port released after B is killed"
    );
    assert!(
        !dir.join(LOCK_FILE_NAME).exists(),
        "clean shutdown removes the lock file"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A stale lock file with no live owner (port free) is recovered on the
/// next start — the port check is authoritative over the lock file.
#[test]
fn stale_lock_without_owner_is_recovered() {
    let Some(exe) = daemon_exe() else {
        eprintln!("daemon binary not built; skipping split-brain process test");
        return;
    };
    let dir = temp_dir("stale-lock");
    let port = free_port();

    // A crashed daemon left only the lock file behind (no listener).
    fs::write(dir.join(LOCK_FILE_NAME), "999999").expect("stale lock");

    let mut daemon = DaemonProc::spawn(&exe, &dir, port);
    assert!(
        wait_ready(port, Duration::from_secs(15)),
        "daemon must start despite the stale lock"
    );
    let lock = fs::read_to_string(dir.join(LOCK_FILE_NAME)).expect("lock content");
    assert_eq!(lock, daemon.pid().to_string(), "stale lock replaced");

    daemon.kill();
    let _ = fs::remove_dir_all(&dir);
}

/// A claim port held by an unrelated process is a clear failure: the
/// daemon cannot bind it, exits with code 3 and explains on stderr.
#[test]
fn external_process_holding_claim_port_is_clear_failure() {
    let Some(exe) = daemon_exe() else {
        eprintln!("daemon binary not built; skipping split-brain process test");
        return;
    };
    let dir = temp_dir("external-port");
    let port = free_port();

    let _external = TcpListener::bind(("127.0.0.1", port)).expect("external listener");
    let mut daemon = DaemonProc::spawn(&exe, &dir, port);
    let status = daemon
        .wait_until_exit(Duration::from_secs(15))
        .expect("daemon must exit: claim port is owned");
    assert_eq!(
        status.code(),
        Some(i32::from(EXIT_ALREADY_RUNNING)),
        "port conflict maps to the already-running exit code"
    );
    let stderr = daemon.stderr_text();
    assert!(
        stderr.contains("claim port") && stderr.contains("already running"),
        "stderr must explain the port conflict, got: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A lock file that cannot be taken (here: a directory with the lock
/// name) exits with the dedicated lock-conflict code 4.
#[test]
fn unrecoverable_lock_conflict_exits_code_4() {
    let Some(exe) = daemon_exe() else {
        eprintln!("daemon binary not built; skipping split-brain process test");
        return;
    };
    let dir = temp_dir("lock-conflict");
    let port = free_port();

    fs::create_dir_all(dir.join(LOCK_FILE_NAME)).expect("lock as directory");
    let mut daemon = DaemonProc::spawn(&exe, &dir, port);
    let status = daemon
        .wait_until_exit(Duration::from_secs(15))
        .expect("daemon must exit: lock cannot be taken");
    assert_eq!(
        status.code(),
        Some(i32::from(EXIT_LOCK_CONFLICT)),
        "unrecoverable lock conflict maps to exit code 4"
    );
    let stderr = daemon.stderr_text();
    assert!(
        stderr.contains("lock file"),
        "stderr must explain the lock conflict, got: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}
