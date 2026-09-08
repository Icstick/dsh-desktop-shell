//! M6-C (0.2.1): a connection disconnect revokes every broker lease it
//! negotiated with `LeaseRevocationReason::Disconnect` - the lease TTL no
//! longer bounds a disconnected participant (fail-closed). A reconnect
//! negotiates a fresh activation at the next generation (ADR-0018
//! decision 1); the disconnected activation stays durably revoked.

mod common;

use std::time::{Duration, Instant};

use common::{AgreementInfo, TestClient};
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_KIND, DAEMON_API_VERSION, DAEMON_KIND, RUNTIME_API_VERSION,
    RUNTIME_KIND, SYSTEM_API_VERSION, SYSTEM_KIND, TERMINAL_API_VERSION, TERMINAL_KIND,
};
use dsh_daemon::envelope::ProtocolCoordinate;
use dsh_supervisor::{CapabilityId, LeaseRevocationReason};

/// The broker indexes grants/leases by `CapabilityId`; the envelope
/// world speaks `ProtocolCoordinate` - same fields, different types
/// (the daemon maps between them at the surface).
fn capability(coordinate: &ProtocolCoordinate) -> CapabilityId {
    CapabilityId::new(&coordinate.api_version, &coordinate.kind)
}

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

/// All leases of a negotiated connection are revoked with the disconnect
/// reason once the client disconnects (M6-C).
#[test]
fn disconnect_revokes_negotiated_leases() {
    let (addr, credential, server) = common::spawn_daemon();
    let broker = server.broker();

    // Shell negotiates (daemon broker grants + leases per capability).
    let mut client = TestClient::connect(addr, &credential);
    let agreement: AgreementInfo = client.negotiate(all_catalog());
    assert_eq!(agreement.granted.len(), 5);
    let activation_id = agreement.activation_id.clone();
    let agent_id = "dsh-desktop-shell-test-client"; // {component}-{facet}

    // The broker state must exist before the disconnect (negotiation runs
    // on the serve thread).
    wait_until("broker holds 5 leases", || {
        broker.lock().expect("broker lock").lease_count() == 5
    });
    assert!(
        !broker
            .lock()
            .expect("broker lock")
            .agent_activation_revoked(agent_id, &activation_id),
        "activation is live before disconnect"
    );

    // Disconnect → serve_connection teardown revokes the leases.
    drop(client);
    wait_until("leases revoked with Disconnect", || {
        let broker = broker.lock().expect("broker lock");
        broker.agent_activation_revoked(agent_id, &activation_id)
            && broker
                .leases_for(&capability(&all_catalog()[0]))
                .iter()
                .all(|lease| {
                    lease
                        .revoked
                        .as_ref()
                        .is_some_and(|r| r.reason == LeaseRevocationReason::Disconnect)
                })
    });

    // Every lease carries the disconnect reason (not a TTL expiry wait).
    let broker = broker.lock().expect("broker lock");
    for coordinate in all_catalog() {
        for lease in broker.leases_for(&capability(&coordinate)) {
            assert_eq!(
                lease.revoked.as_ref().map(|r| r.reason),
                Some(LeaseRevocationReason::Disconnect),
                "lease {} must be revoked with Disconnect",
                lease.id
            );
        }
    }
    assert!(
        broker.agent_activation_revoked(agent_id, &activation_id),
        "activation stays durably revoked"
    );
    drop(broker);

    // A reconnect (fresh credential) negotiates a fresh activation at the
    // next generation: the disconnect did not ban the participant. (The
    // dispatch gate on the revoked activation facts is covered by the
    // supervisor unit tests - LeaseRevoked.)
    let fresh_credential = server.issue_credential(Duration::from_secs(300));
    let mut restarted = TestClient::connect(addr, &fresh_credential);
    let agreement2 = restarted.negotiate(all_catalog());
    assert_ne!(agreement2.activation_id, activation_id);
    assert_eq!(agreement2.granted.len(), 5);
    drop(restarted);
}
