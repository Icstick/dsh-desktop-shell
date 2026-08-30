//! WebSocket layer: pure RFC 6455 frame codec + transport trait.
//!
//! Production framing is handled by tungstenite (sync, no TLS feature - the
//! DSH surface is loopback ws:// only). The pure codec here is the normative
//! wire contract: unit tests exercise it with byte sequences, the in-memory
//! fake transport feeds frame bytes through it, and a raw byte-level
//! transport could reuse it later without touching the rest of the crate.

use crate::error::AdapterError;

/// Defensive cap for a single frame payload.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsFrame {
    pub fin: bool,
    pub opcode: Opcode,
    pub payload: Vec<u8>,
}

/// Decode exactly one WebSocket frame from the front of input (pure).
///
/// Both masked and unmasked frames are accepted (server to client frames are
/// unmasked, client to server masked; the decoder does not enforce the
/// direction so tests can drive either). Control frames must be final and
/// <= 125 bytes. Reserved bits, unknown opcodes and oversized payloads are
/// rejected - fail-closed.
pub fn decode_frame(input: &[u8]) -> Result<WsFrame, AdapterError> {
    if input.len() < 2 {
        return Err(AdapterError::Protocol(
            "ws: truncated frame header".to_string(),
        ));
    }
    let b0 = input[0];
    let b1 = input[1];
    if b0 & 0x70 != 0 {
        return Err(AdapterError::Protocol("ws: reserved bits set".to_string()));
    }
    let fin = b0 & 0x80 != 0;
    let opcode = match b0 & 0x0F {
        0x0 => Opcode::Continuation,
        0x1 => Opcode::Text,
        0x2 => Opcode::Binary,
        0x8 => Opcode::Close,
        0x9 => Opcode::Ping,
        0xA => Opcode::Pong,
        _ => return Err(AdapterError::Protocol("ws: unknown opcode".to_string())),
    };
    let masked = b1 & 0x80 != 0;
    let len7 = (b1 & 0x7F) as usize;
    let mut offset = 2;
    let payload_len = match len7 {
        126 => {
            if input.len() < offset + 2 {
                return Err(AdapterError::Protocol(
                    "ws: truncated extended length".to_string(),
                ));
            }
            let length = u16::from_be_bytes([input[offset], input[offset + 1]]) as usize;
            offset += 2;
            length
        }
        127 => {
            if input.len() < offset + 8 {
                return Err(AdapterError::Protocol(
                    "ws: truncated extended length".to_string(),
                ));
            }
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&input[offset..offset + 8]);
            let length = u64::from_be_bytes(bytes);
            let length = usize::try_from(length)
                .map_err(|_| AdapterError::Protocol("ws: payload length overflow".to_string()))?;
            offset += 8;
            length
        }
        length => length,
    };
    if payload_len > MAX_FRAME_BYTES {
        return Err(AdapterError::Protocol(
            "ws: payload exceeds cap".to_string(),
        ));
    }
    let mask_offset = if masked {
        if input.len() < offset + 4 {
            return Err(AdapterError::Protocol("ws: truncated mask key".to_string()));
        }
        let at = offset;
        offset += 4;
        at
    } else {
        0
    };
    if input.len() < offset + payload_len {
        return Err(AdapterError::Protocol("ws: truncated payload".to_string()));
    }
    let mut payload = input[offset..offset + payload_len].to_vec();
    if masked {
        let mask = &input[mask_offset..mask_offset + 4];
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    if matches!(opcode, Opcode::Close | Opcode::Ping | Opcode::Pong) && (!fin || payload_len > 125)
    {
        return Err(AdapterError::Protocol(
            "ws: invalid control frame".to_string(),
        ));
    }
    Ok(WsFrame {
        fin,
        opcode,
        payload,
    })
}

/// Mask key for client frames. A fixed key is wire-valid (masking is not a
/// security mechanism) and keeps the codec deterministic for tests.
const MASK_KEY: [u8; 4] = [0x12, 0x34, 0x56, 0x78];

/// Encode a server to client text frame (unmasked, final) - used by the
/// fake DSH and by tests.
pub fn encode_server_text_frame(text: &str) -> Vec<u8> {
    encode_frame(text.as_bytes(), Opcode::Text, false, None)
}

/// Encode a client to server text frame (masked, final).
pub fn encode_client_text_frame(text: &str) -> Vec<u8> {
    encode_frame(text.as_bytes(), Opcode::Text, true, Some(MASK_KEY))
}

/// Encode a close frame (server style, unmasked).
pub fn encode_close_frame(code: u16, reason: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(2 + reason.len());
    payload.extend_from_slice(&code.to_be_bytes());
    payload.extend_from_slice(reason.as_bytes());
    encode_frame(&payload, Opcode::Close, false, None)
}

fn encode_frame(
    payload: &[u8],
    opcode: Opcode,
    masked: bool,
    mask_key: Option<[u8; 4]>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | opcode as u8);
    let mask_bit = if masked { 0x80 } else { 0 };
    match payload.len() {
        0..=125 => out.push(mask_bit | payload.len() as u8),
        126..=0xFFFF => {
            out.push(mask_bit | 126);
            out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        _ => {
            out.push(mask_bit | 127);
            out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
    }
    let key = mask_key.unwrap_or(MASK_KEY);
    if masked {
        out.extend_from_slice(&key);
    }
    let payload_start = out.len();
    out.extend_from_slice(payload);
    if masked {
        for index in 0..payload.len() {
            out[payload_start + index] ^= key[index % 4];
        }
    }
    out
}

/// Transport-level WS message channel (text-oriented).
///
/// recv_text answers pings with pongs internally and skips binary frames, so
/// consumers only ever see text messages and stream end.
pub trait WsTransport {
    fn send_text(&mut self, text: &str) -> Result<(), AdapterError>;
    /// Next text message; Ok(None) means the peer closed the stream.
    fn recv_text(&mut self) -> Result<Option<String>, AdapterError>;
    fn close(&mut self) -> Result<(), AdapterError>;
}

/// Real transport over tungstenite (sync WebSocket over TcpStream).
pub struct TungsteniteTransport {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
}

impl TungsteniteTransport {
    /// Connect to a ws:// URL, attaching the launch cookie when present.
    pub fn connect(url: &str, cookie: Option<&str>) -> Result<Self, AdapterError> {
        use tungstenite::client::IntoClientRequest;
        use tungstenite::http::header::{COOKIE, HeaderValue};
        let mut request = url
            .into_client_request()
            .map_err(|error| AdapterError::Transport(format!("ws request: {error}")))?;
        if let Some(cookie) = cookie {
            request.headers_mut().insert(
                COOKIE,
                HeaderValue::from_str(cookie).map_err(|error| {
                    AdapterError::Auth(format!("invalid cookie header: {error}"))
                })?,
            );
        }
        let (socket, response) = tungstenite::connect(request)
            .map_err(|error| AdapterError::Transport(format!("ws connect: {error}")))?;
        if response.status() != 101 {
            return Err(AdapterError::Auth(format!(
                "ws upgrade rejected: {}",
                response.status()
            )));
        }
        Ok(Self { socket })
    }
}

impl WsTransport for TungsteniteTransport {
    fn send_text(&mut self, text: &str) -> Result<(), AdapterError> {
        self.socket
            .send(tungstenite::Message::Text(text.into()))
            .map_err(|error| AdapterError::Transport(format!("ws send: {error}")))
    }

    fn recv_text(&mut self) -> Result<Option<String>, AdapterError> {
        loop {
            match self
                .socket
                .read()
                .map_err(|error| AdapterError::Transport(format!("ws read: {error}")))?
            {
                tungstenite::Message::Text(text) => return Ok(Some(text.to_string())),
                tungstenite::Message::Binary(_) => continue,
                tungstenite::Message::Ping(payload) => {
                    self.socket
                        .send(tungstenite::Message::Pong(payload))
                        .map_err(|error| AdapterError::Transport(format!("ws pong: {error}")))?;
                }
                tungstenite::Message::Pong(_) => continue,
                tungstenite::Message::Close(_) => {
                    let _ = self.socket.close(None);
                    return Ok(None);
                }
                tungstenite::Message::Frame(_) => continue,
            }
        }
    }

    fn close(&mut self) -> Result<(), AdapterError> {
        self.socket
            .close(None)
            .map_err(|error| AdapterError::Transport(format!("ws close: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_text_frame_roundtrip() {
        let bytes = encode_server_text_frame("{\"type\":\"ready\"}");
        let frame = decode_frame(&bytes).expect("decode");
        assert!(frame.fin);
        assert_eq!(frame.opcode, Opcode::Text);
        assert_eq!(frame.payload, b"{\"type\":\"ready\"}");
    }

    #[test]
    fn client_text_frame_is_masked_and_decodes() {
        let bytes = encode_client_text_frame("hello");
        assert_eq!(bytes[1] & 0x80, 0x80, "client frames must be masked");
        let frame = decode_frame(&bytes).expect("decode");
        assert_eq!(frame.payload, b"hello");
    }

    #[test]
    fn masked_server_frame_decodes() {
        let mut bytes = encode_server_text_frame("abc");
        bytes[1] |= 0x80;
        let key = [0x11, 0x22, 0x33, 0x44];
        bytes.splice(2..2, key.iter().copied());
        for index in 0..3 {
            bytes[2 + 4 + index] ^= key[index % 4];
        }
        let frame = decode_frame(&bytes).expect("decode");
        assert_eq!(frame.payload, b"abc");
    }

    #[test]
    fn length_boundaries_125_126_65535_65536() {
        for length in [125usize, 126, 65535, 65536, 70_000] {
            let payload = "x".repeat(length);
            let bytes = encode_server_text_frame(&payload);
            let frame = decode_frame(&bytes).expect("decode");
            assert_eq!(frame.payload.len(), length);
            assert_eq!(frame.payload, payload.as_bytes());
        }
    }

    #[test]
    fn rejects_truncated_and_malformed_frames() {
        assert!(decode_frame(&[]).is_err());
        assert!(decode_frame(&[0x81]).is_err());
        assert!(decode_frame(&[0x81, 126, 0x01]).is_err());
        assert!(decode_frame(&[0x81, 127, 0, 0, 0]).is_err());
        assert!(decode_frame(&[0x81, 5, b'a', b'b']).is_err());
        assert!(decode_frame(&[0xC1, 0]).is_err());
        assert!(decode_frame(&[0x83, 0]).is_err());
        assert!(decode_frame(&[0x09, 0]).is_err());
        let mut bytes = vec![0x89, 126, 0x00, 126];
        bytes.extend(std::iter::repeat_n(0u8, 126));
        assert!(decode_frame(&bytes).is_err());
    }

    #[test]
    fn close_frame_roundtrip() {
        let bytes = encode_close_frame(1000, "bye");
        let frame = decode_frame(&bytes).expect("decode");
        assert_eq!(frame.opcode, Opcode::Close);
        assert_eq!(&frame.payload[..2], &[0x03, 0xE8]);
        assert_eq!(&frame.payload[2..], b"bye");
    }
}
