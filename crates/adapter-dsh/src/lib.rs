//! # dsh-adapter-dsh
//!
//! Legacy DSH adapter (MOD-ADAPTER-DSH, M5-C, ADR-0018). Consumes the DSH
//! public HTTP/WS surface only; every DSH-specific type stops at this
//! crate's boundary (ADR-0018 decision 3) and the desktop layer never sees
//! raw DSH JSON.
//!
//! Scope (ADR-0018 decision 6):
//! - notification: full - $events stream over the /api/remote.mux WS,
//!   cookie auth, allowlist, mapping to AdapterNotification.
//! - usage: partial - token usage aggregation from session flows
//!   (assistant/message data.usage and usage chunks); the session/follow
//!   acquisition seam is documented as a known gap.
//! - restart hints: downgraded to event inference (cordis/dynamic-package,
//!   cordis/dynamic-retract, settings/document-updated produce a generic
//!   config-changed hint).
//!
//! Degradation (ADR-0018 decision 4): every failure is an AdapterError;
//! the caller keeps the L0 baseline (DSH process + HTTP Web UI) untouched.
//!
//! Module layout:
//! - auth: launch token -> cookie (pure)
//! - http: minimal loopback HTTP/1.1 codec + transport trait
//! - ws: RFC 6455 frame codec (pure) + tungstenite transport
//! - mux: /api/remote.mux stream multiplexer (open/item/error/end)
//! - events: $events message parsing + M5-C allowlist
//! - notify: event -> AdapterNotification mapping
//! - usage: UsageAggregator -> usage-record/snapshot shapes
//! - jsonrpc: /api envelope + ApiClient + $events/result submission
//! - client: DshClient, EventStream, SessionFlow, AdapterPipeline
//!
//! All protocol code is pure and unit-tested; sockets are confined to the
//! thin transports (TcpHttpTransport, TungsteniteTransport). Tests drive
//! the crate with in-memory fakes feeding real frame byte sequences.

#![forbid(unsafe_code)]

pub mod auth;
pub mod client;
pub mod error;
pub mod events;
pub mod http;
pub mod jsonrpc;
pub mod mux;
pub mod notify;
pub mod usage;
pub mod ws;

pub use auth::{
    AuthResult, LAUNCH_COOKIE_PREFIX, build_token_request, parse_auth_response, percent_encode,
};
pub use client::{
    AdapterPipeline, BaseUrl, DshClient, DshClientConfig, EventStream, REMOTE_MUX_PATH,
    SESSION_FOLLOW_ENDPOINT, SessionFlow, parse_base_url,
};
pub use error::AdapterError;
pub use events::{
    ALLOWLIST, AllowedEvent, ClientIdentity, EventMessage, allowlist, parse_message,
    parse_message_value,
};
pub use http::{
    HttpRequest, HttpResponse, HttpTransport, TcpHttpTransport, decode_chunked, request_to_wire,
    response_from_wire,
};
pub use jsonrpc::{
    ApiClient, EVENTS_RESULT_METHOD, EventOutcome, build_envelope, build_events_result_payload,
    parse_response,
};
pub use mux::{EVENTS_ENDPOINT, MuxClient, MuxFrame, parse_mux_frame};
pub use notify::{
    AdapterNotification, ContentPolicy, NotificationEventKind, map as map_notification,
};
pub use usage::{
    USAGE_SOURCE, UsageAggregator, UsagePeriod, UsageRecord, UsageSample, UsageTotals,
    extract_sample, unix_ms_to_rfc3339,
};
pub use ws::{
    Opcode, TungsteniteTransport, WsFrame, WsTransport, decode_frame, encode_client_text_frame,
    encode_close_frame, encode_server_text_frame,
};
