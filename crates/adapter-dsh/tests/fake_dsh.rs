//! Fake DSH test kit + M5-C integration tests.
//!
//! Per the M5-C plan, tests do not depend on a real DSH or a real network:
//! - FakeHttp scripts HTTP responses and records requests.
//! - FakeWs feeds REAL RFC 6455 frame byte sequences (encoded with the
//!   crate's own codec) and records client frames.
//!
//! Every protocol layer (auth, mux, events, notify, usage, pipeline) is
//! exercised end to end over the in-memory transports.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use dsh_adapter_dsh::*;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// Scripted HTTP transport: pops the next response, records every request.
#[derive(Default)]
pub struct FakeHttp {
    pub responses: VecDeque<HttpResponse>,
    pub requests: Vec<HttpRequest>,
}

impl FakeHttp {
    pub fn script(mut self, response: HttpResponse) -> Self {
        self.responses.push_back(response);
        self
    }
}

impl HttpTransport for FakeHttp {
    fn roundtrip(&mut self, request: &HttpRequest) -> Result<HttpResponse, AdapterError> {
        self.requests.push(request.clone());
        self.responses
            .pop_front()
            .ok_or_else(|| AdapterError::Transport("fake: no scripted response".to_string()))
    }
}

/// Byte-level WS fake: feeds frame bytes, records client text frames.
pub struct FakeWs {
    frames: VecDeque<Vec<u8>>,
    sent: Arc<Mutex<Vec<String>>>,
}

impl FakeWs {
    pub fn new(frames: Vec<Vec<u8>>) -> Self {
        Self::new_with_sent(frames, Arc::new(Mutex::new(Vec::new())))
    }

    pub fn new_with_sent(frames: Vec<Vec<u8>>, sent: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            frames: frames.into(),
            sent,
        }
    }

    pub fn sent(&self) -> Vec<String> {
        self.sent.lock().expect("sent lock").clone()
    }
}

impl WsTransport for FakeWs {
    fn send_text(&mut self, text: &str) -> Result<(), AdapterError> {
        self.sent.lock().expect("sent lock").push(text.to_string());
        Ok(())
    }

    fn recv_text(&mut self) -> Result<Option<String>, AdapterError> {
        loop {
            let Some(bytes) = self.frames.pop_front() else {
                return Ok(None);
            };
            let frame = decode_frame(&bytes)?;
            match frame.opcode {
                Opcode::Text => {
                    let text = String::from_utf8(frame.payload)
                        .map_err(|_| AdapterError::Protocol("fake: non-utf8 text".to_string()))?;
                    return Ok(Some(text));
                }
                Opcode::Close => return Ok(None),
                _ => continue,
            }
        }
    }

    fn close(&mut self) -> Result<(), AdapterError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Frame helpers (server side of the mux + $events protocol)
// ---------------------------------------------------------------------------

fn http_response(status: u16, headers: &[(&str, &str)], body: &str) -> HttpResponse {
    HttpResponse {
        status,
        reason: String::new(),
        headers: headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        body: body.as_bytes().to_vec(),
    }
}

fn mux_item(stream_id: &str, value: Value) -> Vec<u8> {
    encode_server_text_frame(
        &json!({"type": "item", "streamId": stream_id, "value": value}).to_string(),
    )
}

fn mux_end(stream_id: &str) -> Vec<u8> {
    encode_server_text_frame(&json!({"type": "end", "streamId": stream_id}).to_string())
}

fn mux_error(stream_id: &str, code: &str, message: &str) -> Vec<u8> {
    encode_server_text_frame(
        &json!({
            "type": "error",
            "streamId": stream_id,
            "error": {"code": code, "message": message, "details": null}
        })
        .to_string(),
    )
}

fn ready_frame(stream_id: &str) -> Vec<u8> {
    mux_item(
        stream_id,
        json!({"type": "ready", "clientId": "client-1", "host": {"home": "C:\\home"}}),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

const STREAM: &str = "adapter-dsh-1";

#[test]
fn auth_then_jsonrpc_call_carries_cookie_and_envelope() {
    let http = FakeHttp::default()
        .script(http_response(
            303,
            &[("Set-Cookie", "dsh-auth-ab=cd; HttpOnly; Path=/"), ("Location", "/")],
            "",
        ))
        .script(http_response(
            200,
            &[("Content-Type", "application/json")],
            r#"{"type":"server-response","rpcId":"adapter-dsh-1","result":{"ok":true,"value":{"n":1}}}"#,
        ));
    let config = DshClientConfig::new("http://127.0.0.1:6800", "tok123");
    let mut client = DshClient::new(config, http);

    let auth = client.authenticate().expect("authenticate");
    assert_eq!(auth.cookie, "dsh-auth-ab=cd");
    assert_eq!(client.cookie(), Some("dsh-auth-ab=cd"));

    let value = client.call_api("session/list", json!({})).expect("call");
    assert_eq!(value, json!({"n": 1}));

    let fake = client.http_mut();
    assert_eq!(fake.requests.len(), 2);
    assert_eq!(fake.requests[0].method, "GET");
    assert_eq!(fake.requests[0].path, "/?token=tok123");

    assert_eq!(fake.requests[1].method, "POST");
    assert_eq!(fake.requests[1].path, "/api");
    let cookie = fake.requests[1]
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("cookie"))
        .map(|(_, value)| value.as_str());
    assert_eq!(cookie, Some("dsh-auth-ab=cd"));

    let envelope: Value =
        serde_json::from_slice(fake.requests[1].body.as_deref().expect("body")).expect("envelope");
    assert_eq!(envelope["type"], "client-request");
    assert_eq!(envelope["method"], "session/list");
    assert_eq!(envelope["payload"], json!({"args": {}}));
    assert!(envelope["rpcId"].as_str().is_some());
}

#[test]
fn submit_events_result_uses_verified_wire_shape() {
    let http = FakeHttp::default().script(http_response(
        200,
        &[("Content-Type", "application/json")],
        r#"{"type":"server-response","rpcId":"r","result":{"ok":true,"value":null}}"#,
    ));
    let config = DshClientConfig::new("http://127.0.0.1:6800", "tok");
    let mut client = DshClient::new(config, http);
    client
        .submit_events_result("client-1", "event-1", &EventOutcome::Next)
        .expect("submit");

    let fake = client.http_mut();
    let envelope: Value =
        serde_json::from_slice(fake.requests[0].body.as_deref().expect("body")).expect("envelope");
    assert_eq!(envelope["method"], "$events/result");
    assert_eq!(
        envelope["payload"],
        json!({"args": {"clientId": "client-1", "eventId": "event-1", "outcome": {"kind": "next"}}})
    );
}

#[test]
fn pipeline_streams_notifications_and_aggregates_usage() {
    let frames = vec![
        ready_frame(STREAM),
        mux_item(
            STREAM,
            json!({"type": "waterfall", "event": "approval/request", "eventId": "e1", "agentId": "ag", "request": {"toolName": "browser_navigate"}}),
        ),
        mux_item(
            STREAM,
            json!({"type": "event", "event": "api-session/removed", "args": [{"sessionId": "s-1"}]}),
        ),
        mux_item(
            STREAM,
            json!({"type": "event", "event": "api-session/status", "args": [{"sessionId": "s-1", "usage": {"inputTokens": 4, "outputTokens": 6}}]}),
        ),
        mux_item(
            STREAM,
            json!({"type": "event", "event": "settings/document-updated", "args": ["settings", 3]}),
        ),
        mux_item(
            STREAM,
            json!({"type": "event", "event": "cordis/dynamic-package", "args": [{"pluginId": "p1", "packageId": "pk", "pluginRunId": "r", "name": "x"}]}),
        ),
        mux_item(
            STREAM,
            json!({"type": "waterfall", "event": "user-questions/request", "eventId": "e2", "agentId": "ag", "request": {"questions": [{"id": "q", "question": "继续？"}]}}),
        ),
        mux_item(
            STREAM,
            json!({"type": "event", "event": "commands/change", "args": []}),
        ),
        mux_item(
            STREAM,
            json!({"type": "event", "event": "some/unknown", "args": []}),
        ),
        // A frame for a stream we never opened: must be ignored, not fatal.
        mux_item(
            "other-stream",
            json!({"type": "event", "event": "x", "args": []}),
        ),
        mux_end(STREAM),
    ];
    let fake = FakeWs::new(frames);
    let stream = EventStream::open(fake).expect("open");
    let mut pipeline = AdapterPipeline::new(stream);

    let mut kinds = Vec::new();
    while let Some(notification) = pipeline.next_notification().expect("next") {
        kinds.push(notification.event);
    }
    assert_eq!(
        kinds,
        vec![
            NotificationEventKind::ApprovalRequired,
            NotificationEventKind::TurnCompleted,
            NotificationEventKind::ConfigChanged,
            NotificationEventKind::ConfigChanged,
            NotificationEventKind::QuestionRequired,
        ]
    );

    assert_eq!(
        pipeline.stream.ready.as_ref().expect("ready").client_id,
        "client-1"
    );
    assert_eq!(pipeline.stream.parsed, 9);
    assert_eq!(
        pipeline.stream.skipped, 3,
        "ready + commands/change + some/unknown"
    );
    assert_eq!(pipeline.stream.foreign_frames, 1);

    let (records, totals) = pipeline.snapshot_usage(1_788_048_000_000);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input_tokens, 4);
    assert_eq!(records[0].output_tokens, 6);
    assert_eq!(records[0].source, "dsh");
    assert_eq!(records[0].period.start, "2026-08-30T00:00:00.000Z");
    assert_eq!(totals.input_tokens, 4);
    assert_eq!(totals.estimate_count, 1);
}

#[test]
fn open_frame_uses_verified_mux_shape() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let fake = FakeWs::new_with_sent(vec![ready_frame(STREAM), mux_end(STREAM)], sent.clone());
    let mut stream = EventStream::open(fake).expect("open");
    while stream.next_event().expect("drain").is_some() {}

    let sent = sent.lock().expect("sent lock");
    assert_eq!(sent.len(), 1);
    let open: Value = serde_json::from_str(&sent[0]).expect("open frame json");
    assert_eq!(open["type"], "open");
    assert_eq!(open["streamId"], "adapter-dsh-1");
    assert_eq!(open["endpoint"], "$events");
    assert_eq!(open["payload"], json!({ "args": {} }));
}

#[test]
fn malformed_event_fails_closed() {
    let frames = vec![
        ready_frame(STREAM),
        mux_item(STREAM, json!([1, 2, 3])),
        mux_end(STREAM),
    ];
    let fake = FakeWs::new(frames);
    let mut stream = EventStream::open(fake).expect("open");
    assert!(stream.next_event().expect("ready").is_some());
    let error = stream.next_event().expect_err("malformed must fail closed");
    assert!(matches!(error, AdapterError::Protocol(_)));
}

#[test]
fn mux_error_frame_fails_closed() {
    let frames = vec![
        ready_frame(STREAM),
        mux_error(STREAM, "E_BAD", "stream exploded"),
        mux_end(STREAM),
    ];
    let fake = FakeWs::new(frames);
    let mut stream = EventStream::open(fake).expect("open");
    assert!(stream.next_event().expect("ready").is_some());
    let error = stream.next_event().expect_err("stream error must surface");
    assert!(matches!(error, AdapterError::Rpc(_)));
}

#[test]
fn ws_close_ends_the_stream() {
    let frames = vec![ready_frame(STREAM), encode_close_frame(1000, "bye")];
    let fake = FakeWs::new(frames);
    let mut stream = EventStream::open(fake).expect("open");
    assert!(stream.next_event().expect("ready").is_some());
    assert!(stream.next_event().expect("closed").is_none());
}

#[test]
fn session_flow_feeds_usage_aggregation() {
    let frames = vec![
        mux_item(
            STREAM,
            json!({"type": "usage", "usage": {"inputTokens": 10, "outputTokens": 2}}),
        ),
        mux_item(
            STREAM,
            json!({"type": "assistant/message", "data": {"usage": {"inputTokens": 3, "outputTokens": 1, "cacheReadTokens": 5}}}),
        ),
        mux_item(STREAM, json!({"type": "assistant/message", "data": {}})),
        mux_end(STREAM),
    ];
    let fake = FakeWs::new(frames);
    let mut flow = SessionFlow::open(fake, "s-1").expect("open session flow");

    let mut aggregator = UsageAggregator::new();
    while let Some(value) = flow.next_event().expect("flow next") {
        aggregator.ingest_value(Some("s-1"), &value);
    }

    assert_eq!(aggregator.usage_events, 2);
    assert_eq!(aggregator.skipped_events, 1);
    let records = aggregator.snapshot(1_788_048_000_000);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input_tokens, 13);
    assert_eq!(records[0].output_tokens, 3);
    assert_eq!(records[0].cache_read_tokens, Some(5));
}

#[test]
fn auth_rejects_non_redirect_scripted_response() {
    let http = FakeHttp::default().script(http_response(200, &[], ""));
    let config = DshClientConfig::new("http://127.0.0.1:6800", "tok");
    let mut client = DshClient::new(config, http);
    let error = client.authenticate().expect_err("must reject 200");
    assert!(matches!(error, AdapterError::Auth(_)));
}
