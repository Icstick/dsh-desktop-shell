//! Transport limits (AC-IPC-002).

use std::time::Duration;

use crate::framing::MAX_FRAME_BYTES;

/// Frame, deadline and concurrency limits of a server/client pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum payload size per frame in bytes.
    pub max_frame_bytes: usize,
    /// How long a client may take to complete the handshake.
    pub handshake_deadline: Duration,
    /// Idle deadline between frames: a client that sends nothing for this
    /// long is disconnected (slow client, AC-IPC-002).
    pub read_deadline: Duration,
    /// How long a server-side write may block before the connection is
    /// disconnected (client that does not drain data).
    pub write_deadline: Duration,
    /// Maximum number of simultaneous authenticated connections.
    pub max_connections: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame_bytes: MAX_FRAME_BYTES,
            handshake_deadline: Duration::from_secs(10),
            read_deadline: Duration::from_secs(30),
            write_deadline: Duration::from_secs(30),
            max_connections: 8,
        }
    }
}
