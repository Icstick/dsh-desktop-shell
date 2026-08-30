//! Wire-level tests for the real browser session state capability
//! (M6-C3): the daemon is the **state authority** for browser sessions
//! (ADR-0019 decision 2, option A — rendering in the Shell, state in the
//! daemon). browser.create / browser.list / browser.status /
//! browser.close over the envelope, lifecycle events
//! (browser.session-created / browser.session-closed) routed by session
//! id, and connection-scoped ownership. Pure logic — no WebView2, so
//! these are fast.

mod common;

use common::{TestClient, spawn_daemon};
use dsh_daemon::browser::{BROWSER_SESSION_CLOSED_EVENT, BROWSER_SESSION_CREATED_EVENT};
use dsh_daemon::capabilities::{
    BROWSER_API_VERSION, BROWSER_CLOSE_METHOD, BROWSER_CREATE_METHOD, BROWSER_KIND,
    BROWSER_LIST_METHOD, BROWSER_STATUS_METHOD, DAEMON_API_VERSION, DAEMON_KIND,
    DAEMON_STATUS_METHOD,
};
use dsh_daemon::envelope::{
    EnvelopeKind, ErrorCode, ProtocolCoordinate, UnavailableReason, validate_envelope,
};
use std::time::Duration;

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

fn create_request() -> serde_json::Value {
    serde_json::json!({ "schemaVersion": 1, "mode": "human_surface" })
}

fn close_request(session_id: &str) -> serde_json::Value {
    serde_json::json!({ "schemaVersion": 1, "sessionId": session_id })
}

/// Assert the opaque session id shape (ADR-0017 decision 5,
/// `^brw-[a-z0-9-]+$`, exactly `brw-<ms>-<seq>`).
fn assert_opaque_id(session_id: &str) {
    assert!(
        session_id.starts_with("brw-"),
        "id must start with brw-: {session_id}"
    );
    let rest = &session_id[4..];
    let mut parts = rest.split('-');
    let ms = parts.next().expect("ms part");
    let seq = parts.next().expect("seq part");
    assert!(
        parts.next().is_none(),
        "id must have exactly two parts: {session_id}"
    );
    assert!(
        !ms.is_empty() && ms.chars().all(|c| c.is_ascii_digit()),
        "ms must be digits: {session_id}"
    );
    assert!(
        !seq.is_empty() && seq.chars().all(|c| c.is_ascii_digit()),
        "seq must be digits: {session_id}"
    );
}

/// 1) Full lifecycle over the wire: create → report (opaque id) →
///    session-created Event → list contains the session → status →
///    daemon.status real counter → close → session-closed Event →
///    list empty.
#[test]
fn browser_session_lifecycle_over_envelope() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![browser(), daemon()]);

    let report = client
        .invoke(browser(), BROWSER_CREATE_METHOD, create_request())
        .expect("browser.create succeeds");
    let session_id = report["sessionId"].as_str().expect("sessionId").to_string();
    assert_opaque_id(&session_id);
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["state"], "created");
    assert_eq!(report["mode"], "human_surface");
    assert!(report["currentUrl"].is_null());
    assert!(report["createdAtUnixMs"].as_u64().is_some_and(|ms| ms > 0));
    assert!(report["lastActivityUnixMs"].is_null());

    // The creating connection receives the session-created Event
    // (frame-valid, browser capability, kind "created").
    let created = client
        .wait_for_event(BROWSER_SESSION_CREATED_EVENT, Duration::from_secs(5))
        .expect("session-created event");
    assert_eq!(created.kind, EnvelopeKind::Event);
    validate_envelope(&created).expect("Event must be frame-valid");
    assert_eq!(
        created.capability.as_ref().map(|c| c.kind.as_str()),
        Some(BROWSER_KIND)
    );
    let payload = created.payload.as_ref().expect("event payload");
    assert_eq!(payload["schemaVersion"], 1);
    assert_eq!(payload["sessionId"], session_id);
    assert_eq!(payload["kind"], "created");
    assert!(payload["occurredAtUnixMs"].as_u64().is_some());
    assert!(payload["url"].is_null());

    let listed = client
        .invoke(browser(), BROWSER_LIST_METHOD, serde_json::json!({}))
        .expect("browser.list succeeds");
    assert_eq!(listed["browsers"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed["browsers"][0]["sessionId"], session_id);
    assert_eq!(listed["browsers"][0]["state"], "created");

    let status = client
        .invoke(browser(), BROWSER_STATUS_METHOD, serde_json::json!({}))
        .expect("browser.status succeeds");
    assert_eq!(status["count"], 1);
    assert_eq!(status["sessions"][0]["sessionId"], session_id);

    // daemon.status reports the real browser resource counter (M6-C3).
    let daemon_status = client
        .invoke(daemon(), DAEMON_STATUS_METHOD, serde_json::json!({}))
        .expect("daemon.status succeeds");
    assert_eq!(daemon_status["resources"]["browsers"], 1);

    client
        .invoke(browser(), BROWSER_CLOSE_METHOD, close_request(&session_id))
        .expect("browser.close succeeds");

    let closed = client
        .wait_for_event(BROWSER_SESSION_CLOSED_EVENT, Duration::from_secs(5))
        .expect("session-closed event");
    validate_envelope(&closed).expect("Event must be frame-valid");
    assert_eq!(closed.payload.as_ref().expect("payload")["kind"], "closed");
    assert_eq!(
        closed.payload.as_ref().expect("payload")["sessionId"],
        session_id
    );

    let listed = client
        .invoke(browser(), BROWSER_LIST_METHOD, serde_json::json!({}))
        .expect("browser.list succeeds");
    assert_eq!(listed["browsers"].as_array().map(Vec::len), Some(0));

    let status = client
        .invoke(browser(), BROWSER_STATUS_METHOD, serde_json::json!({}))
        .expect("browser.status succeeds");
    assert_eq!(status["count"], 0);

    let daemon_status = client
        .invoke(daemon(), DAEMON_STATUS_METHOD, serde_json::json!({}))
        .expect("daemon.status succeeds");
    assert_eq!(daemon_status["resources"]["browsers"], 0);
}

/// 2) Broker-relaxed browser use is Shell-only (REVIEW-M6-DAEMON HIGH-2):
///    a non-Shell participant conflicting with the single-owner grant
///    gets nothing and every browser method is rejected by the grant
///    check; the owner's lifecycle events are connection-scoped (no
///    leakage to other connections).
#[test]
fn browser_events_are_connection_scoped() {
    let (addr, credential, server) = spawn_daemon();
    let agent_credential = server.issue_credential(Duration::from_secs(300));
    let mut first = TestClient::connect(addr, &credential);
    let mut agent = TestClient::connect_as(addr, &agent_credential, "other-agent", "automation");
    first.negotiate(vec![browser()]);

    // The non-Shell participant conflicts with the single-owner grant:
    // fail-closed (nothing granted, PolicyDenied).
    let agreement = agent.negotiate(vec![browser()]);
    assert!(
        agreement.granted.is_empty(),
        "agent must not be granted browser"
    );
    assert_eq!(agreement.unavailable.len(), 1);
    assert_eq!(
        agreement.unavailable[0].reason,
        UnavailableReason::PolicyDenied
    );

    // The agent cannot use the capability at all: every browser method is
    // rejected by the grant check (stronger than owner-scoping).
    for method in [
        BROWSER_CREATE_METHOD,
        BROWSER_LIST_METHOD,
        BROWSER_STATUS_METHOD,
        BROWSER_CLOSE_METHOD,
    ] {
        let error = agent
            .invoke(browser(), method, serde_json::json!({}))
            .expect_err("agent cannot use the browser capability");
        assert_eq!(error.code, ErrorCode::Unauthorized, "{method}");
    }

    // The owner creates a session and receives its lifecycle event.
    let first_report = first
        .invoke(browser(), BROWSER_CREATE_METHOD, create_request())
        .expect("first session");
    let first_session = first_report["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    let first_created = first
        .wait_for_event(BROWSER_SESSION_CREATED_EVENT, Duration::from_secs(5))
        .expect("first created event");
    assert_eq!(
        first_created.payload.as_ref().expect("payload")["sessionId"],
        first_session
    );

    // No cross-connection leakage: the agent never sees a created event
    // (it has no sessions; the daemon routes events to the owner only).
    let leaked = agent.wait_for_event_matching(
        BROWSER_SESSION_CREATED_EVENT,
        |_| true,
        Duration::from_millis(400),
    );
    assert!(leaked.is_none(), "agent leaked the owner session event");

    first
        .invoke(
            browser(),
            BROWSER_CLOSE_METHOD,
            close_request(&first_session),
        )
        .expect("first closes its own");
}

/// 3) Ownership is connection-scoped (an opaque session id is not an
///    access token): after a Shell reconnect the sessions of the dead
///    connection stay alive (resource survival, ADR-0008) but their owner
///    is the old connection — close from the restarted Shell is
///    NOT_PROCESS_OWNER until the daemon-side handover slice (M6-C TODO).
#[test]
fn browser_session_ownership_is_connection_scoped() {
    let (addr, credential, server) = spawn_daemon();
    let mut first = TestClient::connect(addr, &credential);
    first.negotiate(vec![browser()]);

    let report = first
        .invoke(browser(), BROWSER_CREATE_METHOD, create_request())
        .expect("first session");
    let session_id = report["sessionId"].as_str().expect("sessionId").to_string();

    // The Shell closes (connection drops); the session survives.
    drop(first);

    // A fresh Shell connection (same identity, fresh credential) rejoins
    // the daemon; the session is still listed in the authority view.
    let fresh = server.issue_credential(Duration::from_secs(300));
    let mut restarted = TestClient::connect(addr, &fresh);
    restarted.negotiate(vec![browser()]);
    let listed = restarted
        .invoke(browser(), BROWSER_LIST_METHOD, serde_json::json!({}))
        .expect("restarted browser.list succeeds");
    assert_eq!(listed["browsers"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed["browsers"][0]["sessionId"], session_id);

    // But ownership stayed with the dead connection: close is rejected
    // with NOT_PROCESS_OWNER (handover is the M6-C daemon-side TODO).
    let error = restarted
        .invoke(browser(), BROWSER_CLOSE_METHOD, close_request(&session_id))
        .expect_err("restarted connection is not the session owner");
    assert_eq!(error.code, ErrorCode::NotProcessOwner);
}

/// 4) Malformed requests fail closed with MALFORMED_MESSAGE; unknown or
///    already-closed sessions and unknown methods stay UNAVAILABLE.
#[test]
fn browser_validation_and_error_matrix() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![browser()]);

    // Wrong schema version.
    let error = client
        .invoke(
            browser(),
            BROWSER_CREATE_METHOD,
            serde_json::json!({ "schemaVersion": 2, "mode": "human_surface" }),
        )
        .expect_err("schema version 2");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Unsupported mode (schema const: human_surface only).
    let error = client
        .invoke(
            browser(),
            BROWSER_CREATE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "mode": "agent_automation" }),
        )
        .expect_err("agent mode");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Session id outside the opaque brw- shape.
    let error = client
        .invoke(
            browser(),
            BROWSER_CLOSE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "sessionId": "nope" }),
        )
        .expect_err("bad session id");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Well-formed id of a session that never existed.
    let error = client
        .invoke(
            browser(),
            BROWSER_CLOSE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "sessionId": "brw-0000000000000000-1" }),
        )
        .expect_err("unknown session");
    assert_eq!(error.code, ErrorCode::Unavailable);

    // A second close of the same session is rejected (already closed).
    let report = client
        .invoke(browser(), BROWSER_CREATE_METHOD, create_request())
        .expect("create");
    let session_id = report["sessionId"].as_str().expect("sessionId").to_string();
    client
        .invoke(browser(), BROWSER_CLOSE_METHOD, close_request(&session_id))
        .expect("close");
    let error = client
        .invoke(browser(), BROWSER_CLOSE_METHOD, close_request(&session_id))
        .expect_err("second close");
    assert_eq!(error.code, ErrorCode::Unavailable);

    // Unknown browser method → not implemented.
    let error = client
        .invoke(browser(), "browser.shutdown", serde_json::json!({}))
        .expect_err("unknown method");
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(error.message.contains("not implemented"));
}

/// 5) A connection without the browser grant in its Agreement cannot use
///    the capability at all (protocol-level fail-closed).
#[test]
fn browser_without_grant_is_unauthorized() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![]);
    let error = client
        .invoke(browser(), BROWSER_CREATE_METHOD, create_request())
        .expect_err("browser not granted");
    assert_eq!(error.code, ErrorCode::Unauthorized);
    let error = client
        .invoke(browser(), BROWSER_LIST_METHOD, serde_json::json!({}))
        .expect_err("browser not granted");
    assert_eq!(error.code, ErrorCode::Unauthorized);
}
