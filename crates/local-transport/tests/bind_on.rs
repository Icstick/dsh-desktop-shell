//! Fixed-endpoint bind (0.2.1 M6-C): LocalServer::bind_on binds an
//! explicit loopback port (the daemon envelope endpoint) while the
//! default bind stays random. Auth/framing semantics are identical;
//! these tests only pin the endpoint selection behavior.

mod common;

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::time::Duration;

use dsh_local_transport::{Limits, LocalClient, LocalServer};

/// Grab a free loopback port for one test (probe->bind window is the
/// standard ephemeral-port TOCTOU and is negligible here).
fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

#[test]
fn bind_on_fixed_port_serves_authenticated_client() {
    let port = free_port();
    let addr: SocketAddr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let server = LocalServer::bind_on(addr, Limits::default()).expect("binds fixed port");
    assert_eq!(
        server.addr().port(),
        port,
        "envelope binds the requested port"
    );

    let credential = server.issue_credential(Duration::from_secs(300));
    let mut client =
        LocalClient::connect(server.addr(), &credential, &Limits::default()).expect("connects");
    client.send(b"ping").expect("sends");
    let reply = server
        .connections()
        .into_iter()
        .next()
        .expect("authenticated connection")
        .recv_timeout(Duration::from_secs(1))
        .expect("frame arrives");
    assert_eq!(reply.as_slice(), b"ping");
}

#[test]
fn second_bind_on_same_port_is_addr_in_use() {
    let port = free_port();
    let addr: SocketAddr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let _first = LocalServer::bind_on(addr, Limits::default()).expect("first binds");
    let err = LocalServer::bind_on(addr, Limits::default()).expect_err("second must fail");
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
}

#[test]
fn port_zero_requests_os_assigned_port() {
    let addr: SocketAddr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0);
    let server = LocalServer::bind_on(addr, Limits::default()).expect("binds port 0");
    assert!(server.addr().port() > 0, "OS assigned a real port");
}
