//! Integration tests for the daemon envelope server (M6-B1).
//!
//! Every test drives the real wire: `LocalServer`/`LocalClient` with
//! one-time credentials and the broker-driven authorization chain
//! (broker_grant_from_negotiation + enforce_dispatch) — no mocks.

mod common;

use common::{RemoteError, TestClient, spawn_daemon};
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_KIND, BROWSER_LIST_METHOD, DAEMON_API_VERSION, DAEMON_KIND,
    DAEMON_STATUS_METHOD, RUNTIME_API_VERSION, RUNTIME_KIND, RUNTIME_STATUS_METHOD,
    SYSTEM_API_VERSION, SYSTEM_KIND, SYSTEM_PING_METHOD, TERMINAL_API_VERSION, TERMINAL_KIND,
    TERMINAL_STATUS_METHOD,
};
use dsh_daemon::envelope::{
    Envelope, EnvelopeKind, ErrorCode, PROTOCOL, Participant, ProtocolCoordinate,
    UnavailableReason, new_message_id, now_timestamp, validate_envelope,
};
use dsh_daemon::server::LEASE_MAX_SECONDS;
use dsh_local_transport::{Limits, LocalClient};
use std::time::Duration;

fn system() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: SYSTEM_API_VERSION.into(),
        kind: SYSTEM_KIND.into(),
    }
}

fn daemon() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: DAEMON_API_VERSION.into(),
        kind: DAEMON_KIND.into(),
    }
}

fn browser() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: BROWSER_API_VERSION.into(),
        kind: BROWSER_KIND.into(),
    }
}

fn terminal() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: TERMINAL_API_VERSION.into(),
        kind: TERMINAL_KIND.into(),
    }
}

fn runtime() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: RUNTIME_API_VERSION.into(),
        kind: RUNTIME_KIND.into(),
    }
}

fn all_catalog() -> Vec<ProtocolCoordinate> {
    vec![system(), daemon(), browser(), terminal(), runtime()]
}

/// 1) Full closed loop: negotiate (all catalog capabilities granted,
///    broker grants + bounded lease) → ping → success Result.
#[test]
fn ping_round_trip_succeeds() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);

    let agreement = client.negotiate(all_catalog());
    assert_eq!(agreement.granted, all_catalog());
    assert!(agreement.unavailable.is_empty());
    assert!(client.activation_id().is_some());

    let payload = client
        .invoke(
            system(),
            SYSTEM_PING_METHOD,
            serde_json::json!({ "message": "hello" }),
        )
        .expect("ping succeeds");
    assert_eq!(payload["pong"], true);
    assert_eq!(payload["echo"]["message"], "hello");
}

/// 2) `daemon.status` reports identity, live counters and the M6-B1
///    resource placeholders.
#[test]
fn daemon_status_reports_identity_and_placeholders() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![daemon()]);

    let status = client
        .invoke(daemon(), DAEMON_STATUS_METHOD, serde_json::json!({}))
        .expect("daemon.status succeeds");
    assert_eq!(status["daemonVersion"], "0.1.0");
    assert!(status["pid"].as_u64().is_some_and(|pid| pid > 0));
    assert!(status["startedAt"].as_str().is_some());
    assert!(status["uptimeSeconds"].as_u64().is_some());
    assert_eq!(status["claimPort"], dsh_daemon::credential::CLAIM_PORT);
    assert_eq!(status["port"].as_u64(), Some(addr.port().into()));
    assert_eq!(status["connections"].as_u64(), Some(1));
    assert_eq!(status["credentialsIssued"].as_u64(), Some(1));
    assert_eq!(status["activations"].as_u64(), Some(1));
    // M6-B1 placeholders; wired in M6-C.
    assert_eq!(status["resources"]["browsers"], 0);
    assert_eq!(status["resources"]["terminals"], 0);
    assert_eq!(status["resources"]["managedRuntimes"], 0);
}

/// 3) The placeholder capability methods are reachable and return their
///    static shapes.
#[test]
fn placeholder_capabilities_are_reachable() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(all_catalog());

    let browsers = client
        .invoke(browser(), BROWSER_LIST_METHOD, serde_json::json!({}))
        .expect("list_browsers succeeds");
    assert_eq!(browsers["browsers"].as_array().map(Vec::len), Some(0));

    let terminals = client
        .invoke(terminal(), TERMINAL_STATUS_METHOD, serde_json::json!({}))
        .expect("terminal.status succeeds");
    assert_eq!(terminals["count"], 0);
    assert_eq!(terminals["sessions"].as_array().map(Vec::len), Some(0));

    // runtime.status is real since M6-C2: the placeholder empty payload
    // is now malformed (the method requires the schema environmentId
    // payload). The full lifecycle runs against an isolated catalog in
    // runtime_integration.rs.
    let error = client
        .invoke(runtime(), RUNTIME_STATUS_METHOD, serde_json::json!({}))
        .expect_err("runtime.status is real: empty payload is malformed");
    assert_eq!(error.code, ErrorCode::MalformedMessage);
}

/// 4) Invocation without any Agreement on the connection → UNAUTHORIZED,
///    error.correlationId echoes the Invocation id.
#[test]
fn invocation_without_negotiation_is_rejected() {
    let (addr, credential, _server) = spawn_daemon();
    let mut raw = LocalClient::connect(addr, &credential, &Limits::default()).unwrap();

    let invocation = Envelope {
        protocol: PROTOCOL.into(),
        id: new_message_id(),
        kind: EnvelopeKind::Invocation,
        reply_to: None,
        participant: Participant {
            component: "raw".into(),
            facet: "probe".into(),
            activation_id: None,
        },
        timestamp: now_timestamp(),
        generation: 0,
        capability: Some(system()),
        method: Some(SYSTEM_PING_METHOD.into()),
        payload: Some(serde_json::json!({})),
        error: None,
    };
    raw.send_json(&invocation).unwrap();

    let result: Envelope = raw.recv_json().unwrap().expect("server must reply");
    assert_eq!(result.kind, EnvelopeKind::Result);
    let error = result.error.as_ref().expect("UNAUTHORIZED error Result");
    assert_eq!(error.code, ErrorCode::Unauthorized);
    assert!(!error.retryable);
    assert_eq!(error.correlation_id, invocation.id);
    assert_eq!(result.reply_to.as_deref(), Some(invocation.id.as_str()));
}

/// 5) A capability outside the daemon catalog is policy_denied in the
///    Agreement and stays unauthorized on invocation (fail-closed).
#[test]
fn unknown_capability_is_policy_denied() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);

    let unknown = ProtocolCoordinate {
        api_version: "video.dsh-desktop.local/v1alpha1".into(),
        kind: "Video".into(),
    };
    let agreement = client.negotiate(vec![unknown.clone()]);
    assert!(agreement.granted.is_empty());
    assert_eq!(agreement.unavailable.len(), 1);
    assert_eq!(agreement.unavailable[0].coordinate, unknown);
    assert_eq!(
        agreement.unavailable[0].reason,
        UnavailableReason::PolicyDenied
    );

    let error = client
        .invoke(unknown, "list", serde_json::json!({}))
        .expect_err("not granted → UNAUTHORIZED");
    assert_eq!(error.code, ErrorCode::Unauthorized);
    assert!(!error.retryable);
}

/// 6) A second Hello on the same connection issues a *new* activation that
///    supersedes the previous one at the broker (ADR-0018 decision 1): the
///    old activation now fails the dispatch gate with STALE_GENERATION.
#[test]
fn renegotiation_supersedes_previous_activation() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);

    let first = client.negotiate(vec![system()]);
    assert_eq!(first.granted, vec![system()]);
    let first_activation = first.activation_id.clone();
    client
        .invoke(system(), SYSTEM_PING_METHOD, serde_json::json!({}))
        .expect("first activation works");

    let second = client.negotiate(vec![system()]);
    assert_ne!(second.activation_id, first_activation);
    client
        .invoke(system(), SYSTEM_PING_METHOD, serde_json::json!({}))
        .expect("second activation works");

    // The old activation was superseded at the broker: generation change.
    let error = client
        .invoke_as(
            &first_activation,
            system(),
            SYSTEM_PING_METHOD,
            serde_json::json!({}),
        )
        .expect_err("superseded activation fails the gate");
    assert_eq!(error.code, ErrorCode::StaleGeneration);
    assert!(!error.retryable);
}

/// 7) A frame that fails envelope validation is answered with a
///    MALFORMED_MESSAGE Result that still correlates by id.
#[test]
fn malformed_envelope_gets_malformed_message() {
    let (addr, credential, _server) = spawn_daemon();
    let mut raw = LocalClient::connect(addr, &credential, &Limits::default()).unwrap();

    let mut bad = Envelope {
        protocol: PROTOCOL.into(),
        id: new_message_id(),
        kind: EnvelopeKind::Invocation,
        reply_to: None,
        participant: Participant {
            component: "raw".into(),
            facet: "probe".into(),
            activation_id: None,
        },
        timestamp: now_timestamp(),
        generation: 0,
        capability: Some(system()),
        method: Some(SYSTEM_PING_METHOD.into()),
        payload: Some(serde_json::json!({})),
        error: None,
    };
    bad.protocol = "interop.dsh-desktop.local/v2".into();
    raw.send_json(&bad).unwrap();

    let result: Envelope = raw.recv_json().unwrap().expect("server must reply");
    assert_eq!(result.kind, EnvelopeKind::Result);
    let error = result
        .error
        .as_ref()
        .expect("MALFORMED_MESSAGE error Result");
    assert_eq!(error.code, ErrorCode::MalformedMessage);
    assert_eq!(error.correlation_id, bad.id);
}

/// 8) The daemon Agreement and Result outputs are frame-valid, so a strict
///    client (or the TS capability-contracts validator) can consume them.
#[test]
fn server_agreement_and_results_are_frame_valid() {
    let (addr, credential, _server) = spawn_daemon();
    let mut raw = LocalClient::connect(addr, &credential, &Limits::default()).unwrap();

    let hello = Envelope {
        protocol: PROTOCOL.into(),
        id: new_message_id(),
        kind: EnvelopeKind::Hello,
        reply_to: None,
        participant: Participant {
            component: "raw".into(),
            facet: "probe".into(),
            activation_id: None,
        },
        timestamp: now_timestamp(),
        generation: 0,
        capability: None,
        method: None,
        payload: Some(serde_json::json!({
            "instanceId": "raw-probe-00000001",
            "supports": [
                { "apiVersion": SYSTEM_API_VERSION, "kind": SYSTEM_KIND },
                { "apiVersion": BROWSER_API_VERSION, "kind": BROWSER_KIND },
            ],
            "requires": [],
        })),
        error: None,
    };
    raw.send_json(&hello).unwrap();
    let agreement: Envelope = raw.recv_json().unwrap().unwrap();
    assert_eq!(agreement.kind, EnvelopeKind::Agreement);
    validate_envelope(&agreement).expect("Agreement must be frame-valid");
    assert_eq!(agreement.reply_to.as_deref(), Some(hello.id.as_str()));
    // The broker-driven Agreement advertises the bounded lease.
    assert_eq!(
        agreement.payload.as_ref().unwrap()["leaseConstraints"]["maxSeconds"],
        LEASE_MAX_SECONDS
    );

    let mut invocation = hello;
    invocation.id = new_message_id();
    invocation.kind = EnvelopeKind::Invocation;
    invocation.participant.activation_id = agreement
        .payload
        .as_ref()
        .and_then(|p| p.get("activationId"))
        .and_then(|a| a.as_str())
        .map(String::from);
    invocation.capability = Some(system());
    invocation.method = Some(SYSTEM_PING_METHOD.into());
    raw.send_json(&invocation).unwrap();
    let result: Envelope = raw.recv_json().unwrap().unwrap();
    validate_envelope(&result).expect("Result must be frame-valid");
    assert_eq!(result.kind, EnvelopeKind::Result);
    assert_eq!(result.reply_to.as_deref(), Some(invocation.id.as_str()));
    assert!(result.error.is_none());
    assert_eq!(result.payload.as_ref().unwrap()["pong"], true);
}

/// 9) Sessions are isolated per connection: an activation from a previous
///    connection is unknown on a new connection (UNAUTHORIZED), and the
///    broker tracks the new activation independently.
#[test]
fn sessions_are_isolated_per_connection() {
    let (addr, credential, server) = spawn_daemon();
    let mut first = TestClient::connect(addr, &credential);
    let agreement = first.negotiate(vec![system()]);
    let old_activation = agreement.activation_id;
    drop(first);

    // Credentials are single-use (AC-IPC-001): the second connection needs
    // a fresh one from the daemon.
    let second_credential = server.issue_credential(Duration::from_secs(300));
    let mut second = TestClient::connect(addr, &second_credential);
    second.negotiate(vec![system()]);
    let error = second
        .invoke_as(
            &old_activation,
            system(),
            SYSTEM_PING_METHOD,
            serde_json::json!({}),
        )
        .expect_err("activation from a previous connection is unknown");
    assert_eq!(error.code, ErrorCode::Unauthorized);
}

/// 10) Unknown methods on a granted capability → UNAVAILABLE (not
///     implemented), retryable per the protocol model.
#[test]
fn unknown_method_is_unavailable() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![system()]);
    let error = client
        .invoke(system(), "shutdown", serde_json::json!({}))
        .expect_err("shutdown is not implemented");
    let RemoteError {
        code,
        retryable,
        message,
    } = error;
    assert_eq!(code, ErrorCode::Unavailable);
    assert!(!retryable);
    assert!(message.contains("not implemented"));
}
