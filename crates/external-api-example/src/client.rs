//! Envelope client: negotiate (Hello → Agreement) then invoke capabilities
//! over a `dsh-local-transport` connection.
//!
//! The client mirrors the server's authorization posture on the receiving
//! side: a Result is only accepted when it correlates with the pending
//! Invocation — `replyTo` must equal the Invocation id and, on the
//! error branch, `error.correlationId` must too (semantics.ts
//! `result-target` + `correlation-match`). Mismatches are
//! rejected with `ClientError::CorrelationMismatch`.

use std::fmt;
use std::net::SocketAddr;

use dsh_local_transport::{Credential, Limits, LocalClient, TransportError};

use crate::envelope::{
    AgreementPayload, Envelope, EnvelopeKind, ErrorCode, HelloPayload, PROTOCOL, Participant,
    ProtocolCoordinate, UnavailableCapability, new_message_id, now_timestamp, validate_envelope,
};

/// Result of a successful negotiation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementInfo {
    pub activation_id: String,
    pub granted: Vec<ProtocolCoordinate>,
    pub unavailable: Vec<UnavailableCapability>,
    pub agreement_id: String,
}

/// Client-side failures.
#[derive(Debug)]
pub enum ClientError {
    /// Transport-level failure (connect, send, recv, serialization).
    Transport(TransportError),
    /// `invoke` called before a successful `negotiate`.
    NotNegotiated,
    /// The peer sent a frame that fails envelope validation.
    InvalidPeerFrame(String),
    /// A Result that does not correlate with the pending Invocation
    /// (`replyTo` or `error.correlationId` mismatch).
    CorrelationMismatch { expected: String, got: String },
    /// The peer returned a protocol error Result.
    Remote {
        code: ErrorCode,
        message: String,
        retryable: bool,
    },
    /// The peer answered with an unexpected envelope kind.
    UnexpectedKind(EnvelopeKind),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(f, "transport: {error}"),
            Self::NotNegotiated => write!(f, "invoke before negotiate"),
            Self::InvalidPeerFrame(message) => write!(f, "invalid peer frame: {message}"),
            Self::CorrelationMismatch { expected, got } => {
                write!(
                    f,
                    "correlation mismatch: expected \"{expected}\", got \"{got}\""
                )
            }
            Self::Remote {
                code,
                message,
                retryable,
            } => write!(f, "remote error {code} (retryable={retryable}): {message}"),
            Self::UnexpectedKind(kind) => write!(f, "unexpected envelope kind {kind:?}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TransportError> for ClientError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

/// The reference external-API client.
#[derive(Debug)]
pub struct ExampleClient {
    transport: LocalClient,
    instance_id: String,
    participant: Participant,
    generation: u64,
    activation: Option<AgreementInfo>,
}

impl ExampleClient {
    /// Connect and authenticate with a one-time credential.
    pub fn connect(
        addr: SocketAddr,
        credential: &Credential,
        limits: &Limits,
    ) -> Result<Self, TransportError> {
        let instance_id = format!(
            "ext-tool-{:016x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        Ok(Self {
            transport: LocalClient::connect(addr, credential, limits)?,
            instance_id,
            participant: Participant {
                component: "external-tool".into(),
                facet: "example-client".into(),
                activation_id: None,
            },
            generation: 0,
            activation: None,
        })
    }

    /// The negotiated activation, if any.
    pub fn activation(&self) -> Option<&AgreementInfo> {
        self.activation.as_ref()
    }

    /// The negotiated activation id, if any.
    pub fn activation_id(&self) -> Option<&str> {
        self.activation.as_ref().map(|a| a.activation_id.as_str())
    }

    /// Negotiate an activation: send `Hello` (advertising
    /// `supports`), expect `Agreement` back. The server
    /// decides the `granted` subset (minimal grant).
    pub fn negotiate(
        &mut self,
        supports: Vec<ProtocolCoordinate>,
    ) -> Result<AgreementInfo, ClientError> {
        let payload = serde_json::to_value(HelloPayload {
            instance_id: self.instance_id.clone(),
            supports,
            requires: Vec::new(),
        })
        .expect("hello payload serializes");
        let hello = self.outgoing(EnvelopeKind::Hello, None, None, Some(payload), None);
        self.send(&hello)?;

        let Some(reply) = self.recv()? else {
            return Err(ClientError::Transport(TransportError::Closed));
        };
        if reply.kind != EnvelopeKind::Agreement {
            return Err(ClientError::UnexpectedKind(reply.kind));
        }
        if let Err(issues) = validate_envelope(&reply) {
            return Err(ClientError::InvalidPeerFrame(format!(
                "Agreement failed frame validation: {}",
                issues
                    .iter()
                    .map(|issue| format!("{}: {}", issue.path, issue.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        if reply.reply_to.as_deref() != Some(hello.id.as_str()) {
            return Err(ClientError::CorrelationMismatch {
                expected: hello.id.clone(),
                got: reply.reply_to.clone().unwrap_or_default(),
            });
        }
        let Some(payload) = reply
            .payload
            .clone()
            .and_then(|payload| serde_json::from_value::<AgreementPayload>(payload).ok())
        else {
            return Err(ClientError::InvalidPeerFrame(
                "Agreement payload is not a valid agreementPayload".into(),
            ));
        };
        let info = AgreementInfo {
            activation_id: payload.activation_id.clone(),
            granted: payload.granted.clone(),
            unavailable: payload.unavailable.clone(),
            agreement_id: reply.id.clone(),
        };
        self.participant.activation_id = Some(payload.activation_id);
        self.activation = Some(info.clone());
        Ok(info)
    }

    /// Invoke one granted capability and await its Result.
    ///
    /// The Result is checked against the pending Invocation before it is
    /// surfaced: wrong `replyTo` or (on the error branch) wrong
    /// `error.correlationId` → `CorrelationMismatch`.
    pub fn invoke(
        &mut self,
        capability: ProtocolCoordinate,
        method: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        if self.activation.is_none() {
            return Err(ClientError::NotNegotiated);
        }
        let invocation = self.outgoing(
            EnvelopeKind::Invocation,
            Some(capability),
            Some(method.to_string()),
            Some(payload),
            None,
        );
        self.send(&invocation)?;
        let Some(reply) = self.recv()? else {
            return Err(ClientError::Transport(TransportError::Closed));
        };
        if reply.kind != EnvelopeKind::Result {
            return Err(ClientError::UnexpectedKind(reply.kind));
        }
        if let Err(issues) = validate_envelope(&reply) {
            return Err(ClientError::InvalidPeerFrame(format!(
                "Result failed frame validation: {}",
                issues
                    .iter()
                    .map(|issue| format!("{}: {}", issue.path, issue.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        self.verify_result(&invocation, &reply)?;
        match reply.error {
            Some(error) => Err(ClientError::Remote {
                code: error.code,
                message: error.message,
                retryable: error.retryable,
            }),
            None => Ok(reply.payload.unwrap_or_else(|| serde_json::json!({}))),
        }
    }

    /// Correlation check for a Result against the Invocation it answers
    /// (semantics.ts `result-target` + `correlation-match`).
    /// Exposed so tools can verify results on any transport path.
    pub fn verify_result(
        &self,
        invocation: &Envelope,
        result: &Envelope,
    ) -> Result<(), ClientError> {
        if result.reply_to.as_deref() != Some(invocation.id.as_str()) {
            return Err(ClientError::CorrelationMismatch {
                expected: invocation.id.clone(),
                got: result.reply_to.clone().unwrap_or_default(),
            });
        }
        if let Some(error) = &result.error
            && error.correlation_id != invocation.id
        {
            return Err(ClientError::CorrelationMismatch {
                expected: invocation.id.clone(),
                got: error.correlation_id.clone(),
            });
        }
        Ok(())
    }

    fn outgoing(
        &mut self,
        kind: EnvelopeKind,
        capability: Option<ProtocolCoordinate>,
        method: Option<String>,
        payload: Option<serde_json::Value>,
        error: Option<crate::envelope::ProtocolError>,
    ) -> Envelope {
        let envelope = Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind,
            reply_to: None,
            participant: self.participant.clone(),
            timestamp: now_timestamp(),
            generation: self.generation,
            capability,
            method,
            payload,
            error,
        };
        self.generation += 1;
        envelope
    }

    fn send(&mut self, envelope: &Envelope) -> Result<(), ClientError> {
        self.transport.send_json(envelope)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Envelope>, ClientError> {
        self.transport
            .recv_json::<Envelope>()
            .map_err(ClientError::from)
    }
}
