//! Carrier abstraction (ADR-0007 / ADR-0022): the supervised server core
//! (framing, handshake, credentials, deadlines, limits) is carrier-agnostic.
//! Loopback TCP is the historical fallback carrier; Windows Named Pipes and
//! Unix domain sockets are the peer-identity carriers - they let the server
//! ask the kernel *which process* connected (see [crate::peer]).
//!
//! A carrier yields byte streams with socket-style deadlines. Implementations
//! that cannot express OS-level read timeouts (named pipes) apply the
//! deadline inside their own read (poll + would-block), which the
//! supervision loop already treats as a deadline expiry
//! (is_timeout_kind: WouldBlock | TimedOut).

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

/// Wire-independent description of where a connection came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerDesc {
    /// Loopback TCP peer address.
    Tcp(SocketAddr),
    /// Windows named pipe endpoint (the pipe name).
    NamedPipe(String),
    /// Unix domain socket path.
    UnixSocket(String),
}

impl fmt::Display for PeerDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp(addr) => write!(f, "tcp:{addr}"),
            Self::NamedPipe(name) => write!(f, "pipe:{name}"),
            Self::UnixSocket(path) => write!(f, "uds:{path}"),
        }
    }
}

impl PeerDesc {
    /// The TCP address when this is a TCP peer (tests/diagnostics).
    pub fn tcp_addr(&self) -> Option<SocketAddr> {
        match self {
            Self::Tcp(addr) => Some(*addr),
            _ => None,
        }
    }
}

/// A bidirectional stream the supervision layer can drive.
pub trait CarrierStream: Read + Write + Send + 'static {
    /// Restore blocking semantics for the accepted stream (the accept loop
    /// polls a non-blocking listener, and accepted TCP sockets inherit that
    /// flag; carriers without the flag are a no-op).
    fn prepare_accepted(&mut self) -> io::Result<()>;

    /// Set the read deadline; None clears it. Implementations without an
    /// OS-level timeout apply it inside read (see the module docs).
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;

    /// Set the write deadline; None clears it.
    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
}

impl CarrierStream for TcpStream {
    fn prepare_accepted(&mut self) -> io::Result<()> {
        // Accepted sockets inherit the listener's non-blocking mode (set for
        // the accept poll loop). Restore blocking I/O so the handshake and
        // worker loops can rely on their deadlines via SO_RCVTIMEO/SO_SNDTIMEO:
        // on a non-blocking socket a partial read followed by WouldBlock
        // discards already-consumed frame bytes and desyncs the stream.
        self.set_nonblocking(false)
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        TcpStream::set_read_timeout(self, timeout)
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        TcpStream::set_write_timeout(self, timeout)
    }
}

#[cfg(unix)]
impl CarrierStream for std::os::unix::net::UnixStream {
    fn prepare_accepted(&mut self) -> io::Result<()> {
        self.set_nonblocking(false)
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        std::os::unix::net::UnixStream::set_read_timeout(self, timeout)
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        std::os::unix::net::UnixStream::set_write_timeout(self, timeout)
    }
}

/// A carrier listener the accept loop can poll (non-blocking accept +
/// small-sleep polling, mirroring the TCP path).
pub trait CarrierListener {
    type Stream: CarrierStream;

    /// Put the listener into non-blocking accept mode.
    fn set_nonblocking(&self) -> io::Result<()>;

    /// Try to accept one connection (WouldBlock when none is pending).
    fn accept(&self) -> io::Result<(Self::Stream, PeerDesc)>;

    /// Kernel-provided identity of the peer behind the stream, when the
    /// carrier can provide one (named pipes / UDS); TCP returns None
    /// and the caller must treat the connection as identity-less.
    fn peer_identity(&self, stream: &Self::Stream)
    -> io::Result<Option<crate::peer::PeerIdentity>>;

    /// Local endpoint description (diagnostics).
    fn local_desc(&self) -> String;
}

impl CarrierListener for TcpListener {
    type Stream = TcpStream;

    fn set_nonblocking(&self) -> io::Result<()> {
        TcpListener::set_nonblocking(self, true)
    }

    fn accept(&self) -> io::Result<(TcpStream, PeerDesc)> {
        let (stream, peer) = TcpListener::accept(self)?;
        Ok((stream, PeerDesc::Tcp(peer)))
    }

    fn peer_identity(&self, _stream: &TcpStream) -> io::Result<Option<crate::peer::PeerIdentity>> {
        // Loopback TCP exposes no kernel-provided peer identity; such
        // connections are identity-less by design (ADR-0022: TCP stays an
        // explicit, reported degradation - it must never earn shell_control).
        Ok(None)
    }

    fn local_desc(&self) -> String {
        self.local_addr()
            .map(|addr| format!("tcp:{addr}"))
            .unwrap_or_else(|_| "tcp:?".to_string())
    }
}
/// A client-side carrier stream chosen at connect time (ADR-0022): the
/// Shell keeps one [LocalClient](crate::LocalClient) type regardless of
/// which carrier it used.
#[derive(Debug)]
pub enum ClientStream {
    /// Loopback TCP stream (degradation carrier; identity-less).
    Tcp(TcpStream),
    /// Windows named pipe stream (peer-identity carrier).
    #[cfg(windows)]
    NamedPipe(crate::named_pipe::PipeStream),
    /// Unix domain socket stream (peer-identity carrier).
    #[cfg(unix)]
    UnixSocket(std::os::unix::net::UnixStream),
}

impl Read for ClientStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.read(buf),
            #[cfg(windows)]
            Self::NamedPipe(stream) => stream.read(buf),
            #[cfg(unix)]
            Self::UnixSocket(stream) => stream.read(buf),
        }
    }
}

impl Write for ClientStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.write(buf),
            #[cfg(windows)]
            Self::NamedPipe(stream) => stream.write(buf),
            #[cfg(unix)]
            Self::UnixSocket(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.flush(),
            #[cfg(windows)]
            Self::NamedPipe(stream) => stream.flush(),
            #[cfg(unix)]
            Self::UnixSocket(stream) => stream.flush(),
        }
    }
}

impl CarrierStream for ClientStream {
    fn prepare_accepted(&mut self) -> io::Result<()> {
        // Client-side streams are connected by construction.
        Ok(())
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => CarrierStream::set_read_timeout(stream, timeout),
            #[cfg(windows)]
            Self::NamedPipe(stream) => CarrierStream::set_read_timeout(stream, timeout),
            #[cfg(unix)]
            Self::UnixSocket(stream) => CarrierStream::set_read_timeout(stream, timeout),
        }
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => CarrierStream::set_write_timeout(stream, timeout),
            #[cfg(windows)]
            Self::NamedPipe(stream) => CarrierStream::set_write_timeout(stream, timeout),
            #[cfg(unix)]
            Self::UnixSocket(stream) => CarrierStream::set_write_timeout(stream, timeout),
        }
    }
}
