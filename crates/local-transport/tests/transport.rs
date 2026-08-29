//! Integration tests: handshake, bidirectional messages, credential
//! rejection (invalid / replay / stale / malformed) and cleanup.

mod common;

use std::time::{Duration, SystemTime};

use common::*;
use dsh_local_transport::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Msg {
    seq: u32,
    text: String,
}

#[test]
fn handshake_and_bidirectional_roundtrip() {
    let (server, limits) = start_server();
    let (mut client, _credential) = connect(&server, &limits);

    assert_eq!(server.stats().active, 1);
    assert_eq!(server.stats().authenticated, 1);
    assert_eq!(server.stats().credentials_consumed, 1);

    let conn = single_conn(&server);
    assert_eq!(conn.peer(), client.local_addr().expect("client local addr"));

    // client -> server (binary)
    let payload = b"hello from client".to_vec();
    client.send(&payload).expect("client send");
    assert_eq!(conn.recv_timeout(Duration::from_secs(2)), Some(payload));

    // server -> client (binary)
    conn.send(b"hello from server").expect("server send");
    assert_eq!(
        client
            .recv_timeout(Duration::from_secs(2))
            .expect("client recv"),
        Some(b"hello from server".to_vec())
    );

    // JSON roundtrip, several messages in each direction
    for i in 0..5u32 {
        let msg = Msg {
            seq: i,
            text: format!("c2s-{i}"),
        };
        client.send_json(&msg).expect("client send_json");
        assert_eq!(
            conn.recv_json_timeout::<Msg>(Duration::from_secs(2))
                .expect("server recv_json"),
            Some(msg)
        );
    }
    for i in 0..5u32 {
        let msg = Msg {
            seq: i,
            text: format!("s2c-{i}"),
        };
        conn.send_json(&msg).expect("server send_json");
        assert_eq!(
            client.recv_json::<Msg>().expect("client recv_json"),
            Some(msg)
        );
    }
}

#[test]
fn invalid_credential_rejected() {
    let (server, limits) = start_server();
    let forged = Credential::new(
        "lt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        SystemTime::now() + Duration::from_secs(60),
    );
    let err = LocalClient::connect(server.addr(), &forged, &limits)
        .expect_err("forged credential must be rejected");
    assert!(matches!(err, TransportError::Auth(AuthError::Invalid)));

    let stats = server.stats();
    assert_eq!(stats.rejected_auth, 1);
    assert_eq!(stats.authenticated, 0);
    assert_eq!(stats.active, 0);

    // the server still serves legitimate clients
    let (mut client, _) = connect(&server, &limits);
    client.send(b"still alive").expect("send after rejection");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"still alive".to_vec())
    );
}

#[test]
fn replay_credential_rejected() {
    let (server, limits) = start_server();
    let credential = server.issue_credential(Duration::from_secs(60));

    let mut first =
        LocalClient::connect(server.addr(), &credential, &limits).expect("first use succeeds");
    first.send(b"one").expect("first send");
    drop(first);
    wait_until("first connection cleaned up", || server.stats().active == 0);

    let err = LocalClient::connect(server.addr(), &credential, &limits)
        .expect_err("replay must be rejected");
    assert!(matches!(err, TransportError::Auth(AuthError::Replay)));

    let stats = server.stats();
    assert_eq!(stats.credentials_consumed, 1);
    assert_eq!(stats.rejected_auth, 1);

    // a fresh credential still works
    let (mut client, _) = connect(&server, &limits);
    client.send(b"fresh").expect("fresh credential works");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"fresh".to_vec())
    );
}

#[test]
fn stale_credential_rejected() {
    let (server, limits) = start_server();

    // TTL zero => expired at issue time
    let already_expired = server.issue_credential(Duration::ZERO);
    let err = LocalClient::connect(server.addr(), &already_expired, &limits)
        .expect_err("expired credential must be rejected");
    assert!(matches!(err, TransportError::Auth(AuthError::Stale)));

    // short TTL + sleep => becomes stale before use
    let short_ttl = server.issue_credential(Duration::from_millis(20));
    std::thread::sleep(Duration::from_millis(80));
    let err = LocalClient::connect(server.addr(), &short_ttl, &limits)
        .expect_err("short-ttl credential must be stale");
    assert!(matches!(err, TransportError::Auth(AuthError::Stale)));

    // an expired credential is removed from the registry: retry => invalid
    let err = LocalClient::connect(server.addr(), &short_ttl, &limits)
        .expect_err("removed credential must be unknown");
    assert!(matches!(err, TransportError::Auth(AuthError::Invalid)));

    assert_eq!(server.stats().rejected_auth, 3);
}

#[test]
fn malformed_handshake_rejected() {
    let (server, limits) = start_server();

    // non-JSON handshake frame
    let mut raw = raw_connect(server.addr());
    raw_send_frame(&mut raw, b"this is not json");
    let ack =
        raw_read_frame(&mut raw, Duration::from_secs(2)).expect("server replies to handshake");
    let ack: serde_json::Value = serde_json::from_slice(&ack).expect("ack is json");
    assert_eq!(ack["accepted"], false);
    assert_eq!(ack["reason"], "malformed");

    // valid JSON but not a valid hello (unknown fields are denied)
    let mut raw2 = raw_connect(server.addr());
    raw_send_frame(&mut raw2, br#"{"hello": 1}"#);
    let ack =
        raw_read_frame(&mut raw2, Duration::from_secs(2)).expect("server replies to handshake");
    let ack: serde_json::Value = serde_json::from_slice(&ack).expect("ack is json");
    assert_eq!(ack["accepted"], false);
    assert_eq!(ack["reason"], "malformed");

    assert_eq!(server.stats().rejected_auth, 2);

    // the server still serves legitimate clients
    let (mut client, _) = connect(&server, &limits);
    client.send(b"ok").expect("send after malformed handshake");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"ok".to_vec())
    );
}

#[test]
fn disconnect_cleanup_allows_new_connection() {
    let (server, limits) = start_server();
    let (client, _credential) = connect(&server, &limits);
    assert_eq!(server.stats().active, 1);
    assert!(!server.connections().is_empty());

    drop(client);
    wait_until("server observes disconnect", || {
        server.stats().active == 0 && server.connections().is_empty()
    });
    assert_eq!(server.stats().closed_peer, 1);

    // new credential + new connection works after cleanup
    let (mut second, _) = connect(&server, &limits);
    second.send(b"again").expect("second connection works");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"again".to_vec())
    );
    assert_eq!(server.stats().authenticated, 2);
}

#[test]
fn server_stop_is_idempotent_and_drop_is_safe() {
    let (server, limits) = start_server();
    let (mut client, _) = connect(&server, &limits);
    client.send(b"before stop").expect("send");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"before stop".to_vec())
    );

    let mut server = server;
    server.stop();
    server.stop(); // idempotent
    drop(client);
}
