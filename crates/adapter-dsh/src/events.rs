//! $events stream message parsing (verified 2026-08-30).
//!
//! Event frames arrive as mux item values on the $events stream:
//!   ready     = {"type":"ready","clientId":"<uuid>","host":{"home":"..."}}
//!   emit      = {"type":"event","event":"<allowlisted>","args":[...]}  (args ARRAY)
//!   waterfall = {"type":"waterfall","event":"<allowlisted>","eventId":"...","agentId":"...","request":{...}}
//!   cancel    = {"type":"cancel","eventId":"..."}
//!
//! Waterfall results do NOT ride the stream: the client answers over HTTP
//! POST /api/$events/result (see crate::jsonrpc). DSH has no API versioning
//! and no version signal anywhere - parsing is shape-driven and tolerant of
//! extra fields, but fails closed on missing required fields.

use serde_json::{Map, Value};

use crate::error::AdapterError;

/// Verified event allowlist (18 events, D:\deepseek-harness
/// remotes/src/remote-events.ts + session-controller).
/// The adapter consumes the subset relevant to M5-C; the rest is skipped.
pub const ALLOWLIST: [&str; 18] = [
    "agent-preset/selected",
    "approval/request",
    "api-session/activity",
    "api-session/added",
    "api-session/error",
    "api-session/removed",
    "api-session/status",
    "commands/change",
    "credentials/reference-updated",
    "cordis/request-run",
    "cordis/request-run-resolved",
    "cordis/dynamic-package",
    "cordis/dynamic-retract",
    "cordis/inspect-query",
    "cordis/inspect-query-resolved",
    "llm/adapters-updated",
    "settings/document-updated",
    "user-questions/request",
];

/// Client identity announced by the ready frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    pub client_id: String,
    pub host_home: Option<String>,
}

/// A decoded $events message.
#[derive(Debug, Clone, PartialEq)]
pub enum EventMessage {
    Ready(ClientIdentity),
    /// Broadcast-style event; args is a JSON array per the DSH wire.
    Emit {
        event: String,
        args: Value,
    },
    /// Waterfall event awaiting a result via /api/$events/result.
    Waterfall {
        event: String,
        event_id: Option<String>,
        agent_id: Option<String>,
        request: Value,
    },
    /// A waterfall was cancelled by the server.
    Cancel {
        event_id: Option<String>,
        reason: Option<String>,
    },
    /// A frame type we do not model yet; consumers skip it.
    Unknown {
        kind: String,
    },
}

/// The M5-C relevant subset of the allowlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowedEvent {
    /// api-session/<kind>, e.g. activity | added | error | removed | status.
    ApiSession(String),
    ApprovalRequest,
    UserQuestionRequest,
    SettingsDocumentUpdated,
    CordisDynamicPackage,
    CordisDynamicRetract,
}

/// Parse a $events message from JSON text (pure).
pub fn parse_message(text: &str) -> Result<EventMessage, AdapterError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| AdapterError::Protocol(format!("events: invalid json: {error}")))?;
    parse_message_value(&value)
}

/// Parse a $events message from an already-decoded JSON value (pure).
pub fn parse_message_value(value: &Value) -> Result<EventMessage, AdapterError> {
    let object = value
        .as_object()
        .ok_or_else(|| AdapterError::Protocol("events: non-object message".to_string()))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| AdapterError::Protocol("events: missing type".to_string()))?;
    match kind {
        "ready" => {
            let client_id = object
                .get("clientId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AdapterError::Protocol("events: ready without clientId".to_string())
                })?;
            let host_home = object
                .get("host")
                .and_then(Value::as_object)
                .and_then(|host| host.get("home"))
                .and_then(Value::as_str)
                .map(str::to_string);
            Ok(EventMessage::Ready(ClientIdentity {
                client_id: client_id.to_string(),
                host_home,
            }))
        }
        "event" => {
            let event = object.get("event").and_then(Value::as_str).ok_or_else(|| {
                AdapterError::Protocol("events: event without event name".to_string())
            })?;
            let args = object.get("args").cloned().unwrap_or(Value::Null);
            Ok(EventMessage::Emit {
                event: event.to_string(),
                args,
            })
        }
        "waterfall" => {
            let event = object.get("event").and_then(Value::as_str).ok_or_else(|| {
                AdapterError::Protocol("events: waterfall without event name".to_string())
            })?;
            let event_id = get_str(object, &["eventId"]);
            let agent_id = get_str(object, &["agentId"]);
            let request = object
                .get("request")
                .cloned()
                .map(normalize_object)
                .unwrap_or(Value::Null);
            Ok(EventMessage::Waterfall {
                event: event.to_string(),
                event_id,
                agent_id,
                request,
            })
        }
        "cancel" => Ok(EventMessage::Cancel {
            event_id: get_str(object, &["eventId", "event_id"]),
            reason: get_str(object, &["reason"]),
        }),
        other => Ok(EventMessage::Unknown {
            kind: other.to_string(),
        }),
    }
}

/// Classify a message against the M5-C allowlist subset.
pub fn allowlist(message: &EventMessage) -> Option<AllowedEvent> {
    match message {
        EventMessage::Emit { event, .. } | EventMessage::Waterfall { event, .. } => {
            match event.as_str() {
                name if name.starts_with("api-session/") => Some(AllowedEvent::ApiSession(
                    name["api-session/".len()..].to_string(),
                )),
                "approval/request" => Some(AllowedEvent::ApprovalRequest),
                "user-questions/request" => Some(AllowedEvent::UserQuestionRequest),
                "settings/document-updated" => Some(AllowedEvent::SettingsDocumentUpdated),
                "cordis/dynamic-package" => Some(AllowedEvent::CordisDynamicPackage),
                "cordis/dynamic-retract" => Some(AllowedEvent::CordisDynamicRetract),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Payload of an event: emit args[0] (the args array's first element) or the
/// waterfall request object. Returns Null when nothing usable exists.
pub(crate) fn payload_of(message: &EventMessage) -> Value {
    match message {
        EventMessage::Emit { args, .. } => match args {
            Value::Array(items) => items.first().cloned().unwrap_or(Value::Null),
            other => other.clone(),
        },
        EventMessage::Waterfall { request, .. } => request.clone(),
        _ => Value::Null,
    }
}

/// Best-effort session id of an event payload (drift-tolerant).
pub(crate) fn session_id_of(message: &EventMessage) -> Option<String> {
    let payload = payload_of(message);
    let object = payload.as_object()?;
    get_str(object, &["sessionId", "session_id"]).or_else(|| {
        object
            .get("session")
            .and_then(Value::as_object)
            .and_then(|session| session.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn normalize_object(value: Value) -> Value {
    match value {
        Value::Object(_) => value,
        Value::String(text) => serde_json::from_str(&text).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn get_str(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emit(event: &str, args: Value) -> EventMessage {
        EventMessage::Emit {
            event: event.to_string(),
            args,
        }
    }

    #[test]
    fn parses_ready_frame() {
        let message = parse_message(
            "{\"type\":\"ready\",\"clientId\":\"a1b2\",\"host\":{\"home\":\"C:\\\\Users\\\\me\"},\"extra\":1}",
        )
        .expect("ready");
        assert_eq!(
            message,
            EventMessage::Ready(ClientIdentity {
                client_id: "a1b2".to_string(),
                host_home: Some("C:\\Users\\me".to_string()),
            })
        );
        let message = parse_message("{\"type\":\"ready\",\"clientId\":\"a\"}").expect("ready");
        assert_eq!(
            message,
            EventMessage::Ready(ClientIdentity {
                client_id: "a".to_string(),
                host_home: None,
            })
        );
        assert!(parse_message("{\"type\":\"ready\"}").is_err());
    }

    #[test]
    fn parses_emit_with_array_args() {
        let message = parse_message(
            "{\"type\":\"event\",\"event\":\"settings/document-updated\",\"args\":[\"settings\",3]}",
        )
        .expect("emit");
        assert_eq!(
            message,
            emit(
                "settings/document-updated",
                serde_json::json!(["settings", 3])
            )
        );
        assert!(parse_message("{\"type\":\"event\"}").is_err());
    }

    #[test]
    fn parses_waterfall_and_cancel() {
        let message = parse_message(
            "{\"type\":\"waterfall\",\"event\":\"approval/request\",\"eventId\":\"e1\",\"agentId\":\"ag\",\"request\":{\"toolName\":\"browser_navigate\"}}",
        )
        .expect("waterfall");
        assert_eq!(
            message,
            EventMessage::Waterfall {
                event: "approval/request".to_string(),
                event_id: Some("e1".to_string()),
                agent_id: Some("ag".to_string()),
                request: serde_json::json!({"toolName": "browser_navigate"}),
            }
        );

        let cancel = parse_message("{\"type\":\"cancel\",\"eventId\":\"e1\"}").expect("cancel");
        assert_eq!(
            cancel,
            EventMessage::Cancel {
                event_id: Some("e1".to_string()),
                reason: None,
            }
        );
    }

    #[test]
    fn unknown_types_survive_but_foreign_messages_fail_closed() {
        assert!(matches!(
            parse_message("{\"type\":\"something-new\"}").expect("unknown"),
            EventMessage::Unknown { kind } if kind == "something-new"
        ));
        assert!(parse_message("not json").is_err());
        assert!(parse_message("[1]").is_err());
        assert!(parse_message("{\"noType\":1}").is_err());
    }

    #[test]
    fn allowlist_classifies_m5c_subset() {
        assert_eq!(
            allowlist(&emit("api-session/removed", Value::Null)),
            Some(AllowedEvent::ApiSession("removed".to_string()))
        );
        assert_eq!(
            allowlist(&emit("approval/request", Value::Null)),
            Some(AllowedEvent::ApprovalRequest)
        );
        assert_eq!(
            allowlist(&emit("user-questions/request", Value::Null)),
            Some(AllowedEvent::UserQuestionRequest)
        );
        assert_eq!(
            allowlist(&emit("settings/document-updated", Value::Null)),
            Some(AllowedEvent::SettingsDocumentUpdated)
        );
        assert_eq!(
            allowlist(&emit("cordis/dynamic-package", Value::Null)),
            Some(AllowedEvent::CordisDynamicPackage)
        );
        assert_eq!(
            allowlist(&emit("cordis/dynamic-retract", Value::Null)),
            Some(AllowedEvent::CordisDynamicRetract)
        );
        // Allowlisted but outside the M5-C subset.
        assert_eq!(allowlist(&emit("commands/change", Value::Null)), None);
        // api-session/* is a wildcard per the module scope; the notify
        // mapping is the strict gate (only verified kinds produce events).
        assert_eq!(
            allowlist(&emit("api-session/unknown-kind", Value::Null)),
            Some(AllowedEvent::ApiSession("unknown-kind".to_string()))
        );
        // Non-allowlisted.
        assert_eq!(allowlist(&emit("some/other", Value::Null)), None);
        assert_eq!(
            allowlist(&EventMessage::Ready(ClientIdentity {
                client_id: "c".to_string(),
                host_home: None,
            })),
            None
        );
    }

    #[test]
    fn payload_of_unwraps_emit_args_array() {
        assert_eq!(
            payload_of(&emit(
                "cordis/dynamic-package",
                serde_json::json!([{"pluginId": "p"}])
            )),
            serde_json::json!({"pluginId": "p"})
        );
        assert_eq!(
            payload_of(&emit(
                "cordis/dynamic-package",
                serde_json::json!({"pluginId": "p"})
            )),
            serde_json::json!({"pluginId": "p"})
        );
        assert_eq!(payload_of(&emit("x", Value::Null)), Value::Null);
    }
}
