//! Envelope server: negotiates activations and dispatches Invocations over
//! a `dsh-local-transport` connection.
//!
//! Authorization model (ADR-0018 decision 5, minimal grant):
//!
//! 1. an Invocation is only accepted under an activation negotiated on the
//!    same connection (Hello → Agreement); without one → UNAUTHORIZED;
//! 2. Invocation.capability must be in the activation's `granted`
//!    set; otherwise → UNAUTHORIZED;
//! 3. every error Result echoes the Invocation id as
//!    `error.correlationId` (semantics.ts `correlation-match`),
//!    and the client-side counterpart rejects mismatches.
//!
//! The example policy grants `system.ping` and denies
//! `browser.list_browsers` (policy_denied, degrade path) — the
//! browser dispatch handler exists so a policy that grants it works too.

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use dsh_local_transport::{Credential, Limits, LocalServer, ServerConn};

use crate::catalog::{
    BROWSER_API_VERSION, BROWSER_KIND, BROWSER_LIST_METHOD, SYSTEM_API_VERSION, SYSTEM_KIND,
    SYSTEM_PING_METHOD,
};
use crate::envelope::{
    AgreementPayload, Envelope, EnvelopeKind, ErrorCode, HelloPayload, ID_MAX_LEN, ID_MIN_LEN,
    PROTOCOL, Participant, ProtocolCoordinate, ProtocolError, UnavailableCapability,
    UnavailableReason, new_activation_id, new_message_id, now_timestamp, validate_envelope,
};

/// Server-side identity used in every envelope the server sends.
pub const SERVER_COMPONENT: &str = "dsh-desktop-shell";
pub const SERVER_FACET: &str = "external-api-example";

/// One negotiated activation on a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub activation_id: String,
    pub granted: Vec<ProtocolCoordinate>,
    pub hello_id: String,
}

/// Per-connection protocol state: negotiated activations, seen message ids
/// (id-replay rejection) and the server's own generation counter.
#[derive(Debug, Default)]
pub struct SessionState {
    pub activations: HashMap<String, Activation>,
    seen_ids: HashSet<String>,
    next_generation: u64,
}

impl SessionState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Which capabilities the server is willing to grant (policy).
///
/// Default: only `system` (the `ping` health check) is
/// grantable; everything else a peer advertises lands in
/// `unavailable[].reason = policy_denied`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantPolicy {
    pub grantable: Vec<ProtocolCoordinate>,
}

impl Default for GrantPolicy {
    fn default() -> Self {
        Self {
            grantable: vec![ProtocolCoordinate {
                api_version: SYSTEM_API_VERSION.into(),
                kind: SYSTEM_KIND.into(),
            }],
        }
    }
}

/// The reference external-API server: local-transport endpoint + envelope
/// negotiation/dispatch + minimal authorization.
#[derive(Debug)]
pub struct ExampleServer {
    transport: LocalServer,
    policy: GrantPolicy,
}

impl ExampleServer {
    /// Bind a server with the default grant policy (system.ping only).
    pub fn bind(limits: Limits) -> io::Result<Self> {
        Self::bind_with_policy(limits, GrantPolicy::default())
    }

    /// Bind a server with an explicit grant policy.
    pub fn bind_with_policy(limits: Limits, policy: GrantPolicy) -> io::Result<Self> {
        Ok(Self {
            transport: LocalServer::bind(limits)?,
            policy,
        })
    }

    /// The bound loopback address external tools connect to.
    pub fn addr(&self) -> SocketAddr {
        self.transport.addr()
    }

    /// Issue a one-time ephemeral credential (local-transport auth).
    pub fn issue_credential(&self, ttl: Duration) -> Credential {
        self.transport.issue_credential(ttl)
    }

    /// Take one currently authenticated connection, if any (poll).
    pub fn take_connection(&self) -> Option<ServerConn> {
        self.transport.connections().into_iter().next()
    }

    /// Serve one authenticated connection until the peer disconnects.
    pub fn serve_connection(&self, conn: ServerConn) {
        let mut state = SessionState::new();
        while let Some(bytes) = conn.recv() {
            let envelope = match serde_json::from_slice::<Envelope>(&bytes) {
                Ok(envelope) => envelope,
                Err(error) => {
                    // Unparseable frame (bad JSON, unknown field, negative
                    // generation): reply MALFORMED_MESSAGE when the id lets
                    // us correlate, otherwise drop and keep serving.
                    let id = serde_json::from_slice::<serde_json::Value>(&bytes)
                        .ok()
                        .and_then(|value| {
                            value.get("id").and_then(|id| id.as_str()).map(String::from)
                        })
                        .filter(|id| (ID_MIN_LEN..=ID_MAX_LEN).contains(&id.len()));
                    if let Some(id) = id {
                        let reply = self.error_result(
                            &mut state,
                            &id,
                            &id,
                            None,
                            None,
                            ErrorCode::MalformedMessage,
                            &format!("frame is not a valid envelope: {error}"),
                            false,
                        );
                        if conn.send_json(&reply).is_err() {
                            return;
                        }
                    }
                    continue;
                }
            };
            for reply in self.handle_envelope(&mut state, envelope) {
                if conn.send_json(&reply).is_err() {
                    return;
                }
            }
        }
    }

    /// Handle one validated envelope against session state; returns the
    /// envelopes to send back. Pure (no I/O) so tests can drive the
    /// protocol directly.
    pub fn handle_envelope(&self, state: &mut SessionState, envelope: Envelope) -> Vec<Envelope> {
        if !state.seen_ids.insert(envelope.id.clone()) {
            return vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                envelope.capability.as_ref(),
                envelope.method.as_deref(),
                ErrorCode::MalformedMessage,
                &format!(
                    "message id \"{}\" already used on this connection (replay)",
                    envelope.id
                ),
                false,
            )];
        }
        if let Err(issues) = validate_envelope(&envelope) {
            let message = issues
                .iter()
                .map(|issue| format!("{}: {}", issue.path, issue.message))
                .collect::<Vec<_>>()
                .join("; ");
            if (ID_MIN_LEN..=ID_MAX_LEN).contains(&envelope.id.len()) {
                return vec![self.error_result(
                    state,
                    &envelope.id,
                    &envelope.id,
                    envelope.capability.as_ref(),
                    envelope.method.as_deref(),
                    ErrorCode::MalformedMessage,
                    &format!("envelope validation failed: {message}"),
                    false,
                )];
            }
            return vec![];
        }
        match envelope.kind {
            EnvelopeKind::Hello => self.handle_hello(state, envelope),
            EnvelopeKind::Invocation => self.handle_invocation(state, envelope),
            // Agreement/Result/Event from the peer are validated and then
            // ignored: the example server never sends Invocations, and
            // Events are asynchronous by design.
            _ => vec![],
        }
    }

    fn handle_hello(&self, state: &mut SessionState, envelope: Envelope) -> Vec<Envelope> {
        let Some(hello) = envelope
            .payload
            .clone()
            .and_then(|payload| serde_json::from_value::<HelloPayload>(payload).ok())
        else {
            // Frame validation already verified the shape; unreachable.
            return vec![];
        };

        let activation_id = new_activation_id();
        let mut granted = Vec::new();
        let mut unavailable = Vec::new();
        for support in &hello.supports {
            if self.policy.grantable.contains(support) {
                granted.push(support.clone());
            } else {
                unavailable.push(UnavailableCapability {
                    coordinate: support.clone(),
                    reason: UnavailableReason::PolicyDenied,
                });
            }
        }
        state.activations.insert(
            activation_id.clone(),
            Activation {
                activation_id: activation_id.clone(),
                granted: granted.clone(),
                hello_id: envelope.id.clone(),
            },
        );

        let mut participant = self.participant(None);
        participant.activation_id = Some(activation_id.clone());
        let generation = state.next_generation;
        state.next_generation += 1;
        vec![Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind: EnvelopeKind::Agreement,
            reply_to: Some(envelope.id.clone()),
            participant,
            timestamp: now_timestamp(),
            generation,
            capability: None,
            method: None,
            payload: Some(
                serde_json::to_value(AgreementPayload {
                    activation_id,
                    granted,
                    unavailable,
                    lease_constraints: None,
                })
                .expect("agreement payload serializes"),
            ),
            error: None,
        }]
    }

    fn handle_invocation(&self, state: &mut SessionState, envelope: Envelope) -> Vec<Envelope> {
        let capability = envelope.capability.clone().expect("validated Invocation");
        let method = envelope.method.clone().expect("validated Invocation");

        // 1) Activation required (no Agreement → UNAUTHORIZED).
        let Some(activation_id) = envelope.participant.activation_id.clone() else {
            return vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                Some(&capability),
                Some(&method),
                ErrorCode::Unauthorized,
                "Invocation without an Agreement: participant.activationId is missing",
                false,
            )];
        };
        let Some(activation) = state.activations.get(&activation_id) else {
            return vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                Some(&capability),
                Some(&method),
                ErrorCode::Unauthorized,
                &format!("no Agreement for activation \"{activation_id}\": negotiate Hello → Agreement first"),
                false,
            )];
        };
        // 2) Capability must be granted to this activation.
        if !activation.granted.contains(&capability) {
            return vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                Some(&capability),
                Some(&method),
                ErrorCode::Unauthorized,
                &format!(
                    "capability {}/{} is not granted by the Agreement for activation \"{activation_id}\"",
                    capability.api_version, capability.kind
                ),
                false,
            )];
        }

        // 3) Dispatch.
        let result = match (
            capability.api_version.as_str(),
            capability.kind.as_str(),
            method.as_str(),
        ) {
            (SYSTEM_API_VERSION, SYSTEM_KIND, SYSTEM_PING_METHOD) => {
                let echo = envelope
                    .payload
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({}));
                Ok(serde_json::json!({ "pong": true, "echo": echo }))
            }
            (BROWSER_API_VERSION, BROWSER_KIND, BROWSER_LIST_METHOD) => Ok(serde_json::json!({
                "browsers": [
                    { "name": "edge", "kind": "webview2", "status": "available" },
                    { "name": "chrome", "kind": "cdp", "status": "not-configured" },
                ]
            })),
            _ => Err(format!(
                "method \"{method}\" is not implemented for {}/{}",
                capability.api_version, capability.kind
            )),
        };

        match result {
            Ok(payload) => {
                let generation = state.next_generation;
                state.next_generation += 1;
                vec![Envelope {
                    protocol: PROTOCOL.into(),
                    id: new_message_id(),
                    kind: EnvelopeKind::Result,
                    reply_to: Some(envelope.id.clone()),
                    participant: self.participant(Some(activation_id)),
                    timestamp: now_timestamp(),
                    generation,
                    capability: Some(capability),
                    method: Some(method),
                    payload: Some(payload),
                    error: None,
                }]
            }
            Err(message) => vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                Some(&capability),
                Some(&method),
                ErrorCode::Unavailable,
                &message,
                false,
            )],
        }
    }

    /// Build an error Result. The correlationId always echoes the id of the
    /// message being answered (semantics.ts `correlation-match`).
    #[allow(clippy::too_many_arguments)]
    fn error_result(
        &self,
        state: &mut SessionState,
        correlation_id: &str,
        reply_to: &str,
        capability: Option<&ProtocolCoordinate>,
        method: Option<&str>,
        code: ErrorCode,
        message: &str,
        retryable: bool,
    ) -> Envelope {
        let generation = state.next_generation;
        state.next_generation += 1;
        Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind: EnvelopeKind::Result,
            reply_to: Some(reply_to.to_string()),
            participant: self.participant(None),
            timestamp: now_timestamp(),
            generation,
            capability: capability.cloned(),
            method: method.map(String::from),
            payload: None,
            error: Some(ProtocolError {
                code,
                message: message.chars().take(512).collect(),
                retryable,
                correlation_id: correlation_id.to_string(),
            }),
        }
    }

    fn participant(&self, activation_id: Option<String>) -> Participant {
        Participant {
            component: SERVER_COMPONENT.into(),
            facet: SERVER_FACET.into(),
            activation_id,
        }
    }
}
