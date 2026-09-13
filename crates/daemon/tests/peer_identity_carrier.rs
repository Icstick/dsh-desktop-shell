//! ADR-0022 slice 6/7: the same-user impostor test over the peer-identity
//! carrier, on every platform that has one (Windows named pipe, Unix domain
//! socket). The threat model of audit H-2 is a same-user process that reads
//! the one-time credential file; it can still connect and present a valid
//! token, but over the carrier the daemon additionally learns *which binary*
//! connected. Under the strict policy that binary must be the expected Shell
//! image - everything else is degraded to Participant and never reaches the
//! broker-relaxed control plane.
//!
//! The same two legs run on all three CI platforms; only the carrier
//! underneath differs (see `common::connect_carrier`).
#![cfg(any(windows, unix))]

mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use common::{connect_carrier, connect_carrier_as};
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_CREATE_METHOD, BROWSER_KIND, DAEMON_API_VERSION, DAEMON_KIND,
};
use dsh_daemon::envelope::{ErrorCode, ProtocolCoordinate};
use dsh_daemon::server::{CarrierEndpoint, DaemonServer, PeerIdentityPolicy};
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

fn spawn_with_policy(policy: PeerIdentityPolicy) -> (Arc<DaemonServer>, CarrierEndpoint) {
    let server = Arc::new(
        DaemonServer::bind_with_policy(Limits::default(), 0, temp_catalog(), policy)
            .expect("bind daemon server"),
    );
    let endpoint = server
        .carrier_endpoint()
        .expect("the daemon attaches the peer-identity carrier for this platform")
        .clone();
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
    (server, endpoint)
}

/// A catalog path per process: the carrier endpoint is derived from its
/// parent directory, so each test process gets its own socket.
fn temp_catalog() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dsh-peer-carrier-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("environments.json")
}

/// This test binary's own image path, in the same form the kernel reports
/// for the peer. `current_exe()` already is that form on all three
/// targets (Linux `/proc/self/exe`, macOS `_NSGetExecutablePath` like
/// `proc_pidpath`, Windows `GetModuleFileNameW` like
/// `QueryFullProcessImageNameW`).
///
/// Canonicalizing here breaks Windows: `canonicalize` answers with the
/// verbatim `\\?\`-prefixed form no kernel API returns - exactly the
/// mismatch this test caught on 2026-09-13, when the positive leg went red
/// while the negative leg stayed green (i.e. the strict policy would have
/// refused every real Shell while still "passing" the impostor test).
fn own_image_path() -> String {
    std::env::current_exe()
        .expect("test executable path")
        .to_string_lossy()
        .into_owned()
}

/// A path no test process can be running from.
fn foreign_image_path() -> &'static str {
    if cfg!(windows) {
        "C:\\Program Files\\DSH Desktop Shell\\dsh-desktop-shell.exe"
    } else {
        "/opt/dsh-desktop-shell/dsh-desktop-shell"
    }
}

/// Positive leg: the connecting process IS the expected image (this test
/// binary), so the Shell claim over the carrier earns ShellControl and the
/// broker-relaxed path stays available.
#[test]
fn strict_policy_admits_the_shell_when_the_carrier_image_matches() {
    let (server, endpoint) = spawn_with_policy(PeerIdentityPolicy::strict(own_image_path()));

    // Incumbent participant holds the grants; the Shell then conflicts.
    let incumbent_credential = server.issue_credential(Duration::from_secs(300));
    let mut incumbent = connect_carrier_as(&endpoint, &incumbent_credential, "other", "tool");
    incumbent.negotiate(vec![browser(), daemon_coordinate()]);

    let shell_credential = server.issue_credential(Duration::from_secs(300));
    let mut shell = connect_carrier(&endpoint, &shell_credential);
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
#[test]
fn strict_policy_refuses_a_same_user_impostor_with_a_different_image() {
    // Expected image: somewhere the impostor (this test binary) is not.
    let (server, endpoint) = spawn_with_policy(PeerIdentityPolicy::strict(foreign_image_path()));

    let incumbent_credential = server.issue_credential(Duration::from_secs(300));
    let mut incumbent = connect_carrier_as(&endpoint, &incumbent_credential, "other", "tool");
    incumbent.negotiate(vec![browser(), daemon_coordinate()]);

    // The impostor: same user, valid one-time credential, claims the Shell.
    let impostor_credential = server.issue_credential(Duration::from_secs(300));
    let mut impostor = connect_carrier(&endpoint, &impostor_credential);
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
