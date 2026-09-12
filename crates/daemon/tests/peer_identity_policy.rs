//! Control-plane peer-identity policy (ADR-0022): under strict policy a
//! Shell claim arriving on the identity-less TCP carrier must NOT earn
//! shell_control authority - the broker-relaxed human path stays closed.
//! The same flow under the legacy Off policy proves the policy is the only
//! variable. These are the negative tests ADR-0022 asks for, minus the
//! named-pipe impostor case (that one needs a real Shell image, slice 7).

mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use common::TestClient;
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

fn daemon() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: DAEMON_API_VERSION.into(),
        kind: DAEMON_KIND.into(),
    }
}

fn spawn_with_policy(policy: PeerIdentityPolicy) -> (std::net::SocketAddr, Arc<DaemonServer>) {
    let server = Arc::new(
        DaemonServer::bind_with_policy(Limits::default(), 0, temp_catalog(), policy)
            .expect("bind daemon server"),
    );
    let addr = server.addr();
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
    (addr, server)
}

fn temp_catalog() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dsh-peer-policy-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("environments.json")
}

/// Strict policy + identity-less TCP: a connection claiming the Shell
/// identity cannot take over grants from the incumbent participant - the
/// broker-relaxed path requires a kernel peer identity, so the claim is
/// degraded to Participant and the negotiation stays fail-closed.
#[test]
fn strict_policy_denies_shell_claim_on_identityless_tcp() {
    let (addr, server) = spawn_with_policy(PeerIdentityPolicy::strict(
        "C:\\definitely\\not\\the-shell.exe",
    ));

    // Incumbent: a plain participant holds the browser grant.
    let incumbent_credential = server.issue_credential(Duration::from_secs(300));
    let mut incumbent = TestClient::connect_as(addr, &incumbent_credential, "other", "tool");
    incumbent.negotiate(vec![browser(), daemon()]);

    // Shell claim over TCP: no kernel identity -> not ShellControl -> the
    // conflict is resolved fail-closed (no relaxed path).
    let shell_credential = server.issue_credential(Duration::from_secs(300));
    let mut shell_claim =
        TestClient::connect_as(addr, &shell_credential, "dsh-desktop-shell", "shell");
    let agreement = shell_claim.negotiate(vec![browser()]);
    assert!(
        agreement.granted.is_empty(),
        "strict policy must not relax the grant for an identity-less Shell claim"
    );

    // And a mutation is refused at the gate.
    let error = shell_claim
        .invoke(
            browser(),
            BROWSER_CREATE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "mode": "human_surface" }),
        )
        .expect_err("no grant, no dispatch");
    assert_eq!(error.code, ErrorCode::Unauthorized);
}

/// Off policy (legacy/test posture): the same flow keeps the relaxed path,
/// proving the identity policy is the only difference.
#[test]
fn off_policy_keeps_the_legacy_relaxed_path() {
    let (addr, server) = spawn_with_policy(PeerIdentityPolicy::Off);

    let incumbent_credential = server.issue_credential(Duration::from_secs(300));
    let mut incumbent = TestClient::connect_as(addr, &incumbent_credential, "other", "tool");
    incumbent.negotiate(vec![browser(), daemon()]);

    let shell_credential = server.issue_credential(Duration::from_secs(300));
    let mut shell_claim =
        TestClient::connect_as(addr, &shell_credential, "dsh-desktop-shell", "shell");
    let agreement = shell_claim.negotiate(vec![browser()]);
    assert!(
        !agreement.granted.is_empty(),
        "legacy posture keeps the Shell claim relaxed"
    );
    // The relaxed claim can dispatch (the daemon authorizes by connection).
    let created = shell_claim.invoke(
        browser(),
        BROWSER_CREATE_METHOD,
        serde_json::json!({ "schemaVersion": 1, "mode": "human_surface" }),
    );
    assert!(
        created.is_ok(),
        "relaxed Shell dispatch succeeds: {created:?}"
    );
}
