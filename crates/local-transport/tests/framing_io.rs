//! Integration tests: malformed / oversized frames over the wire and the
//! frame-size boundary (AC-IPC-002).

mod common;

use std::io::Write;
use std::time::Duration;

use common::*;
use dsh_local_transport::*;

#[test]
fn bad_length_prefix_disconnects() {
    let (server, limits) = start_server();
    let mut raw = raw_connect(server.addr());
    raw.write_all(&0xFFFF_FFFFu32.to_le_bytes())
        .expect("write prefix");
    wait_until("bad-prefix client cleaned up", || {
        server.stats().active == 0 && server.stats().closed_protocol == 1
    });

    let (mut client, _) = connect(&server, &limits);
    client.send(b"still alive").expect("server still serves");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"still alive".to_vec())
    );
}

#[test]
fn truncated_frame_disconnects() {
    let (server, limits) = start_server();
    let mut raw = raw_connect(server.addr());
    // claim 1000 bytes, send 10, then close the socket
    raw.write_all(&1000u32.to_le_bytes()).expect("write prefix");
    raw.write_all(&[0u8; 10]).expect("write partial payload");
    drop(raw);
    wait_until("truncated client cleaned up", || {
        server.stats().active == 0 && server.stats().closed_protocol == 1
    });

    let (mut client, _) = connect(&server, &limits);
    client.send(b"ok").expect("server still serves");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"ok".to_vec())
    );
}

#[test]
fn oversized_frame_before_handshake_disconnects() {
    let (server, limits) = start_server();
    let mut raw = raw_connect(server.addr());
    // the length prefix alone exceeds the limit; no payload is sent
    let over = limits.max_frame_bytes as u64 + 1;
    raw.write_all(&over.to_le_bytes())
        .expect("write oversized prefix");
    wait_until("oversized client cleaned up", || {
        server.stats().active == 0 && server.stats().closed_protocol == 1
    });

    let (mut client, _) = connect(&server, &limits);
    client.send(b"still alive").expect("server still serves");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"still alive".to_vec())
    );
}

#[test]
fn oversized_frame_after_handshake_disconnects() {
    let (server, limits) = start_server();
    let credential = server.issue_credential(Duration::from_secs(60));
    let mut raw = raw_connect(server.addr());

    // manual handshake over a raw socket
    let hello = serde_json::json!({ "token": credential.token() });
    raw_send_frame(&mut raw, &serde_json::to_vec(&hello).expect("hello json"));
    let ack = raw_read_frame(&mut raw, Duration::from_secs(2)).expect("handshake ack");
    let ack: serde_json::Value = serde_json::from_slice(&ack).expect("ack json");
    assert_eq!(ack["accepted"], true);

    // authenticated connection now sends an oversized frame
    let over = limits.max_frame_bytes as u64 + 1;
    raw.write_all(&over.to_le_bytes())
        .expect("write oversized prefix");
    wait_until("oversized frame cleaned up", || {
        server.stats().active == 0 && server.stats().closed_protocol == 1
    });

    let (mut client, _) = connect(&server, &limits);
    client.send(b"survived").expect("server still serves");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"survived".to_vec())
    );
}

#[test]
fn exact_max_frame_passes_over_wire() {
    let (server, limits) = start_server();
    let (mut client, _) = connect(&server, &limits);
    let max = limits.max_frame_bytes;

    let payload = vec![0xABu8; max];
    client.send(&payload).expect("exactly-max frame accepted");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(payload.clone())
    );

    // server -> client at exactly max
    conn.send(&payload).expect("server max frame");
    assert_eq!(
        client
            .recv_timeout(Duration::from_secs(2))
            .expect("client recv"),
        Some(payload)
    );
}

#[test]
fn client_send_rejects_over_max_locally() {
    let (server, limits) = start_server();
    let (mut client, _) = connect(&server, &limits);
    let max = limits.max_frame_bytes;

    let over = vec![0u8; max + 1];
    let err = client
        .send(&over)
        .expect_err("over-max client send rejected");
    assert!(matches!(
        err,
        TransportError::Oversized { declared, max: m }
            if declared == (max + 1) as u64 && m == max
    ));

    // the client stays usable after the rejected send
    client
        .send(b"ok")
        .expect("client usable after oversized reject");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"ok".to_vec())
    );
}
