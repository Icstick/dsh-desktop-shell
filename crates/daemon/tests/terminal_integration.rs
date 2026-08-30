//! Wire-level tests for the real terminal capability (M6-C1): the daemon
//! hosts PTY sessions over the envelope (terminal.create / write / resize /
//! close / status), routes output events by session id, and gates agent
//! sessions through the broker while human sessions ride on the
//! credential-authenticated connection alone.
//!
//! Real PTYs (cmd.exe on ConPTY) — same pattern as the provider and M5
//! bridge tests; every wait polls the wire with a generous deadline.

mod common;

use common::{TestClient, spawn_daemon};
use dsh_daemon::capabilities::{
    DAEMON_API_VERSION, DAEMON_KIND, DAEMON_STATUS_METHOD, TERMINAL_API_VERSION,
    TERMINAL_CLOSE_METHOD, TERMINAL_CREATE_METHOD, TERMINAL_KIND, TERMINAL_RESIZE_METHOD,
    TERMINAL_STATUS_METHOD, TERMINAL_WRITE_METHOD,
};
use dsh_daemon::envelope::{EnvelopeKind, ErrorCode, ProtocolCoordinate, validate_envelope};
use dsh_daemon::terminal::TERMINAL_OUTPUT_EVENT;
use std::time::Duration;

fn terminal() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: TERMINAL_API_VERSION.into(),
        kind: TERMINAL_KIND.into(),
    }
}

fn daemon() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: DAEMON_API_VERSION.into(),
        kind: DAEMON_KIND.into(),
    }
}

fn human_create(cols: u16, rows: u16) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "mode": "human_surface",
        "cols": cols,
        "rows": rows,
    })
}

fn agent_create(
    agent_id: &str,
    activation_id: &str,
    generation: u64,
    workspace: &str,
) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "mode": "agent_automation",
        "cols": 80,
        "rows": 24,
        "agent": {
            "agentId": agent_id,
            "activationId": activation_id,
            "generation": generation,
            "scope": { "workspace": workspace },
        },
    })
}

fn write_request(session_id: &str, data: &str) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "sessionId": session_id,
        "data": data,
    })
}

fn close_request(session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "sessionId": session_id,
    })
}

/// 1) Full human roundtrip over the wire: create → write → output Event
///    → resize → status → close, with daemon.status reporting the live
///    session count.
#[test]
fn human_terminal_roundtrip_over_envelope() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![terminal(), daemon()]);

    let report = client
        .invoke(terminal(), TERMINAL_CREATE_METHOD, human_create(80, 24))
        .expect("terminal.create succeeds for a human session");
    let session_id = report["sessionId"].as_str().expect("sessionId").to_string();
    assert!(session_id.starts_with("pty-"));
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["state"], "running");
    assert_eq!(report["mode"], "human_surface");
    assert_eq!(report["cols"], 80);
    assert_eq!(report["rows"], 24);

    client
        .invoke(
            terminal(),
            TERMINAL_WRITE_METHOD,
            write_request(&session_id, "echo daemon-human-pty-ok\r\n"),
        )
        .expect("terminal.write succeeds");

    // Output events flow back as frame-valid envelope Events with the
    // terminal.output method and the session id (the creator connection
    // is the subscriber).
    let saw = client.wait_for_output("daemon-human-pty-ok", Duration::from_secs(10));
    assert!(saw.contains("daemon-human-pty-ok"), "output: {saw:?}");
    let events = client.events();
    assert!(!events.is_empty(), "at least one output event");
    let last = events.last().expect("event");
    assert_eq!(last.kind, EnvelopeKind::Event);
    validate_envelope(last).expect("Event must be frame-valid");
    assert_eq!(last.method.as_deref(), Some(TERMINAL_OUTPUT_EVENT));
    assert_eq!(
        last.capability.as_ref().map(|c| c.kind.as_str()),
        Some(TERMINAL_KIND)
    );
    let payload = last.payload.as_ref().expect("event payload");
    assert_eq!(payload["schemaVersion"], 1);
    assert_eq!(payload["sessionId"], session_id);
    assert!(payload["seq"].as_u64().is_some_and(|seq| seq >= 1));
    assert!(payload["timestampUnixMs"].as_u64().is_some());
    // Output events of one session all carry the same session id.
    assert!(events.iter().all(|event| {
        event
            .payload
            .as_ref()
            .and_then(|p| p.get("sessionId"))
            .and_then(|s| s.as_str())
            == Some(session_id.as_str())
    }));

    let resized = client
        .invoke(
            terminal(),
            TERMINAL_RESIZE_METHOD,
            serde_json::json!({
                "schemaVersion": 1,
                "sessionId": session_id,
                "cols": 100,
                "rows": 40,
            }),
        )
        .expect("terminal.resize succeeds");
    assert_eq!(resized["cols"], 100);
    assert_eq!(resized["rows"], 40);
    assert_eq!(resized["mode"], "human_surface");

    let status = client
        .invoke(terminal(), TERMINAL_STATUS_METHOD, serde_json::json!({}))
        .expect("terminal.status succeeds");
    assert_eq!(status["count"], 1);
    assert_eq!(status["sessions"][0]["sessionId"], session_id);

    let daemon_status = client
        .invoke(daemon(), DAEMON_STATUS_METHOD, serde_json::json!({}))
        .expect("daemon.status succeeds");
    assert_eq!(daemon_status["resources"]["terminals"], 1);

    client
        .invoke(
            terminal(),
            TERMINAL_CLOSE_METHOD,
            close_request(&session_id),
        )
        .expect("terminal.close succeeds");
    let status = client
        .invoke(terminal(), TERMINAL_STATUS_METHOD, serde_json::json!({}))
        .expect("terminal.status succeeds");
    assert_eq!(status["count"], 0);
}

/// 2) Malformed terminal requests fail closed with MALFORMED_MESSAGE;
///    unknown sessions and methods stay unavailable.
#[test]
fn terminal_validation_matrix() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![terminal()]);

    // Wrong schema version.
    let error = client
        .invoke(
            terminal(),
            TERMINAL_CREATE_METHOD,
            serde_json::json!({ "schemaVersion": 2, "mode": "human_surface", "cols": 80, "rows": 24 }),
        )
        .expect_err("schema version 2");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Cross-mode: human_surface carrying agent facts.
    let error = client
        .invoke(
            terminal(),
            TERMINAL_CREATE_METHOD,
            serde_json::json!({
                "schemaVersion": 1,
                "mode": "human_surface",
                "cols": 80,
                "rows": 24,
                "agent": {
                    "agentId": "a|b",
                    "activationId": "act-0001",
                    "generation": 1,
                    "scope": { "workspace": "ws-a" },
                },
            }),
        )
        .expect_err("cross-mode");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // agent_automation without agent facts.
    let error = client
        .invoke(
            terminal(),
            TERMINAL_CREATE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "mode": "agent_automation", "cols": 80, "rows": 24 }),
        )
        .expect_err("agent required");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Geometry outside schema bounds (provider InvalidGeometry).
    let error = client
        .invoke(
            terminal(),
            TERMINAL_CREATE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "mode": "human_surface", "cols": 10, "rows": 24 }),
        )
        .expect_err("cols too small");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Session id outside the opaque pty- shape.
    let error = client
        .invoke(
            terminal(),
            TERMINAL_WRITE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "sessionId": "not-a-pty", "data": "x" }),
        )
        .expect_err("bad session id");
    assert_eq!(error.code, ErrorCode::MalformedMessage);

    // Well-formed id of a session that never existed.
    let error = client
        .invoke(
            terminal(),
            TERMINAL_WRITE_METHOD,
            serde_json::json!({ "schemaVersion": 1, "sessionId": "pty-0000000000000000-1", "data": "x" }),
        )
        .expect_err("unknown session");
    assert_eq!(error.code, ErrorCode::Unavailable);

    // Unknown terminal method → not implemented.
    let error = client
        .invoke(terminal(), "terminal.shutdown", serde_json::json!({}))
        .expect_err("unknown method");
    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(error.message.contains("not implemented"));
}

/// 3) Agent sessions gate through the broker (AC-TERM-001): facts without
///    a negotiation are UNAUTHORIZED; after the agent negotiates, its
///    create passes and every later mutation re-validates the recorded
///    binding. Human sessions never touch the broker. A second connection
///    negotiating the same capability conflicts at the broker (single
///    owner per capability); the Shell identity keeps the protocol-level
///    Agreement grant through the broker-relaxed path (HIGH-2).
#[test]
fn agent_sessions_gate_through_the_broker() {
    let (addr, credential, server) = spawn_daemon();

    // The agent negotiates terminal on its own connection (daemon Hello →
    // broker grant for component|facet, generation 1).
    let agent_credential = server.issue_credential(Duration::from_secs(300));
    let mut agent = TestClient::connect_as(addr, &agent_credential, "dsh-agent", "main");
    let agreement = agent.negotiate(vec![terminal()]);
    let agent_id = "dsh-agent-main".to_string();
    let activation_id = agreement.activation_id.clone();

    // The shell connection (human surface) negotiates too: the broker
    // keeps the agent as the single terminal owner (M6-C1 decision), and
    // the Shell identity keeps the protocol-level grant through the
    // broker-relaxed path (REVIEW-M6-DAEMON HIGH-2: Shell-only).
    let mut shell = TestClient::connect_as(addr, &credential, "dsh-desktop-shell", "shell");
    let shell_agreement = shell.negotiate(vec![terminal()]);
    assert_eq!(shell_agreement.granted, vec![terminal()]);

    // 3a) Agent facts without any negotiation → UNAUTHORIZED.
    let error = shell
        .invoke(
            terminal(),
            TERMINAL_CREATE_METHOD,
            agent_create("ghost-main", "act-ghost-0001", 1, "ws-a"),
        )
        .expect_err("no broker grant for ghost");
    assert_eq!(error.code, ErrorCode::Unauthorized);

    // 3b) Negotiated agent facts → create succeeds as agent_automation.
    let report = agent
        .invoke(
            terminal(),
            TERMINAL_CREATE_METHOD,
            agent_create(&agent_id, &activation_id, 1, "ws-a"),
        )
        .expect("authorized agent create");
    let session_id = report["sessionId"].as_str().expect("sessionId").to_string();
    assert_eq!(report["mode"], "agent_automation");

    // 3c) The agent mutates its session through the broker gate.
    agent
        .invoke(
            terminal(),
            TERMINAL_WRITE_METHOD,
            write_request(&session_id, "echo agent-gate-ok\r\n"),
        )
        .expect("agent write through the gate");
    let saw = agent.wait_for_output("agent-gate-ok", Duration::from_secs(10));
    assert!(saw.contains("agent-gate-ok"), "output: {saw:?}");

    // 3d) Another connection cannot mutate the agent session (ownership).
    let error = shell
        .invoke(
            terminal(),
            TERMINAL_WRITE_METHOD,
            write_request(&session_id, "echo nope\r\n"),
        )
        .expect_err("foreign connection cannot mutate");
    assert_eq!(error.code, ErrorCode::NotProcessOwner);

    // 3e) Renegotiation supersedes the agent activation (ADR-0018
    //     decision 1): the recorded binding (generation 1) now fails the
    //     gate with STALE_GENERATION.
    let second = agent.negotiate(vec![terminal()]);
    assert_ne!(second.activation_id, activation_id);
    let error = agent
        .invoke(
            terminal(),
            TERMINAL_WRITE_METHOD,
            write_request(&session_id, "echo stale\r\n"),
        )
        .expect_err("superseded activation fails the gate");
    assert_eq!(error.code, ErrorCode::StaleGeneration);

    // 3f) Human sessions never touch the broker: the Shell connection has
    //     no broker state of its own (its negotiation conflicted at the
    //     broker) yet its human create is authorized by the credential
    //     alone (broker-relaxed path, Shell-only).
    let human = shell
        .invoke(terminal(), TERMINAL_CREATE_METHOD, human_create(80, 24))
        .expect("human create without broker state");
    assert_eq!(human["mode"], "human_surface");
    let human_session = human["sessionId"].as_str().expect("sessionId").to_string();
    shell
        .invoke(
            terminal(),
            TERMINAL_CLOSE_METHOD,
            close_request(&human_session),
        )
        .expect("human close");

    // The orphaned agent session stays alive (the daemon never kills
    // resources on revocation — human takeover semantics, M5 parity).
    let status = shell
        .invoke(terminal(), TERMINAL_STATUS_METHOD, serde_json::json!({}))
        .expect("terminal.status succeeds");
    assert_eq!(status["count"], 1);
    assert_eq!(status["sessions"][0]["mode"], "agent_automation");
}

/// 4) Event routing isolates sessions: an agent connection (broker-
///    backed, agent_automation) and the Shell connection (broker-relaxed,
///    human_surface) each create their own session and receive only their
///    own output (sessionId-scoped; connection-scoped ownership).
#[test]
fn event_routing_isolates_sessions() {
    let (addr, credential, server) = spawn_daemon();
    let second_credential = server.issue_credential(Duration::from_secs(300));

    let mut first = TestClient::connect_as(addr, &credential, "dsh-agent", "main");
    let mut second = TestClient::connect_as(addr, &second_credential, "dsh-desktop-shell", "shell");
    let first_agreement = first.negotiate(vec![terminal()]);
    let second_agreement = second.negotiate(vec![terminal()]);
    assert!(first_agreement.granted.contains(&terminal()));
    assert!(second_agreement.granted.contains(&terminal()));

    // The agent session carries its broker facts; the Shell session is a
    // plain human surface.
    let first_report = first
        .invoke(
            terminal(),
            TERMINAL_CREATE_METHOD,
            agent_create("dsh-agent-main", &first_agreement.activation_id, 1, "ws-a"),
        )
        .expect("first pty");
    let first_session = first_report["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    assert_eq!(first_report["mode"], "agent_automation");
    let second_report = second
        .invoke(terminal(), TERMINAL_CREATE_METHOD, human_create(80, 24))
        .expect("second pty");
    let second_session = second_report["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();
    assert_eq!(second_report["mode"], "human_surface");
    assert_ne!(first_session, second_session);

    first
        .invoke(
            terminal(),
            TERMINAL_WRITE_METHOD,
            write_request(&first_session, "echo marker-first\r\n"),
        )
        .expect("first write");
    second
        .invoke(
            terminal(),
            TERMINAL_WRITE_METHOD,
            write_request(&second_session, "echo marker-second\r\n"),
        )
        .expect("second write");

    let first_saw = first.wait_for_output("marker-first", Duration::from_secs(10));
    assert!(first_saw.contains("marker-first"), "output: {first_saw:?}");
    // Give a broken cross-routing a chance to show up, then assert it did
    // not.
    let first_extra = first.wait_for_output("__never__", Duration::from_millis(500));
    let first_all = format!("{first_saw}{first_extra}");
    assert!(
        !first_all.contains("marker-second"),
        "first leaked second output"
    );

    let second_saw = second.wait_for_output("marker-second", Duration::from_secs(10));
    assert!(
        second_saw.contains("marker-second"),
        "output: {second_saw:?}"
    );
    let second_extra = second.wait_for_output("__never__", Duration::from_millis(500));
    let second_all = format!("{second_saw}{second_extra}");
    assert!(
        !second_all.contains("marker-first"),
        "second leaked first output"
    );

    // Every event a client saw carries its own session id.
    assert!(first.events().iter().all(|event| {
        event
            .payload
            .as_ref()
            .and_then(|p| p.get("sessionId"))
            .and_then(|s| s.as_str())
            == Some(first_session.as_str())
    }));
    assert!(second.events().iter().all(|event| {
        event
            .payload
            .as_ref()
            .and_then(|p| p.get("sessionId"))
            .and_then(|s| s.as_str())
            == Some(second_session.as_str())
    }));

    // Cross-connection mutation is rejected (session ownership).
    let error = second
        .invoke(
            terminal(),
            TERMINAL_CLOSE_METHOD,
            close_request(&first_session),
        )
        .expect_err("second cannot close the first session");
    assert_eq!(error.code, ErrorCode::NotProcessOwner);

    first
        .invoke(
            terminal(),
            TERMINAL_CLOSE_METHOD,
            close_request(&first_session),
        )
        .expect("first closes its own");
    second
        .invoke(
            terminal(),
            TERMINAL_CLOSE_METHOD,
            close_request(&second_session),
        )
        .expect("second closes its own");
}

/// 5) A connection without the terminal grant in its Agreement cannot use
///    the capability at all (protocol-level fail-closed).
#[test]
fn terminal_create_without_grant_is_unauthorized() {
    let (addr, credential, _server) = spawn_daemon();
    let mut client = TestClient::connect(addr, &credential);
    client.negotiate(vec![]);
    let error = client
        .invoke(terminal(), TERMINAL_CREATE_METHOD, human_create(80, 24))
        .expect_err("terminal not granted");
    assert_eq!(error.code, ErrorCode::Unauthorized);
}
