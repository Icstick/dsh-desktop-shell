//! DSH /api/remote.mux stream multiplexer (verified 2026-08-30).
//!
//! DSH exposes a single WebSocket route, /api/remote.mux. Logical streams
//! (e.g. $events, session flows) are opened by the client with an open
//! frame; the server answers with item/error/end frames tagged by streamId.
//!
//! Client frames (exact keys, verified in D:\deepseek-harness):
//!   {"type":"open","streamId":"...","endpoint":"<ns>/<name>","payload":{"args":{...}}}
//!   {"type":"cancel","streamId":"..."}
//! Server frames:
//!   {"type":"item","streamId":"...","value":<any json>}
//!   {"type":"error","streamId":"...","error":{"code","message","details"}}
//!   {"type":"end","streamId":"..."}
//!
//! The server heartbeats with WS-level pings (handled by the transport).

use serde_json::Value;

use crate::error::AdapterError;
use crate::ws::WsTransport;

/// The $events logical endpoint on the mux.
pub const EVENTS_ENDPOINT: &str = "$events";

/// One decoded mux frame from the server.
#[derive(Debug, Clone, PartialEq)]
pub enum MuxFrame {
    /// A payload for the named stream (item.value).
    Item { stream_id: String, value: Value },
    /// Terminal stream failure.
    Error {
        stream_id: String,
        code: Option<String>,
        message: Option<String>,
        details: Option<Value>,
    },
    /// The stream ended cleanly.
    End { stream_id: String },
}

/// Parse one mux message (pure). Unknown types are rejected - the mux
/// envelope is exact-keys on the DSH side, so a foreign frame shape means a
/// protocol drift and must fail closed rather than be guessed.
pub fn parse_mux_frame(text: &str) -> Result<MuxFrame, AdapterError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| AdapterError::Protocol(format!("mux: invalid json: {error}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| AdapterError::Protocol("mux: non-object frame".to_string()))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| AdapterError::Protocol("mux: missing type".to_string()))?;
    let stream_id = object
        .get("streamId")
        .and_then(Value::as_str)
        .ok_or_else(|| AdapterError::Protocol("mux: missing streamId".to_string()))?
        .to_string();
    match kind {
        "item" => {
            let value = object
                .get("value")
                .cloned()
                .ok_or_else(|| AdapterError::Protocol("mux: item without value".to_string()))?;
            Ok(MuxFrame::Item { stream_id, value })
        }
        "error" => {
            let error = object
                .get("error")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    AdapterError::Protocol("mux: error without error object".to_string())
                })?;
            Ok(MuxFrame::Error {
                stream_id,
                code: error
                    .get("code")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                details: error.get("details").cloned(),
            })
        }
        "end" => Ok(MuxFrame::End { stream_id }),
        other => Err(AdapterError::Protocol(format!(
            "mux: unknown frame type {other:?}"
        ))),
    }
}

/// Client side of the mux over one WS connection.
pub struct MuxClient<T: WsTransport> {
    transport: T,
    next_stream_id: u64,
}

impl<T: WsTransport> MuxClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            next_stream_id: 0,
        }
    }

    /// Open a logical stream and return its client-chosen stream id.
    pub fn open(&mut self, endpoint: &str, payload: Value) -> Result<String, AdapterError> {
        self.next_stream_id = self.next_stream_id.wrapping_add(1);
        let stream_id = format!("adapter-dsh-{}", self.next_stream_id);
        let frame = serde_json::json!({
            "type": "open",
            "streamId": stream_id,
            "endpoint": endpoint,
            "payload": payload,
        });
        self.transport
            .send_text(&frame.to_string())
            .map_err(|error| AdapterError::Transport(format!("mux open send: {error}")))?;
        Ok(stream_id)
    }

    /// Cancel a logical stream.
    pub fn cancel(&mut self, stream_id: &str) -> Result<(), AdapterError> {
        let frame = serde_json::json!({ "type": "cancel", "streamId": stream_id });
        self.transport
            .send_text(&frame.to_string())
            .map_err(|error| AdapterError::Transport(format!("mux cancel send: {error}")))
    }

    /// Next mux frame from the server; Ok(None) when the WS closes.
    pub fn next_frame(&mut self) -> Result<Option<MuxFrame>, AdapterError> {
        let Some(text) = self.transport.recv_text()? else {
            return Ok(None);
        };
        parse_mux_frame(&text).map(Some)
    }

    /// Underlying transport (for close etc.).
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_item_error_end_frames() {
        let item = parse_mux_frame(
            "{\"type\":\"item\",\"streamId\":\"s1\",\"value\":{\"type\":\"ready\",\"clientId\":\"c\"}}",
        )
        .expect("item");
        assert_eq!(
            item,
            MuxFrame::Item {
                stream_id: "s1".to_string(),
                value: serde_json::json!({"type": "ready", "clientId": "c"}),
            }
        );

        let error = parse_mux_frame(
            "{\"type\":\"error\",\"streamId\":\"s1\",\"error\":{\"code\":\"E_BAD\",\"message\":\"nope\",\"details\":{\"x\":1}}}",
        )
        .expect("error");
        assert_eq!(
            error,
            MuxFrame::Error {
                stream_id: "s1".to_string(),
                code: Some("E_BAD".to_string()),
                message: Some("nope".to_string()),
                details: Some(serde_json::json!({"x": 1})),
            }
        );

        let end = parse_mux_frame("{\"type\":\"end\",\"streamId\":\"s1\"}").expect("end");
        assert_eq!(
            end,
            MuxFrame::End {
                stream_id: "s1".to_string()
            }
        );
    }

    #[test]
    fn rejects_foreign_or_malformed_frames() {
        assert!(parse_mux_frame("{\"type\":\"item\"}").is_err());
        assert!(parse_mux_frame("{\"streamId\":\"s\"}").is_err());
        assert!(parse_mux_frame("{\"type\":\"bogus\",\"streamId\":\"s\"}").is_err());
        assert!(parse_mux_frame("not json").is_err());
        assert!(parse_mux_frame("[1,2]").is_err());
        assert!(parse_mux_frame("{\"type\":\"error\",\"streamId\":\"s\",\"error\":null}").is_err());
    }
}
