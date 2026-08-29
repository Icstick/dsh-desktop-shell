//! # dsh-local-transport
//!
//! Authenticated, framed, supervised loopback transport for local IPC
//! (module `MOD-LOCAL-TRANSPORT`, milestone M2 slice M2-C).
//!
//! ## Scope
//!
//! - **Carrier**: loopback TCP on `127.0.0.1` with a random port. The
//!   framing codec works over any `std::io::Read + Write` stream, which is
//!   the extension point reserved for Named Pipe / UDS carriers (ADR-0007).
//! - **Owns**: endpoint lifecycle, ACL/mode, ephemeral credentials,
//!   framing/reconnect supervision.
//! - **Does not own**: capability semantics, plugin identity proof.
//!
//! ## Security model (AC-IPC-001 / AC-IPC-002)
//!
//! - Every server issues one-time ephemeral credentials with a TTL; the
//!   handshake rejects invalid, replayed and stale credentials, and a
//!   credential is consumed by its first successful use.
//! - Oversized frames (default limit `MAX_FRAME_BYTES`), slow clients
//!   (handshake/read/write deadlines) and connection floods (concurrency
//!   cap) are rejected or disconnected, and every exit path fully cleans
//!   up its resources.
//!
//! ## Example
//!
//! ```no_run
//! use std::time::Duration;
//! use dsh_local_transport::{Limits, LocalClient, LocalServer};
//!
//! # fn main() -> Result<(), dsh_local_transport::TransportError> {
//! let server = LocalServer::bind(Limits::default())?;
//! let credential = server.issue_credential(Duration::from_secs(300));
//! let mut client = LocalClient::connect(server.addr(), &credential, &Limits::default())?;
//! client.send(b"hello")?;
//! let conn = server.connections().into_iter().next().unwrap();
//! let reply = conn.recv_timeout(Duration::from_secs(1)).unwrap();
//! # let _ = reply;
//! # Ok(())
//! # }
//! ```
//!
//! ## Modules
//!
//! - `client`: handshaking client with send/recv helpers.
//! - `credential`: ephemeral one-time credentials and their validation.
//! - `error`: the transport error model.
//! - `framing`: u32-LE length-prefixed frame codec (carrier-agnostic).
//! - `handshake`: the wire-format handshake messages (serde JSON).
//! - `limits`: frame/deadline/concurrency limits.
//! - `server`: listener, accept loop, supervision and stats.

#![forbid(unsafe_code)]

pub mod client;
pub mod credential;
pub mod error;
pub mod framing;
pub mod handshake;
pub mod limits;
pub mod server;

pub use client::LocalClient;
pub use credential::{AuthError, Credential, CredentialIssuer};
pub use error::TransportError;
pub use framing::{
    FrameError, FrameReadError, MAX_FRAME_BYTES, encode_frame, encode_frame_checked, read_frame,
};
pub use handshake::{ClientHello, ServerHello};
pub use limits::Limits;
pub use server::{LocalServer, ServerConn, ServerStats};
