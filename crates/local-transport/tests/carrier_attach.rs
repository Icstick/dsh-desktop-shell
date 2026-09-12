//! Attached-carrier semantics (ADR-0022): a secondary carrier shares the
//! primary server's supervision state - one credential registry, one
//! connection table, one stats block. Exercised over the Windows named pipe
//! carrier, the peer-identity transport the daemon attaches next to TCP.

#![cfg(windows)]

use std::io::Write;
use std::time::{Duration, Instant};

use dsh_local_transport::carrier::PeerDesc;
use dsh_local_transport::framing::{encode_frame, read_frame};
use dsh_local_transport::handshake::ClientHello;
use dsh_local_transport::{Limits, LocalServer};

fn wait_for<T>(label: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn attached_pipe_carrier_shares_credentials_and_connections() {
    let name = format!("dsh-lt-attach-{}-{}", std::process::id(), 1);
    let limits = Limits::default();

    // Primary TCP server + attached named pipe carrier on the same state.
    let mut server = LocalServer::bind(limits).expect("bind tcp server");
    let listener = dsh_local_transport::named_pipe::NamedPipeListener::bind(&name, &limits)
        .expect("bind pipe carrier");
    server.attach_carrier(listener);

    // One credential issued on the primary server must authenticate a client
    // arriving over the attached carrier (shared registry).
    let credential = server.issue_credential(Duration::from_secs(300));
    let token = credential.token().to_string();

    let client = std::thread::spawn({
        let name = name.clone();
        let token = token.clone();
        move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match dsh_local_transport::named_pipe::connect(&name) {
                    Ok(stream) => break stream,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => panic!("pipe connect failed: {error}"),
                }
            };
            let hello = serde_json::to_vec(&ClientHello { token }).expect("hello json");
            stream
                .write_all(&encode_frame(&hello))
                .expect("hello write");
            let reply = read_frame(&mut stream, 64 * 1024)
                .expect("reply read")
                .expect("reply frame");
            let accepted = String::from_utf8_lossy(&reply).contains("\"accepted\":true");
            assert!(accepted, "shared credential must be accepted: {reply:?}");
            stream
        }
    });

    // The connection appears in the shared table with a pipe peer descriptor
    // and a kernel-provided identity - the property the daemon needs.
    let (conn_id, peer_desc, identity_present) = wait_for("pipe connection", || {
        server
            .connections()
            .first()
            .map(|conn| (conn.id(), conn.peer().clone(), conn.identity().is_some()))
    });
    assert!(
        matches!(peer_desc, PeerDesc::NamedPipe(_)),
        "pipe peer desc"
    );
    assert!(identity_present, "kernel peer identity on the pipe carrier");

    // Framing round trip through the supervision layer (client -> server).
    let mut client_stream = client.join().expect("client thread");
    let payload = encode_frame(b"through-the-attached-carrier");
    client_stream.write_all(&payload).expect("payload write");
    let received = wait_for("frame on the connection", || {
        server
            .connections()
            .iter()
            .find(|conn| conn.id() == conn_id)
            .and_then(|conn| conn.recv_timeout(Duration::from_millis(50)))
    });
    assert_eq!(received, b"through-the-attached-carrier");
    assert_eq!(server.stats().authenticated, 1, "shared stats block");
}
