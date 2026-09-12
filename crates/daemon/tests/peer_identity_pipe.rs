//! ADR-0022 slice 7: the same-user impostor test over the peer-identity
//! carrier. The threat model of audit H-2 is a same-user process that reads
//! the one-time credential file; it can still connect and present a valid
//! token, but over the named pipe the daemon additionally learns *which
//! binary* connected. Under strict policy that binary must be the expected
//! Shell image - everything else is degraded to Participant and never
//! reaches the broker-relaxed control plane.

mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use common::{connect_pipe, connect_pipe_as};
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_CREATE_METHOD, BROWSER_KIND, DAEMON_API_VERSION, DAEMON_KIND,
};
use dsh_daemon::envelope::{ErrorCode, ProtocolCoordinate};
use dsh_daemon::server::{DaemonServer, PeerIdentityPolicy};
use dsh_local_transport::Limits;

fn browser() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: BROWSER_API_VERSION.into(),
        kind: BROWSER_KIND.into(),
    }
}

fn daemon_coordinate() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: DAEMON_API_VERSION.into(),
        kind: DAEMON_KIND.into(),
    }
}

fn spawn_with_policy(policy: PeerIdentityPolicy) -> (Arc<DaemonServer>, String) {
    let server = Arc::new(
        DaemonServer::bind_with_policy(Limits::default(), 0, temp_catalog(), policy)
            .expect("bind daemon server"),
    );
    let pipe_name = server
        .pipe_name()
        .expect("a Windows daemon attaches the peer-identity carrier")
        .to_string();
    let serve_server = Arc::clone(&server);
    thread::spawn(move || {
        let mut served: HashSet<u64> = HashSet::new();
        loop {
            for conn in serve_server.connections() {
                if served.insert(conn.id()) {
                    let server = Arc::clone(&serve_server);
                    thread::spawn(move || server.serve_connection(conn));
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    (server, pipe_name)
}

fn temp_catalog() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dsh-peer-pipe-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("environments.json")
}

/// Positive leg: the connecting process IS the expected image (this test
/// binary), so the Shell claim over the pipe earns ShellControl and the
/// broker-relaxed path stays available.
#[cfg(windows)]
#[test]
fn strict_policy_admits_the_shell_when_the_pipe_image_matches() {
    let expected = std::env::current_exe()
        .expect("test executable path")
        .to_string_lossy()
        .into_owned();
    let (server, pipe_name) = spawn_with_policy(PeerIdentityPolicy::strict(expected));

    // Incumbent participant holds the grants; the Shell then conflicts.
    let incumbent_credential = server.issue_credential(Duration::from_secs(300));
    let mut incumbent = connect_pipe_as(&pipe_name, &incumbent_credential, "other", "tool");
    incumbent.negotiate(vec![browser(), daemon_coordinate()]);

    let shell_credential = server.issue_credential(Duration::from_secs(300));
    let mut shell = connect_pipe(&pipe_name, &shell_credential);
    let agreement = shell.negotiate(vec![browser()]);
    assert!(
        !agreement.granted.is_empty(),
        "the matching Shell image keeps the relaxed human path (granted: {:?})",
        agreement.granted
    );
    let created = shell.invoke(
        browser(),
        BROWSER_CREATE_METHOD,
        serde_json::json!({ "schemaVersion": 1, "mode": "human_surface" }),
    );
    assert!(
        created.is_ok(),
        "relaxed Shell dispatch succeeds: {created:?}"
    );
}

/// Negative leg - the actual H-2 threat: a valid credential in a process
/// whose image is NOT the expected Shell gets no control-plane authority,
/// even though it presents the right token and the right claimed identity.
#[cfg(windows)]
#[test]
fn strict_policy_refuses_a_same_user_impostor_with_a_different_image() {
    // Expected image: somewhere the impostor (this test binary) is not.
    let (server, pipe_name) = spawn_with_policy(PeerIdentityPolicy::strict(
        "C:\\Program Files\\DSH Desktop Shell\\dsh-desktop-shell.exe",
    ));

    let incumbent_credential = server.issue_credential(Duration::from_secs(300));
    let mut incumbent = connect_pipe_as(&pipe_name, &incumbent_credential, "other", "tool");
    incumbent.negotiate(vec![browser(), daemon_coordinate()]);

    // The impostor: same user, valid one-time credential, claims the Shell.
    let impostor_credential = server.issue_credential(Duration::from_secs(300));
    let mut impostor = connect_pipe(&pipe_name, &impostor_credential);
    let agreement = impostor.negotiate(vec![browser()]);
    assert!(
        agreement.granted.is_empty(),
        "a different image must never earn the relaxed human path (granted: {:?})",
        agreement.granted
    );
    let error = impostor
        .invoke(
            browser(),
            BROWSER_CREATE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "mode": "human_surface" }),
        )
        .expect_err("the impostor has no grant");
    assert_eq!(error.code, ErrorCode::Unauthorized);
}
