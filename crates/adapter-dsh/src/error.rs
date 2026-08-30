//! Adapter error model (fail-closed: every failure path is explicit).
//!
//! The adapter degrades to the L0 baseline (ADR-0018 decision 4): callers
//! observe an AdapterError and keep the DSH HTTP Web UI untouched - no
//! adapter failure panics or blocks lower layers.

use std::fmt;

/// All adapter failure modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    /// DSH answered an HTTP call with a non-2xx status.
    Http { status: u16, detail: String },
    /// DSH /api JSON-RPC answered with ok:false or a stream-level error.
    Rpc(String),
    /// Wire/protocol-level failure (malformed frames, bad JSON shapes).
    Protocol(String),
    /// Authentication failure (missing/unknown launch cookie, bad redirect).
    Auth(String),
    /// Transport-level failure (connect/io).
    Transport(String),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Http { status, detail } => {
                write!(f, "dsh http error {status}: {detail}")
            }
            AdapterError::Rpc(detail) => write!(f, "dsh rpc error: {detail}"),
            AdapterError::Protocol(detail) => write!(f, "dsh protocol error: {detail}"),
            AdapterError::Auth(detail) => write!(f, "dsh auth error: {detail}"),
            AdapterError::Transport(detail) => write!(f, "dsh transport error: {detail}"),
        }
    }
}

impl std::error::Error for AdapterError {}
