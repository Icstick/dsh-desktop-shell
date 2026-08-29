#![allow(dead_code)] // helpers are used selectively by each test binary

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use dsh_local_transport::{Credential, Limits, LocalClient, LocalServer, ServerConn, encode_frame};

/// Limits with short-but-comfortable deadlines for tests.
pub fn test_limits() -> Limits {
    Limits {
        handshake_deadline: Duration::from_secs(3),
        read_deadline: Duration::from_secs(3),
        write_deadline: Duration::from_secs(3),
        ..Limits::default()
    }
}

/// Bind a server with the standard test limits.
pub fn start_server() -> (LocalServer, Limits) {
    let limits = test_limits();
    let server = LocalServer::bind(limits).expect("bind loopback server");
    (server, limits)
}

/// Issue a fresh credential and complete a client handshake.
pub fn connect(server: &LocalServer, limits: &Limits) -> (LocalClient, Credential) {
    let credential = server.issue_credential(Duration::from_secs(60));
    let client = LocalClient::connect(server.addr(), &credential, limits)
        .unwrap_or_else(|e| panic!("client handshake failed: {e}"));
    (client, credential)
}

/// The single live server-side connection (tests use at most one at a time).
pub fn single_conn(server: &LocalServer) -> ServerConn {
    server
        .connections()
        .into_iter()
        .next()
        .expect("server has a live connection")
}

/// Poll `cond` until it holds or 5s elapse.
pub fn wait_until<F: Fn() -> bool>(label: &str, cond: F) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if cond() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for: {label}");
}

/// Raw TCP connect (bypasses the handshake client).
pub fn raw_connect(addr: SocketAddr) -> TcpStream {
    TcpStream::connect(addr).expect("raw tcp connect")
}

/// Send one framed payload over a raw stream.
pub fn raw_send_frame(stream: &mut TcpStream, payload: &[u8]) {
    stream
        .write_all(&encode_frame(payload))
        .expect("raw write frame");
}

/// Read one framed payload from a raw stream; `None` on timeout or close.
pub fn raw_read_frame(stream: &mut TcpStream, timeout: Duration) -> Option<Vec<u8>> {
    stream.set_read_timeout(Some(timeout)).ok()?;
    let mut prefix = [0u8; 4];
    if stream.read_exact(&mut prefix).is_err() {
        return None;
    }
    let len = u32::from_le_bytes(prefix) as usize;
    let mut payload = vec![0u8; len];
    if stream.read_exact(&mut payload).is_err() {
        return None;
    }
    Some(payload)
}
