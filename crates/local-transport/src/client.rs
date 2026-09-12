//! Authenticated carrier client (ADR-0007 / ADR-0022).
//!
//! Generic over the carrier stream: the default (TcpStream) keeps the
//! historical API, while peer-identity carriers (Windows named pipe,
//! Unix domain socket) connect through [LocalClient::connect_stream] once
//! the endpoint is established by the caller.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::carrier::{CarrierStream, PeerDesc};
use crate::credential::Credential;
use crate::error::{TransportError, is_timeout_kind};
use crate::framing::{FrameReadError, encode_frame, encode_frame_checked, read_frame};
use crate::handshake::{ClientHello, ServerHello};
use crate::limits::Limits;

/// A client that authenticated with the server via a one-time credential.
#[derive(Debug)]
pub struct LocalClient<S = TcpStream> {
    stream: S,
    limits: Limits,
    peer: PeerDesc,
}

impl LocalClient<TcpStream> {
    /// Connect to a [LocalServer](crate::LocalServer) over loopback TCP and
    /// complete the credential handshake. Fails with
    /// [AuthError](crate::AuthError) for invalid/replay/stale credentials and
    /// with [TransportError::Busy] when the server concurrency limit is
    /// reached.
    pub fn connect(
        addr: SocketAddr,
        credential: &Credential,
        limits: &Limits,
    ) -> Result<Self, TransportError> {
        let stream = TcpStream::connect(addr)?;
        Self::connect_stream(stream, PeerDesc::Tcp(addr), credential, limits)
    }

    /// Server address this client connected to.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer
            .tcp_addr()
            .expect("a TCP client always carries a TCP peer descriptor")
    }

    /// Local address of the client socket.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.stream.local_addr()
    }
}

impl<S: CarrierStream> LocalClient<S> {
    /// Complete the credential handshake over an already-connected carrier
    /// stream (named pipe, UDS, or a pre-connected TCP socket).
    pub fn connect_stream(
        mut stream: S,
        peer: PeerDesc,
        credential: &Credential,
        limits: &Limits,
    ) -> Result<Self, TransportError> {
        stream.set_read_timeout(Some(limits.handshake_deadline))?;
        stream.set_write_timeout(Some(limits.handshake_deadline))?;

        let hello = ClientHello {
            token: credential.token().to_string(),
        };
        let payload = serde_json::to_vec(&hello)?;
        let frame = encode_frame_checked(&payload, limits.max_frame_bytes)?;
        stream.write_all(&frame)?;

        let ack = match read_frame(&mut stream, limits.max_frame_bytes)? {
            Some(bytes) => serde_json::from_slice::<ServerHello>(&bytes)?,
            None => return Err(TransportError::Closed),
        };
        if !ack.accepted {
            return Err(TransportError::from_reason(ack.reason));
        }

        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;
        Ok(Self {
            stream,
            limits: *limits,
            peer,
        })
    }

    /// Where this client connected (wire-independent description).
    pub fn peer(&self) -> &PeerDesc {
        &self.peer
    }

    /// Send one frame. Payloads above the frame limit are rejected locally
    /// with [TransportError::Oversized] (AC-IPC-002).
    pub fn send(&mut self, payload: &[u8]) -> Result<(), TransportError> {
        if payload.len() > self.limits.max_frame_bytes {
            return Err(TransportError::Oversized {
                declared: payload.len() as u64,
                max: self.limits.max_frame_bytes,
            });
        }
        let frame = encode_frame(payload);
        self.stream.write_all(&frame)?;
        Ok(())
    }

    /// Serialize and send one message.
    pub fn send_json<T: serde::Serialize>(&mut self, value: &T) -> Result<(), TransportError> {
        self.send(&serde_json::to_vec(value)?)
    }

    /// Block until one frame arrives; Ok(None) when the server closed.
    pub fn recv(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        match read_frame(&mut self.stream, self.limits.max_frame_bytes)? {
            Some(bytes) => Ok(Some(bytes)),
            None => Ok(None),
        }
    }

    /// Like [recv](Self::recv), but Ok(None) after timeout elapses.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
        self.stream.set_read_timeout(Some(timeout))?;
        let result = read_frame(&mut self.stream, self.limits.max_frame_bytes);
        let _ = self.stream.set_read_timeout(None);
        match result {
            Ok(Some(bytes)) => Ok(Some(bytes)),
            Ok(None) => Ok(None),
            Err(FrameReadError::Io(e)) if is_timeout_kind(e.kind()) => Ok(None),
            Err(other) => Err(other.into()),
        }
    }

    /// Block until one deserialized message arrives.
    pub fn recv_json<T: serde::de::DeserializeOwned>(
        &mut self,
    ) -> Result<Option<T>, TransportError> {
        match self.recv()? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }
}
