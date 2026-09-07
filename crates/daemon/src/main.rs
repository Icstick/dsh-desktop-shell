//! dsh-desktop-daemon entry (ADR-0019 decision 1: standalone, UI-less
//! process; the Shell is the only tauri process).
//!
//! Startup sequence:
//!
//! 1. resolve the data directory (`--data-dir` > `DSH_DAEMON_DATA_DIR` >
//!    `%APPDATA%\dev.dsh.desktop-shell`); `--claim-port` overrides the
//!    single-instance claim port (default 37771; M6-D test isolation);
//! 2. acquire the single-instance guard — claim port 37771 ownership +
//!    start lock file (ADR-0019 decision 4; a second daemon exits with
//!    code 3);
//! 3. bind the envelope server and issue a one-time credential;
//! 4. write the credential file (`daemon-credential.json`) the Shell
//!    reads at startup (ADR-0019 decision 5);
//! 5. serve connections until the process is stopped (Ctrl+C / taskkill;
//!    the claim port is the authoritative liveness probe).
//!
//! Exit codes: 0 clean, 1 runtime error, 2 usage, 3 already running
//! (claim port owned), 4 lock file conflict.

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use dsh_daemon::DAEMON_VERSION;
use dsh_daemon::credential::{CLAIM_PORT, CREDENTIAL_FILE_NAME, data_dir};
use dsh_daemon::server::DaemonServer;
use dsh_daemon::singleton::{EXIT_ALREADY_RUNNING, EXIT_LOCK_CONFLICT, InstanceGuard};
use dsh_local_transport::Limits;

fn usage() -> String {
    format!(
        "dsh-desktop-daemon {DAEMON_VERSION}\n\nUSAGE:\n    dsh-desktop-daemon [--data-dir <dir>] [--claim-port <port>] [--version]\n\nOPTIONS:\n    --data-dir <dir>      override the daemon data directory (default: %APPDATA%\\dev.dsh.desktop-shell)\n    --claim-port <port>   override the single-instance claim port (default: 37771; test isolation)\n    --version             print the daemon version and exit"
    )
}

fn main() -> ExitCode {
    // --- argument parsing (minimal, no external clap) ---
    let args: Vec<String> = env::args().skip(1).collect();
    let mut data_dir_override: Option<PathBuf> = None;
    let mut claim_port_override: Option<u16> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--data-dir" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("{}", usage());
                    return ExitCode::from(2);
                };
                data_dir_override = Some(PathBuf::from(value));
            }
            "--claim-port" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("{}", usage());
                    return ExitCode::from(2);
                };
                match value.parse::<u16>() {
                    Ok(port) => claim_port_override = Some(port),
                    Err(_) => {
                        eprintln!("invalid --claim-port value \"{value}\"");
                        return ExitCode::from(2);
                    }
                }
            }
            "--version" => {
                println!("dsh-desktop-daemon {DAEMON_VERSION}");
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!("{}", usage());
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument \"{other}\"\n{}", usage());
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let data_dir = data_dir_override.unwrap_or_else(data_dir);
    let claim_port = claim_port_override.unwrap_or(CLAIM_PORT);

    // --- 1) single-instance guard (claim port + lock file) ---
    let _guard = match InstanceGuard::acquire(&data_dir, claim_port) {
        Ok(guard) => guard,
        Err(dsh_daemon::singleton::InstanceGuardError::ClaimPort(error)) => {
            eprintln!(
                "dsh-desktop-daemon: another daemon instance is already running (claim port {claim_port} is in use): {error}"
            );
            return ExitCode::from(EXIT_ALREADY_RUNNING);
        }
        Err(dsh_daemon::singleton::InstanceGuardError::Lock(error)) => {
            eprintln!("dsh-desktop-daemon: cannot take the lock file: {error}");
            return ExitCode::from(EXIT_LOCK_CONFLICT);
        }
    };

    // --- 2) envelope server ---
    // The Shell keeps one long-lived connection (event bridge + repeated
    // invocations); the transport default 30s idle read deadline would drop
    // it whenever the user pauses longer than that, leaving every later
    // invoke failing as a closed transport. Raise the idle deadline to 24h:
    // the Shell restarts daily and the credential-lease maintenance keeps
    // the file token fresh while the daemon idles (M6 bootstrap fix).
    let mut limits = Limits::default();
    limits.read_deadline = std::time::Duration::from_secs(24 * 60 * 60);
    let server = match DaemonServer::bind(limits, claim_port) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("dsh-desktop-daemon: cannot bind the envelope server: {error}");
            return ExitCode::FAILURE;
        }
    };

    // --- 3) credential file for the Shell (freshness-maintained so a
    // Shell start always finds a usable token, even after an idle period
    // past the lease; BLOCK-M8E-BOOTSTRAP-STUCK fix) ---
    match server
        .issue_bootstrap_credential_file(Duration::from_secs(dsh_daemon::server::LEASE_MAX_SECONDS))
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            eprintln!("dsh-desktop-daemon: no data directory for the credential file");
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("dsh-desktop-daemon: cannot write the credential file: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!("dsh-desktop-daemon {DAEMON_VERSION} started");
    println!("  pid:          {}", std::process::id());
    println!("  claim port:   {claim_port} (presence probe / single instance)");
    println!("  envelope:     127.0.0.1:{}", server.addr().port());
    println!(
        "  credential:   {}",
        data_dir.join(CREDENTIAL_FILE_NAME).display()
    );
    println!("  data dir:     {}", data_dir.display());

    // --- 4) serve loop: spawn one thread per authenticated connection ---
    // (dedup by connection id; the transport removes closed connections).
    let server = std::sync::Arc::new(server);
    let mut served: HashSet<u64> = HashSet::new();
    let mut last_credential_maintain = std::time::Instant::now();
    loop {
        // Bootstrap credential freshness (BLOCK-M8E-BOOTSTRAP-STUCK fix):
        // keep the file token usable even when the daemon is idle past a
        // lease, so a Shell (re)start can always re-attach.
        if last_credential_maintain.elapsed() >= Duration::from_secs(1) {
            server.maintain_bootstrap_credential();
            last_credential_maintain = std::time::Instant::now();
        }
        for conn in server.connections() {
            if served.insert(conn.id()) {
                let server = std::sync::Arc::clone(&server);
                thread::spawn(move || server.serve_connection(conn));
            }
        }
        thread::sleep(Duration::from_millis(5));
        // The daemon runs until the process is stopped (Ctrl+C / taskkill).
        // Dropping `guard` (lock cleanup) happens on process teardown paths
        // that return here; on hard kill the stale lock is recovered by the
        // next start via the authoritative port check.
    }
}
