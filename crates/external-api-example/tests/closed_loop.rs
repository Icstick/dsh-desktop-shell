//! Integration tests for the unified external API reference loop.
//!
//! Every test drives the real wire: `LocalServer`/`LocalClient`
//! (dsh-local-transport) with one-time credentials — no mocks, no direct
//! function calls across the transport boundary.

use std::thread;
use std::time::Duration;

use dsh_external_api_example::catalog::{browser, system};
use dsh_external_api_example::client::{ClientError, ExampleClient};
use dsh_external_api_example::envelope::{
    AgreementPayload, Envelope, EnvelopeKind, ErrorCode, PROTOCOL, Participant, ProtocolError,
    UnavailableReason, new_message_id, now_timestamp, validate_envelope,
};
use dsh_external_api_example::server::{ExampleServer, GrantPolicy};
use dsh_local_transport::{Limits, LocalClient, LocalServer};

/// Bind a server and move it into a serving thread; return addr + credential.
fn spawn(policy: Option<GrantPolicy>) -> (std::net::SocketAddr, dsh_local_transport::Credential) {
    let server = match policy {
        Some(policy) => ExampleServer::bind_with_policy(Limits::default(), policy).unwrap(),
        None => ExampleServer::bind(Limits::default()).unwrap(),
    };
    let addr = server.addr();
    let credential = server.issue_credential(Duration::from_secs(300));
    thread::spawn(move || {
        loop {
            if let Some(conn) = server.take_connection() {
                server.serve_connection(conn);
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
    });
    (addr, credential)
}

fn connect_client(
    addr: std::net::SocketAddr,
    credential: &dsh_local_transport::Credential,
) -> ExampleClient {
    ExampleClient::connect(addr, credential, &Limits::default()).unwrap()
}

/// 1) Full closed loop: negotiate → ping → success Result with pong.
#[test]
fn ping_round_trip_succeeds() {
    let (addr, credential) = spawn(None);
    let mut client = connect_client(addr, &credential);

    let agreement = client.negotiate(vec![system(), browser()]).unwrap();
    assert_eq!(agreement.granted, vec![system()]);
    assert_eq!(agreement.unavailable.len(), 1);
    assert_eq!(agreement.unavailable[0].coordinate, browser());
    assert_eq!(
        agreement.unavailable[0].reason,
        UnavailableReason::PolicyDenied
    );
    assert!(client.activation_id().is_some());

    let payload = client
        .invoke(system(), "ping", serde_json::json!({ "message": "hello" }))
        .unwrap();
    assert_eq!(payload["pong"], true);
    assert_eq!(payload["echo"]["message"], "hello");
}

/// 2) Capability not in the Agreement's granted set → UNAUTHORIZED.
#[test]
fn ungranted_capability_is_rejected() {
    let (addr, credential) = spawn(None);
    let mut client = connect_client(addr, &credential);
    client.negotiate(vec![system(), browser()]).unwrap();

    let err = client
        .invoke(browser(), "list_browsers", serde_json::json!({}))
        .unwrap_err();
    match err {
        ClientError::Remote {
            code,
            retryable,
            message,
        } => {
            assert_eq!(code, ErrorCode::Unauthorized);
            assert!(!retryable);
            assert!(message.contains("not granted"));
        }
        other => panic!("expected Remote UNAUTHORIZED, got {other:?}"),
    }
}

/// 3) Invocation without any Agreement on the connection → UNAUTHORIZED.
#[test]
fn invocation_without_negotiation_is_rejected() {
    let (addr, credential) = spawn(None);
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
        method: Some("ping".into()),
        payload: Some(serde_json::json!({})),
        error: None,
    };
    raw.send_json(&invocation).unwrap();

    let result: Envelope = raw.recv_json().unwrap().expect("server must reply");
    assert_eq!(result.kind, EnvelopeKind::Result);
    let error = result.error.as_ref().expect("UNAUTHORIZED error Result");
    assert_eq!(error.code, ErrorCode::Unauthorized);
    assert_eq!(error.correlation_id, invocation.id);
    assert_eq!(result.reply_to.as_deref(), Some(invocation.id.as_str()));
}

/// 4) A Result whose error.correlationId does not match the Invocation id is
///    rejected by the client (semantics.ts correlation-match).
#[test]
fn correlation_mismatch_is_rejected() {
    // Fake server: negotiates normally, then answers the Invocation with a
    // Result carrying a bogus correlationId.
    let server = LocalServer::bind(Limits::default()).unwrap();
    let addr = server.addr();
    let credential = server.issue_credential(Duration::from_secs(300));
    thread::spawn(move || serve_fake(server));

    let mut client = connect_client(addr, &credential);
    client.negotiate(vec![system()]).unwrap();
    let err = client
        .invoke(system(), "ping", serde_json::json!({}))
        .unwrap_err();
    match err {
        ClientError::CorrelationMismatch { expected, got } => {
            assert!(expected.starts_with("msg-"));
            assert_eq!(got, "msg-bogus-correlation-0001");
        }
        other => panic!("expected CorrelationMismatch, got {other:?}"),
    }
}

fn serve_fake(server: LocalServer) {
    loop {
        let Some(conn) = server.connections().into_iter().next() else {
            thread::sleep(Duration::from_millis(5));
            continue;
        };
        let hello: Envelope = conn.recv_json().unwrap().expect("hello");
        let agreement = Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind: EnvelopeKind::Agreement,
            reply_to: Some(hello.id.clone()),
            participant: Participant {
                component: "fake".into(),
                facet: "server".into(),
                activation_id: Some("act-fake-0000000001".into()),
            },
            timestamp: now_timestamp(),
            generation: 0,
            capability: None,
            method: None,
            payload: Some(
                serde_json::to_value(AgreementPayload {
                    activation_id: "act-fake-0000000001".into(),
                    granted: vec![system()],
                    unavailable: Vec::new(),
                    lease_constraints: None,
                })
                .unwrap(),
            ),
            error: None,
        };
        conn.send_json(&agreement).unwrap();

        let invocation: Envelope = conn.recv_json().unwrap().expect("invocation");
        let bogus = Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind: EnvelopeKind::Result,
            reply_to: Some(invocation.id.clone()),
            participant: Participant {
                component: "fake".into(),
                facet: "server".into(),
                activation_id: None,
            },
            timestamp: now_timestamp(),
            generation: 1,
            capability: invocation.capability.clone(),
            method: invocation.method.clone(),
            payload: None,
            error: Some(ProtocolError {
                code: ErrorCode::Unauthorized,
                message: "no such lease".into(),
                retryable: false,
                correlation_id: "msg-bogus-correlation-0001".into(),
            }),
        };
        conn.send_json(&bogus).unwrap();
        let _ = conn.recv(); // wait for the client to close
        return;
    }
}

/// 5) A frame that fails envelope validation is answered with a
///    MALFORMED_MESSAGE Result that still correlates by id.
#[test]
fn malformed_envelope_gets_malformed_message() {
    let (addr, credential) = spawn(None);
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
        method: Some("ping".into()),
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

/// 6) With a policy that grants the browser capability, the static
///    list_browsers dispatch is reachable (example capability, both halves).
#[test]
fn browser_list_succeeds_when_granted() {
    let policy = GrantPolicy {
        grantable: vec![system(), browser()],
    };
    let (addr, credential) = spawn(Some(policy));
    let mut client = connect_client(addr, &credential);

    let agreement = client.negotiate(vec![system(), browser()]).unwrap();
    assert_eq!(agreement.granted, vec![system(), browser()]);
    assert!(agreement.unavailable.is_empty());

    let payload = client
        .invoke(browser(), "list_browsers", serde_json::json!({}))
        .unwrap();
    assert_eq!(payload["browsers"][0]["name"], "edge");
    assert_eq!(payload["browsers"][1]["kind"], "cdp");
}

/// The example server's own Agreement output must be frame-valid, so a
/// strict client (or a TS capability-contracts validator) can consume it.
#[test]
fn server_agreement_and_results_are_frame_valid() {
    let (addr, credential) = spawn(None);
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
            "instanceId": "ext-tool-probe-0001",
            "supports": [
                { "apiVersion": "system.dsh-desktop.local/v1alpha1", "kind": "System" },
                { "apiVersion": "browser.dsh-desktop.local/v1alpha1", "kind": "Browser" },
            ],
            "requires": [],
        })),
        error: None,
    };
    raw.send_json(&hello).unwrap();
    let agreement: Envelope = raw.recv_json().unwrap().unwrap();
    assert_eq!(agreement.kind, EnvelopeKind::Agreement);
    assert!(validate_envelope(&agreement).is_ok());
    assert_eq!(agreement.reply_to.as_deref(), Some(hello.id.as_str()));

    // Invoke ping under the activation from the agreement.
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
    invocation.method = Some("ping".into());
    raw.send_json(&invocation).unwrap();
    let result: Envelope = raw.recv_json().unwrap().unwrap();
    assert!(validate_envelope(&result).is_ok());
    assert_eq!(result.kind, EnvelopeKind::Result);
    assert_eq!(result.reply_to.as_deref(), Some(invocation.id.as_str()));
    assert!(result.error.is_none());
    assert_eq!(result.payload.as_ref().unwrap()["pong"], true);
}
