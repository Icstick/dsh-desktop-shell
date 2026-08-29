//! Wire-format handshake messages.
//!
//! The client sends exactly one framed `ClientHello` as its first frame;
//! the server answers with exactly one framed `ServerHello`.

use serde::{Deserialize, Serialize};

/// Client handshake frame (AC-IPC-001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientHello {
    /// The ephemeral credential token issued by the server.
    pub token: String,
}

/// Server handshake reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerHello {
    /// Whether the handshake was accepted.
    pub accepted: bool,
    /// Rejection reason when `accepted` is `false` (absent when accepted):
    /// `invalid` | `replay` | `stale` | `malformed` | `busy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
