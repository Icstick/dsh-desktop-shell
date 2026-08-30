//! DSH /api JSON-RPC envelope (verified 2026-08-30).
//!
//! Request:  POST /api/<ns>/<method>, Content-Type: application/json
//!   {"type":"client-request","rpcId":"...","method":"<ns>/<method>","payload":{"args":{...}}}
//! Response: {"type":"server-response","rpcId":"...","result":{"ok":true,"value":...}}
//!           {"type":"server-response","rpcId":"...","result":{"ok":false,"error":{"code","message","details"}}}
//!
//! Waterfall results are submitted as the RPC method $events/result with
//! payload {"args":{"clientId":"...","eventId":"...","outcome":{...}}} where
//! outcome is one of {"kind":"next"} | {"kind":"result","value":...} |
//! {"kind":"rejected","error":{"name","message","code?","details?"}}.

use serde_json::{Value, json};

use crate::error::AdapterError;
use crate::http::{HttpRequest, HttpTransport};

/// Type discriminator of the request envelope.
pub const RPC_TYPE: &str = "client-request";
/// RPC method used to answer waterfalls.
pub const EVENTS_RESULT_METHOD: &str = "$events/result";

/// Build a client-request envelope (pure).
pub fn build_envelope(method: &str, payload: Value, rpc_id: &str) -> Value {
    json!({
        "type": RPC_TYPE,
        "rpcId": rpc_id,
        "method": method,
        "payload": payload,
    })
}

/// Build the args member of a $events/result payload (pure, verified
/// shape). ApiClient::call wraps it as {"args": <this>}.
pub fn build_events_result_payload(
    client_id: &str,
    event_id: &str,
    outcome: &EventOutcome,
) -> Value {
    json!({
        "clientId": client_id,
        "eventId": event_id,
        "outcome": outcome.to_value(),
    })
}

/// Outcome of one waterfall event.
#[derive(Debug, Clone, PartialEq)]
pub enum EventOutcome {
    /// Continue the stream without a result.
    Next,
    /// Deliver a concrete result value.
    Result(Value),
    /// Reject the waterfall.
    Rejected {
        name: String,
        message: String,
        code: Option<String>,
        details: Option<Value>,
    },
}

impl EventOutcome {
    pub fn to_value(&self) -> Value {
        match self {
            EventOutcome::Next => json!({ "kind": "next" }),
            EventOutcome::Result(value) => json!({ "kind": "result", "value": value }),
            EventOutcome::Rejected {
                name,
                message,
                code,
                details,
            } => {
                let mut error = json!({ "name": name, "message": message });
                if let Some(code) = code {
                    error["code"] = json!(code);
                }
                if let Some(details) = details {
                    error["details"] = details.clone();
                }
                json!({ "kind": "rejected", "error": error })
            }
        }
    }
}

/// Parse a server-response body (pure, fail-closed).
pub fn parse_response(body: &str) -> Result<Value, AdapterError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| AdapterError::Protocol(format!("rpc: invalid json: {error}")))?;
    let result = value
        .get("result")
        .ok_or_else(|| AdapterError::Protocol("rpc: missing result member".to_string()))?;
    match result.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(result.get("value").cloned().unwrap_or(Value::Null)),
        Some(false) => {
            let code = result.pointer("/error/code").and_then(Value::as_str);
            let message = result
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("unknown rpc error");
            match code {
                Some(code) => Err(AdapterError::Rpc(format!("{code}: {message}"))),
                None => Err(AdapterError::Rpc(message.to_string())),
            }
        }
        _ => Err(AdapterError::Protocol(
            "rpc: unrecognized result member".to_string(),
        )),
    }
}

/// JSON-RPC client over the pluggable HTTP transport.
pub struct ApiClient<T: HttpTransport> {
    transport: T,
    base_path: String,
    cookie: String,
    next_rpc_id: u64,
}

impl<T: HttpTransport> ApiClient<T> {
    pub fn new(transport: T, base_path: impl Into<String>, cookie: String) -> Self {
        Self {
            transport,
            base_path: base_path.into(),
            cookie,
            next_rpc_id: 0,
        }
    }

    /// Call /api/<method> with an args-wrapped payload.
    pub fn call(&mut self, method: &str, args: Value) -> Result<Value, AdapterError> {
        self.next_rpc_id = self.next_rpc_id.wrapping_add(1);
        let rpc_id = format!("adapter-dsh-{}", self.next_rpc_id);
        let envelope = build_envelope(method, json!({ "args": args }), &rpc_id);
        let request = HttpRequest::post_json(format!("{}/api", self.base_path), &envelope)
            .with_header("Cookie", &self.cookie);
        let response = self.transport.roundtrip(&request)?;
        if !(200..300).contains(&response.status) {
            return Err(AdapterError::Http {
                status: response.status,
                detail: String::from_utf8_lossy(&response.body).into_owned(),
            });
        }
        let body = String::from_utf8_lossy(&response.body);
        parse_response(&body)
    }

    /// Submit a waterfall outcome (verified $events/result shape).
    pub fn submit_events_result(
        &mut self,
        client_id: &str,
        event_id: &str,
        outcome: &EventOutcome,
    ) -> Result<Value, AdapterError> {
        self.call(
            EVENTS_RESULT_METHOD,
            build_events_result_payload(client_id, event_id, outcome),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_shape_matches_verified_wire() {
        let envelope = build_envelope("session/list", json!({"args": {}}), "rpc-1");
        assert_eq!(envelope["type"], "client-request");
        assert_eq!(envelope["rpcId"], "rpc-1");
        assert_eq!(envelope["method"], "session/list");
        assert_eq!(envelope["payload"], json!({"args": {}}));
    }

    #[test]
    fn events_result_payload_shape_matches_verified_wire() {
        let payload = build_events_result_payload("cid", "eid", &EventOutcome::Next);
        assert_eq!(
            payload,
            json!({"clientId": "cid", "eventId": "eid", "outcome": {"kind": "next"}})
        );

        let payload =
            build_events_result_payload("cid", "eid", &EventOutcome::Result(json!({"ok": true})));
        assert_eq!(
            payload["outcome"],
            json!({"kind": "result", "value": {"ok": true}})
        );

        let payload = build_events_result_payload(
            "cid",
            "eid",
            &EventOutcome::Rejected {
                name: "Error".to_string(),
                message: "denied".to_string(),
                code: Some("E_DENY".to_string()),
                details: None,
            },
        );
        assert_eq!(
            payload["outcome"],
            json!({"kind": "rejected", "error": {"name": "Error", "message": "denied", "code": "E_DENY"}})
        );
    }

    #[test]
    fn parse_response_ok_and_error() {
        assert_eq!(
            parse_response("{\"type\":\"server-response\",\"rpcId\":\"r\",\"result\":{\"ok\":true,\"value\":{\"n\":1}}}")
                .expect("ok"),
            json!({"n": 1})
        );
        let error = parse_response(
            "{\"type\":\"server-response\",\"rpcId\":\"r\",\"result\":{\"ok\":false,\"error\":{\"code\":\"E_X\",\"message\":\"boom\",\"details\":null}}}",
        )
        .expect_err("error");
        assert!(matches!(error, AdapterError::Rpc(message) if message == "E_X: boom"));
        assert!(parse_response("{\"nope\":1}").is_err());
        assert!(parse_response("not json").is_err());
    }
}
