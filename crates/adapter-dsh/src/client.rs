//! DshClient: connect, authenticate, open the $events mux stream, and the
//! notification/usage pipeline (M5-C).
//!
//! Flow (verified 2026-08-30):
//!   1. GET /?token=<launch token>  -> 303 + Set-Cookie dsh-auth-* (stored)
//!   2. POST /api/<ns>/<method> with the cookie for JSON-RPC
//!   3. WS connect to ws://<host>/api/remote.mux with the cookie
//!   4. mux open {"type":"open","streamId":"...","endpoint":"$events","payload":{"args":{}}}
//!   5. consume mux item values as $events messages
//!
//! WS upgrade requires the cookie (401/403 otherwise); the Host fence is
//! loopback-only, so the adapter targets 127.0.0.1 base URLs.

use std::time::Duration;

use serde_json::{Value, json};

use crate::auth::{AuthResult, build_token_request, parse_auth_response};
use crate::error::AdapterError;
use crate::events::{ClientIdentity, EventMessage, allowlist, parse_message_value};
use crate::http::HttpTransport;
use crate::jsonrpc::{ApiClient, EventOutcome};
use crate::mux::{EVENTS_ENDPOINT, MuxClient, MuxFrame};
use crate::notify::{AdapterNotification, map as map_notification};
use crate::usage::{UsageAggregator, UsageRecord, UsageTotals};
use crate::ws::WsTransport;

/// The mux WS route on the DSH HTTP server.
pub const REMOTE_MUX_PATH: &str = "/api/remote.mux";
/// Session-follow endpoint name on the mux. NOTE: the exact endpoint name
/// and open payload are not yet verified against a live DSH; the mux
/// machinery below is real and tested, this constant is the documented
/// acquisition seam (ADR-0018 decision 6: usage is partial).
pub const SESSION_FOLLOW_ENDPOINT: &str = "session/follow";

/// Adapter connection configuration.
#[derive(Debug, Clone)]
pub struct DshClientConfig {
    /// e.g. http://127.0.0.1:6800 (loopback only, http only).
    pub base_url: String,
    /// DSH launch token (from the launch command line / env).
    pub launch_token: String,
    /// Socket timeout for HTTP and WS operations.
    pub timeout: Duration,
}

impl DshClientConfig {
    pub fn new(base_url: impl Into<String>, launch_token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            launch_token: launch_token.into(),
            timeout: Duration::from_secs(10),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Parsed base URL pieces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl {
    pub host_port: String,
    pub path_prefix: String,
}

/// Parse http://host:port[/prefix] (pure).
pub fn parse_base_url(base_url: &str) -> Result<BaseUrl, AdapterError> {
    let rest = base_url
        .strip_prefix("http://")
        .ok_or_else(|| AdapterError::Auth("base_url must use http:// (loopback)".to_string()))?;
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, String::new()),
    };
    if authority.is_empty() {
        return Err(AdapterError::Auth("base_url without host".to_string()));
    }
    Ok(BaseUrl {
        host_port: authority.to_string(),
        path_prefix: path.trim_end_matches('/').to_string(),
    })
}

/// Connection manager over the pluggable HTTP transport.
pub struct DshClient<H: HttpTransport> {
    config: DshClientConfig,
    http: H,
    cookie: Option<String>,
}

impl<H: HttpTransport> DshClient<H> {
    pub fn new(config: DshClientConfig, http: H) -> Self {
        Self {
            config,
            http,
            cookie: None,
        }
    }

    /// Launch-token exchange; stores the cookie for later requests.
    pub fn authenticate(&mut self) -> Result<AuthResult, AdapterError> {
        let request = build_token_request(&self.config.base_url, &self.config.launch_token)?;
        let response = self.http.roundtrip(&request)?;
        let result = parse_auth_response(&response)?;
        self.cookie = Some(result.cookie.clone());
        Ok(result)
    }

    /// The launch cookie, after a successful authenticate().
    pub fn cookie(&self) -> Option<&str> {
        self.cookie.as_deref()
    }

    /// Access to the underlying HTTP transport (wiring/diagnostics).
    pub fn http_mut(&mut self) -> &mut H {
        &mut self.http
    }

    /// The mux WS URL for this base (ws://host:port/prefix/api/remote.mux).
    pub fn ws_url(&self) -> Result<String, AdapterError> {
        let base = parse_base_url(&self.config.base_url)?;
        Ok(format!(
            "ws://{}{}{}",
            base.host_port, base.path_prefix, REMOTE_MUX_PATH
        ))
    }

    /// JSON-RPC client view over the shared HTTP transport.
    pub fn api_client(&mut self) -> Result<ApiClient<&mut H>, AdapterError> {
        let base = parse_base_url(&self.config.base_url)?;
        Ok(ApiClient::new(
            &mut self.http,
            base.path_prefix,
            self.cookie.clone().unwrap_or_default(),
        ))
    }

    /// Convenience: one /api call.
    pub fn call_api(&mut self, method: &str, args: Value) -> Result<Value, AdapterError> {
        self.api_client()?.call(method, args)
    }

    /// Convenience: answer a waterfall event.
    pub fn submit_events_result(
        &mut self,
        client_id: &str,
        event_id: &str,
        outcome: &EventOutcome,
    ) -> Result<Value, AdapterError> {
        self.api_client()?
            .submit_events_result(client_id, event_id, outcome)
    }

    /// The mux WS route as an HTTP path (for diagnostics).
    pub fn api_path(&self) -> Result<String, AdapterError> {
        let base = parse_base_url(&self.config.base_url)?;
        Ok(format!("{}/api", base.path_prefix))
    }
}

/// $events stream over the mux (one logical stream on the shared WS).
pub struct EventStream<T: WsTransport> {
    mux: MuxClient<T>,
    stream_id: String,
    /// Total parsed events (including ready).
    pub parsed: u64,
    /// Events outside the M5-C allowlist subset.
    pub skipped: u64,
    /// Mux frames for streams we did not open (protocol drift indicator).
    pub foreign_frames: u64,
    /// Identity from the ready frame, once received.
    pub ready: Option<ClientIdentity>,
}

impl<T: WsTransport> EventStream<T> {
    /// Open the $events stream on a fresh mux connection.
    pub fn open(transport: T) -> Result<Self, AdapterError> {
        let mut mux = MuxClient::new(transport);
        let stream_id = mux.open(EVENTS_ENDPOINT, json!({ "args": {} }))?;
        Ok(Self {
            mux,
            stream_id,
            parsed: 0,
            skipped: 0,
            foreign_frames: 0,
            ready: None,
        })
    }

    /// Next event; Ok(None) when the stream or WS ends.
    pub fn next_event(&mut self) -> Result<Option<EventMessage>, AdapterError> {
        loop {
            match self.mux.next_frame()? {
                None => return Ok(None),
                Some(MuxFrame::End { stream_id }) if stream_id == self.stream_id => {
                    return Ok(None);
                }
                Some(MuxFrame::Error {
                    stream_id,
                    code,
                    message,
                    ..
                }) if stream_id == self.stream_id => {
                    return Err(AdapterError::Rpc(format!(
                        "$events stream error: {}: {}",
                        code.as_deref().unwrap_or("?"),
                        message.as_deref().unwrap_or("?")
                    )));
                }
                Some(MuxFrame::Item { stream_id, value }) if stream_id == self.stream_id => {
                    let message = parse_message_value(&value)?;
                    self.parsed += 1;
                    if let EventMessage::Ready(identity) = &message {
                        self.ready = Some(identity.clone());
                    }
                    if allowlist(&message).is_none() {
                        self.skipped += 1;
                    }
                    return Ok(Some(message));
                }
                Some(_) => {
                    self.foreign_frames += 1;
                }
            }
        }
    }

    /// Cancel the stream (best-effort).
    pub fn cancel(&mut self) -> Result<(), AdapterError> {
        self.mux.cancel(&self.stream_id)
    }
}

/// Session-follow seam for usage acquisition (documented gap: endpoint
/// contract pending live verification; the mux layer is real).
pub struct SessionFlow<T: WsTransport> {
    mux: MuxClient<T>,
    stream_id: String,
}

impl<T: WsTransport> SessionFlow<T> {
    /// Open a session flow. The open payload shape follows the $events
    /// convention ({args: {...}}) but is UNVERIFIED against a live DSH -
    /// see SESSION_FOLLOW_ENDPOINT.
    pub fn open(transport: T, session_id: &str) -> Result<Self, AdapterError> {
        let mut mux = MuxClient::new(transport);
        let stream_id = mux.open(
            SESSION_FOLLOW_ENDPOINT,
            json!({ "args": { "sessionId": session_id } }),
        )?;
        Ok(Self { mux, stream_id })
    }

    /// Next raw flow value (feed it to UsageAggregator::ingest_value).
    pub fn next_event(&mut self) -> Result<Option<Value>, AdapterError> {
        loop {
            match self.mux.next_frame()? {
                None => return Ok(None),
                Some(MuxFrame::End { stream_id }) if stream_id == self.stream_id => {
                    return Ok(None);
                }
                Some(MuxFrame::Error {
                    stream_id,
                    code,
                    message,
                    ..
                }) if stream_id == self.stream_id => {
                    return Err(AdapterError::Rpc(format!(
                        "session flow error: {}: {}",
                        code.as_deref().unwrap_or("?"),
                        message.as_deref().unwrap_or("?")
                    )));
                }
                Some(MuxFrame::Item { stream_id, value }) if stream_id == self.stream_id => {
                    return Ok(Some(value));
                }
                Some(_) => continue,
            }
        }
    }

    pub fn cancel(&mut self) -> Result<(), AdapterError> {
        self.mux.cancel(&self.stream_id)
    }
}

/// One-stop notification + usage pipeline over the $events stream.
pub struct AdapterPipeline<T: WsTransport> {
    pub stream: EventStream<T>,
    pub usage: UsageAggregator,
}

impl<T: WsTransport> AdapterPipeline<T> {
    pub fn new(stream: EventStream<T>) -> Self {
        Self {
            stream,
            usage: UsageAggregator::new(),
        }
    }

    /// Pull events until a notification is produced or the stream ends.
    /// Usage samples embedded in events are aggregated as a side effect.
    pub fn next_notification(&mut self) -> Result<Option<AdapterNotification>, AdapterError> {
        loop {
            let Some(message) = self.stream.next_event()? else {
                return Ok(None);
            };
            self.usage.ingest_message(&message);
            if let Some(notification) = map_notification(&message) {
                return Ok(Some(notification));
            }
        }
    }

    /// Usage snapshot + totals at the given clock.
    pub fn snapshot_usage(&self, now_unix_ms: u64) -> (Vec<UsageRecord>, UsageTotals) {
        (self.usage.snapshot(now_unix_ms), self.usage.totals())
    }
}

impl<T: WsTransport> EventStream<T> {
    /// Convenience for tests/consumers that do not need transport access.
    pub fn drain_events(&mut self) -> Result<Vec<EventMessage>, AdapterError> {
        let mut events = Vec::new();
        while let Some(event) = self.next_event()? {
            events.push(event);
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::session_id_of;

    #[test]
    fn parse_base_url_extracts_host_port_and_prefix() {
        let base = parse_base_url("http://127.0.0.1:6800").expect("base");
        assert_eq!(base.host_port, "127.0.0.1:6800");
        assert_eq!(base.path_prefix, "");
        let base = parse_base_url("http://127.0.0.1:6800/dsh/").expect("base");
        assert_eq!(base.path_prefix, "/dsh");
        assert!(parse_base_url("https://127.0.0.1:6800").is_err());
        assert!(parse_base_url("ftp://x").is_err());
        assert!(parse_base_url("http://").is_err());
    }

    #[test]
    fn ws_url_uses_remote_mux_route() {
        let config = DshClientConfig::new("http://127.0.0.1:6800", "tok");
        let client: DshClient<crate::http::TcpHttpTransport> = DshClient::new(
            config,
            crate::http::TcpHttpTransport::new("127.0.0.1:6800".parse().expect("addr")),
        );
        assert_eq!(
            client.ws_url().expect("ws url"),
            "ws://127.0.0.1:6800/api/remote.mux"
        );
    }

    #[test]
    fn session_id_of_is_exposed() {
        let message = EventMessage::Emit {
            event: "api-session/status".to_string(),
            args: json!([{"sessionId": "s-1"}]),
        };
        assert_eq!(session_id_of(&message).as_deref(), Some("s-1"));
    }
}
