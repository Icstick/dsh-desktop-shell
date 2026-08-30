//! HIGH regression tests (REVIEW-M6-DAEMON):
//!
//! - HIGH-1: a surviving daemon re-issues the bootstrap credential after a
//!   disconnect, so a Shell restart can re-attach (credential one-time is
//!   consumed by the first handshake; the file must carry a fresh token).
//! - HIGH-2: the broker-relaxed path (conflicting negotiation still
//!   granted at the protocol level) is Shell-only; any other participant
//!   conflicting with the single-owner grant stays fail-closed.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use common::TestClient;
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_KIND, DAEMON_API_VERSION, DAEMON_KIND, RUNTIME_API_VERSION,
    RUNTIME_KIND, SYSTEM_API_VERSION, SYSTEM_KIND, TERMINAL_API_VERSION, TERMINAL_KIND,
    TERMINAL_STATUS_METHOD,
};
use dsh_daemon::credential::{CLAIM_PORT, CredentialFile};
use dsh_daemon::envelope::{ErrorCode, ProtocolCoordinate, UnavailableReason};
use dsh_daemon::server::DaemonServer;
use dsh_local_transport::{AuthError, Credential, Limits, LocalClient, TransportError};

fn all_catalog() -> Vec<ProtocolCoordinate> {
    vec![
        ProtocolCoordinate {
            api_version: SYSTEM_API_VERSION.into(),
            kind: SYSTEM_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: DAEMON_API_VERSION.into(),
            kind: DAEMON_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: BROWSER_API_VERSION.into(),
            kind: BROWSER_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: TERMINAL_API_VERSION.into(),
            kind: TERMINAL_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: RUNTIME_API_VERSION.into(),
            kind: RUNTIME_KIND.into(),
        },
    ]
}

fn terminal() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: TERMINAL_API_VERSION.into(),
        kind: TERMINAL_KIND.into(),
    }
}

fn wait_until(label: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for: {label}");
}

/// HIGH-1: disconnect re-issues the bootstrap credential file, and the
/// fresh token authenticates while the consumed one is rejected as replay.
#[test]
fn disconnect_reissues_bootstrap_credential() {
    let dir = std::env::temp_dir().join(format!("dsh-reissue-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let catalog = dir.join("environments.json");

    let server = Arc::new(
        DaemonServer::bind_with_catalog(Limits::default(), CLAIM_PORT, catalog)
            .expect("bind daemon server"),
    );
    let addr = server.addr();
    let startup_credential = server.issue_credential(Duration::from_secs(300));
    // main.rs startup shape: write the bootstrap credential file.
    let startup_file = CredentialFile::new(
        "0.1.0",
        std::process::id(),
        CLAIM_PORT,
        addr.port(),
        startup_credential.token(),
        startup_credential.expires_at(),
        SystemTime::now(),
    );
    startup_file.write_to(&dir).expect("write credential file");
    let original_token = CredentialFile::read_from(&dir)
        .expect("read credential file")
        .credential
        .token;

    let serve_server = Arc::clone(&server);
    std::thread::spawn(move || {
        // Minimal serve loop (mirrors main.rs): serve each authenticated
        // connection on its own thread.
        let mut served = std::collections::HashSet::new();
        loop {
            for conn in serve_server.connections() {
                if served.insert(conn.id()) {
                    let server = Arc::clone(&serve_server);
                    std::thread::spawn(move || server.serve_connection(conn));
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });

    // First Shell: connect + negotiate (consumes the startup credential).
    let mut client = TestClient::connect(addr, &startup_credential);
    let agreement = client.negotiate(all_catalog());
    assert_eq!(agreement.granted.len(), 5);
    drop(client); // disconnect → serve_connection exits → re-issue

    // The file must carry a fresh token (daemon rewrote it).
    wait_until("credential file re-issued", || {
        CredentialFile::read_from(&dir)
            .map(|file| file.credential.token != original_token)
            .unwrap_or(false)
    });
    let fresh = CredentialFile::read_from(&dir).expect("re-read credential file");
    assert_ne!(
        fresh.credential.token, original_token,
        "token must be re-issued"
    );
    assert_eq!(fresh.claim_port, CLAIM_PORT);
    assert_eq!(fresh.port, addr.port());

    // The fresh token authenticates (a restarted Shell connects).
    let fresh_credential = Credential::new(
        fresh.credential.token.clone(),
        SystemTime::now() + Duration::from_secs(300),
    );
    let mut restarted = TestClient::connect(addr, &fresh_credential);
    let agreement = restarted.negotiate(all_catalog());
    assert_eq!(agreement.granted.len(), 5);

    // The consumed startup token is rejected as replay (AC-IPC-001).
    let err = LocalClient::connect(addr, &startup_credential, &Limits::default())
        .expect_err("consumed credential must be rejected");
    // The consumed token is gone either way: Replay while the registry
    // still holds the used entry, Invalid after the bounded sweep removed
    // it (both prove one-time consumption, AC-IPC-001).
    assert!(
        matches!(
            err,
            TransportError::Auth(AuthError::Replay | AuthError::Invalid)
        ),
        "expected replay/invalid rejection, got {err}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// HIGH-2: a conflicting non-Shell participant stays fail-closed (nothing
/// granted, invocations unauthorized); the Shell identity keeps the
/// broker-relaxed human path.
#[test]
fn non_shell_conflict_stays_fail_closed() {
    let (addr, credential, server) = common::spawn_daemon();

    // 1) The Shell negotiates first and owns all grants.
    let mut shell = TestClient::connect_as(addr, &credential, "dsh-desktop-shell", "shell");
    let agreement = shell.negotiate(all_catalog());
    assert_eq!(agreement.granted.len(), 5);
    assert!(agreement.unavailable.is_empty());

    // 2) A non-Shell participant (agent automation) conflicts with the
    //    single-owner grant → fail-closed: nothing granted, all denied.
    let agent_credential = server.issue_credential(Duration::from_secs(300));
    let mut agent = TestClient::connect_as(addr, &agent_credential, "evil-agent", "automation");
    let agreement = agent.negotiate(all_catalog());
    assert!(
        agreement.granted.is_empty(),
        "non-Shell conflict must grant nothing: {:?}",
        agreement.granted
    );
    assert_eq!(agreement.unavailable.len(), 5);
    assert!(
        agreement
            .unavailable
            .iter()
            .all(|u| u.reason == UnavailableReason::PolicyDenied)
    );

    // 3) Its invocations are unauthorized (grant check rejects).
    let error = agent
        .invoke(terminal(), TERMINAL_STATUS_METHOD, serde_json::json!({}))
        .expect_err("non-Shell conflict activation must not dispatch");
    assert_eq!(error.code, ErrorCode::Unauthorized);

    // 4) A second Shell identity (fresh credential, same component/facet)
    //    keeps the broker-relaxed human path: granted, invocations run.
    let shell_credential = server.issue_credential(Duration::from_secs(300));
    let mut shell2 = TestClient::connect_as(addr, &shell_credential, "dsh-desktop-shell", "shell");
    let agreement = shell2.negotiate(all_catalog());
    assert_eq!(
        agreement.granted.len(),
        5,
        "Shell identity keeps the broker-relaxed path"
    );
    let status = shell2
        .invoke(terminal(), TERMINAL_STATUS_METHOD, serde_json::json!({}))
        .expect("Shell human path dispatches");
    assert_eq!(status["count"], 0);
}
