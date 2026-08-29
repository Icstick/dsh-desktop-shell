//! Integration tests: deadlines for slow clients and the concurrency cap
//! (AC-IPC-002).

mod common;

use std::time::Duration;

use common::*;
use dsh_local_transport::*;

#[test]
fn slow_handshake_disconnected() {
    let limits = Limits {
        handshake_deadline: Duration::from_millis(150),
        ..test_limits()
    };
    let server = LocalServer::bind(limits).expect("bind");

    // connect but never send the hello
    let _raw = raw_connect(server.addr());
    wait_until("slow handshake cleaned", || {
        server.stats().active == 0 && server.stats().closed_timeout >= 1
    });
    assert_eq!(server.stats().closed_timeout, 1);

    // the server still accepts and serves new clients
    let (mut client, _) = connect(&server, &limits);
    client
        .send(b"alive")
        .expect("server serves after slow handshake");
    let conn = single_conn(&server);
    assert_eq!(
        conn.recv_timeout(Duration::from_secs(2)),
        Some(b"alive".to_vec())
    );
}

#[test]
fn slow_client_not_reading_disconnected() {
    let limits = Limits {
        write_deadline: Duration::from_millis(150),
        read_deadline: Duration::from_secs(5),
        ..test_limits()
    };
    let server = LocalServer::bind(limits).expect("bind");

    let credential = server.issue_credential(Duration::from_secs(60));
    let _client =
        LocalClient::connect(server.addr(), &credential, &limits).expect("client connect");
    let conn = single_conn(&server);

    // the client never reads; keep sending until the server write
    // deadline fires and the connection is cleaned up
    let big = vec![0u8; limits.max_frame_bytes];
    let outcome = loop {
        match conn.send(&big) {
            Ok(()) => {}
            Err(e) => break e,
        }
    };
    assert!(
        matches!(outcome, TransportError::Io(_) | TransportError::Closed),
        "send must fail once the write deadline fires: {outcome}"
    );

    wait_until("slow reader cleaned", || server.stats().active == 0);

    // the server still serves new clients
    let (mut client2, _) = connect(&server, &limits);
    client2
        .send(b"alive")
        .expect("server serves after slow reader");
    let conn2 = single_conn(&server);
    assert_eq!(
        conn2.recv_timeout(Duration::from_secs(2)),
        Some(b"alive".to_vec())
    );
}

#[test]
fn concurrency_limit_rejects_excess_and_releases_slots() {
    let limits = Limits {
        max_connections: 2,
        ..test_limits()
    };
    let server = LocalServer::bind(limits).expect("bind");
    let (mut c1, _) = connect(&server, &limits);
    let (c2, _) = connect(&server, &limits);
    assert_eq!(server.stats().active, 2);

    // a third connection is refused while both slots are taken
    let extra = server.issue_credential(Duration::from_secs(60));
    let err = LocalClient::connect(server.addr(), &extra, &limits)
        .expect_err("over-limit connection rejected");
    assert!(matches!(err, TransportError::Busy));
    let stats = server.stats();
    assert_eq!(stats.rejected_busy, 1);
    assert_eq!(stats.active, 2);

    c1.send(b"c1").expect("c1 works");
    drop(c2);
    wait_until("slot released", || server.stats().active == 1);

    // a new connection now fits into the freed slot
    let (mut c3, _) = connect(&server, &limits);
    c3.send(b"c3").expect("c3 works");
    assert_eq!(server.stats().active, 2);
}
