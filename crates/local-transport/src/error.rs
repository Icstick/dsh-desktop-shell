//! Transport error model.

use std::fmt;
use std::io;

use crate::credential::AuthError;
use crate::framing::{FrameError, FrameReadError};

/// Errors raised by the local transport.
#[derive(Debug)]
pub enum TransportError {
    /// Underlying socket error.
    Io(io::Error),
    /// Handshake rejected by the credential check (AC-IPC-001).
    Auth(AuthError),
    /// Server concurrency limit reached (AC-IPC-002).
    Busy,
    /// Connection closed before the operation completed.
    Closed,
    /// Frame payload above the configured limit (AC-IPC-002).
    Oversized { declared: u64, max: usize },
    /// Framing violation: bad length prefix, oversized or truncated frame.
    Protocol(FrameError),
    /// JSON serialization/deserialization error.
    Serialization(Box<serde_json::Error>),
}

impl TransportError {
    /// Whether this error is a read/write deadline expiry.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Io(e) if is_timeout_kind(e.kind()))
    }

    /// Map the wire-format handshake rejection reason to a typed error.
    pub(crate) fn from_reason(reason: Option<String>) -> Self {
        match reason.as_deref() {
            Some("invalid") => Self::Auth(AuthError::Invalid),
            Some("replay") => Self::Auth(AuthError::Replay),
            Some("stale") => Self::Auth(AuthError::Stale),
            Some("malformed") => Self::Auth(AuthError::Malformed),
            Some("busy") => Self::Busy,
            _ => Self::Auth(AuthError::Malformed),
        }
    }
}

/// Deadline expiries surface as `WouldBlock` or `TimedOut` depending on
/// platform; treat both as deadline violations.
pub(crate) fn is_timeout_kind(kind: io::ErrorKind) -> bool {
    matches!(kind, io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Auth(AuthError::Invalid) => {
                write!(f, "authentication failed: invalid credential")
            }
            Self::Auth(AuthError::Replay) => write!(f, "authentication failed: credential replay"),
            Self::Auth(AuthError::Stale) => write!(f, "authentication failed: stale credential"),
            Self::Auth(AuthError::Malformed) => {
                write!(f, "authentication failed: malformed handshake")
            }
            Self::Busy => write!(f, "server connection limit reached"),
            Self::Closed => write!(f, "connection closed"),
            Self::Oversized { declared, max } => {
                write!(f, "frame of {declared} bytes exceeds limit of {max}")
            }
            Self::Protocol(FrameError::Oversized { declared, max }) => {
                write!(f, "protocol error: oversized frame ({declared} > {max})")
            }
            Self::Protocol(FrameError::Truncated { expected, got }) => {
                write!(
                    f,
                    "protocol error: truncated frame (expected {expected}, got {got})"
                )
            }
            Self::Serialization(e) => write!(f, "serialization error: {e}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serialization(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<io::Error> for TransportError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for TransportError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(Box::new(e))
    }
}

impl From<AuthError> for TransportError {
    fn from(e: AuthError) -> Self {
        Self::Auth(e)
    }
}

impl From<FrameError> for TransportError {
    fn from(e: FrameError) -> Self {
        Self::Protocol(e)
    }
}

impl From<FrameReadError> for TransportError {
    fn from(e: FrameReadError) -> Self {
        match e {
            FrameReadError::Io(e) => Self::Io(e),
            FrameReadError::Frame(frame) => Self::Protocol(frame),
        }
    }
}
