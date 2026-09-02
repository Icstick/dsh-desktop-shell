//! Shared helpers for daemon integration tests: a minimal envelope client
//! (negotiate + invoke with correlation checks) and a daemon serve loop.
//!
//! Since M6-C1 the client tolerates asynchronous terminal output Events
//! interleaving with invocation Results (the daemon pushes events through
//! a per-connection writer thread): Events received while waiting for a
//! Result are buffered and replayed by `wait_for_output`.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dsh_daemon::envelope::{
    AgreementPayload, Envelope, EnvelopeKind, HelloPayload, PROTOCOL, Participant,
    ProtocolCoordinate, UnavailableCapability, new_message_id, now_timestamp, validate_envelope,
};
use dsh_daemon::server::DaemonServer;
use dsh_local_transport::{Credential, Limits, LocalClient};

/// Negotiation outcome (mirrors the example client AgreementInfo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementInfo {
    pub activation_id: String,
    pub granted: Vec<ProtocolCoordinate>,
    pub unavailable: Vec<UnavailableCapability>,
    pub agreement_id: String,
}

/// A tiny envelope client for tests: real wire, correlation checks on
/// every Result (replyTo + error.correlationId must match the Invocation
/// id — semantics.ts correlation-match).
pub struct TestClient {
    transport: LocalClient,
    participant: Participant,
    generation: u64,
    activation: Option<AgreementInfo>,
    /// Event envelopes received while waiting for invocation Results
    /// (asynchronous terminal output interleaves with Results).
    events: Vec<Envelope>,
}

impl TestClient {
    pub fn connect(addr: SocketAddr, credential: &Credential) -> Self {
        Self::connect_as(addr, credential, "dsh-desktop-shell", "test-client")
    }

    /// Connect with an explicit participant identity (the daemon broker
    /// owns grants per `component|facet`; agent-authorization tests need
    /// a distinct agent identity).
    pub fn connect_as(
        addr: SocketAddr,
        credential: &Credential,
        component: &str,
        facet: &str,
    ) -> Self {
        Self {
            transport: LocalClient::connect(addr, credential, &Limits::default()).expect("connect"),
            participant: Participant {
                component: component.into(),
                facet: facet.into(),
                activation_id: None,
            },
            generation: 0,
            activation: None,
            events: Vec::new(),
        }
    }

    // Used by a subset of test binaries (each integration test compiles
    // this module separately), so allow dead_code.
    #[allow(dead_code)]
    pub fn activation_id(&self) -> Option<&str> {
        self.activation.as_ref().map(|a| a.activation_id.as_str())
    }

    /// Events received so far (buffered during invokes and output waits).
    #[allow(dead_code)]
    pub fn events(&self) -> &[Envelope] {
        &self.events
    }

    /// Wait for the next envelope Event whose method matches `method`
    /// (buffered or live); returns None after `timeout`. Non-Event frames
    /// are not expected while waiting (no invocation is in flight).
    #[allow(dead_code)]
    pub fn wait_for_event(&mut self, method: &str, timeout: Duration) -> Option<Envelope> {
        self.wait_for_event_matching(method, |_| true, timeout)
    }

    /// Wait for the next envelope Event whose method matches `method` and
    /// satisfies `matches` (buffered or live); returns None after
    /// `timeout`.
    #[allow(dead_code)]
    pub fn wait_for_event_matching(
        &mut self,
        method: &str,
        matches: impl Fn(&Envelope) -> bool,
        timeout: Duration,
    ) -> Option<Envelope> {
        let deadline = Instant::now() + timeout;
        let mut index = 0;
        while index < self.events.len() {
            if self.events[index].method.as_deref() == Some(method) && matches(&self.events[index])
            {
                return Some(self.events.remove(index));
            }
            index += 1;
        }
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let tick = remaining.min(Duration::from_millis(200));
            match self.transport.recv_timeout(tick) {
                Ok(Some(bytes)) => {
                    let envelope: Envelope = serde_json::from_slice(&bytes).expect("event frame");
                    match envelope.kind {
                        EnvelopeKind::Event => {
                            validate_envelope(&envelope).expect("Event must be frame-valid");
                            if envelope.method.as_deref() == Some(method) && matches(&envelope) {
                                return Some(envelope);
                            }
                            self.events.push(envelope);
                        }
                        other => {
                            panic!("unexpected envelope kind {other:?} while waiting for event")
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => panic!("recv error while waiting for event: {error}"),
            }
        }
    }

    pub fn negotiate(&mut self, supports: Vec<ProtocolCoordinate>) -> AgreementInfo {
        let payload = serde_json::to_value(HelloPayload {
            instance_id: format!(
                "test-client-{:016x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ),
            supports,
            requires: Vec::new(),
        })
        .expect("hello serializes");
        let hello = self.outgoing(EnvelopeKind::Hello, None, None, Some(payload));
        self.transport.send_json(&hello).expect("send hello");
        // Events are pushed asynchronously by the daemon writer thread and
        // may arrive before the Agreement (PTY output ordering differs by
        // platform); buffer them and keep reading until the Agreement.
        let reply = loop {
            match self.transport.recv_json::<Envelope>() {
                Ok(Some(envelope)) if envelope.kind == EnvelopeKind::Event => {
                    self.events.push(envelope);
                }
                Ok(Some(envelope)) => break envelope,
                Ok(None) => panic!("Agreement: connection closed"),
                Err(error) => panic!("Agreement: recv error {error}"),
            }
        };
        assert_eq!(reply.kind, EnvelopeKind::Agreement, "expected Agreement");
        validate_envelope(&reply).expect("Agreement must be frame-valid");
        assert_eq!(
            reply.reply_to.as_deref(),
            Some(hello.id.as_str()),
            "Agreement must answer the Hello"
        );
        let payload: AgreementPayload = reply
            .payload
            .clone()
            .and_then(|p| serde_json::from_value(p).ok())
            .expect("agreementPayload shape");
        let info = AgreementInfo {
            activation_id: payload.activation_id.clone(),
            granted: payload.granted.clone(),
            unavailable: payload.unavailable.clone(),
            agreement_id: reply.id.clone(),
        };
        self.participant.activation_id = Some(payload.activation_id);
        self.activation = Some(info.clone());
        info
    }

    pub fn invoke(
        &mut self,
        capability: ProtocolCoordinate,
        method: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, RemoteError> {
        let activation_id = self
            .activation
            .as_ref()
            .map(|a| a.activation_id.clone())
            .expect("negotiate before invoke");
        self.invoke_with(activation_id, capability, method, payload)
    }

    /// Invoke under an explicit activation id (stale-activation tests).
    #[allow(dead_code)]
    pub fn invoke_as(
        &mut self,
        activation_id: &str,
        capability: ProtocolCoordinate,
        method: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, RemoteError> {
        self.invoke_with(activation_id.to_string(), capability, method, payload)
    }

    fn invoke_with(
        &mut self,
        activation_id: String,
        capability: ProtocolCoordinate,
        method: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, RemoteError> {
        self.participant.activation_id = Some(activation_id);
        let invocation = self.outgoing(
            EnvelopeKind::Invocation,
            Some(capability),
            Some(method.to_string()),
            Some(payload),
        );
        self.transport
            .send_json(&invocation)
            .expect("send invocation");
        loop {
            let envelope = self.recv_expect("Result");
            match envelope.kind {
                EnvelopeKind::Result => {
                    validate_envelope(&envelope).expect("Result must be frame-valid");
                    assert_eq!(
                        envelope.reply_to.as_deref(),
                        Some(invocation.id.as_str()),
                        "replyTo must match"
                    );
                    if let Some(error) = envelope.error {
                        assert_eq!(
                            error.correlation_id, invocation.id,
                            "correlationId must match"
                        );
                        return Err(RemoteError {
                            code: error.code,
                            message: error.message,
                            retryable: error.retryable,
                        });
                    }
                    return Ok(envelope.payload.unwrap_or_else(|| serde_json::json!({})));
                }
                EnvelopeKind::Event => {
                    // Async terminal output interleaves with Results;
                    // buffer it for later assertions (M6-C1).
                    validate_envelope(&envelope).expect("Event must be frame-valid");
                    self.events.push(envelope);
                }
                other => panic!("unexpected envelope kind {other:?} while waiting for the Result"),
            }
        }
    }

    /// Replay buffered + receive live events until `needle` appears in the
    /// concatenated event data or `timeout` elapses (the daemon pushes
    /// terminal output events asynchronously; the Result of the invoking
    /// write arrives before the echo). Returns everything seen, buffered
    /// or live — callers assert on both presence and absence.
    #[allow(dead_code)]
    pub fn wait_for_output(&mut self, needle: &str, timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        let mut seen = String::new();
        let mut index = 0;
        while index < self.events.len() {
            if let Some(data) = event_data(&self.events[index]) {
                seen.push_str(&data);
                if seen.contains(needle) {
                    return seen;
                }
            }
            index += 1;
        }
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return seen;
            }
            let tick = remaining.min(Duration::from_millis(200));
            match self.transport.recv_timeout(tick) {
                Ok(Some(bytes)) => {
                    let envelope: Envelope = serde_json::from_slice(&bytes).expect("event frame");
                    match envelope.kind {
                        EnvelopeKind::Event => {
                            validate_envelope(&envelope).expect("Event must be frame-valid");
                            if let Some(data) = event_data(&envelope) {
                                seen.push_str(&data);
                            }
                            self.events.push(envelope);
                            if seen.contains(needle) {
                                return seen;
                            }
                        }
                        other => {
                            panic!("unexpected envelope kind {other:?} while waiting for output")
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => panic!("recv error while waiting for output: {error}"),
            }
        }
    }

    fn outgoing(
        &mut self,
        kind: EnvelopeKind,
        capability: Option<ProtocolCoordinate>,
        method: Option<String>,
        payload: Option<serde_json::Value>,
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
            error: None,
        };
        self.generation += 1;
        envelope
    }

    fn recv_expect(&mut self, what: &str) -> Envelope {
        self.transport
            .recv_json::<Envelope>()
            .expect("recv")
            .unwrap_or_else(|| panic!("{what}: connection closed"))
    }
}

#[allow(dead_code)]
/// Event payload `data` field (terminal output chunks).
fn event_data(envelope: &Envelope) -> Option<String> {
    envelope
        .payload
        .as_ref()
        .and_then(|payload| payload.get("data"))
        .and_then(|data| data.as_str())
        .map(String::from)
}

/// Remote protocol error Result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteError {
    pub code: dsh_daemon::envelope::ErrorCode,
    pub message: String,
    pub retryable: bool,
}

/// Bind a daemon server and move it into a serving thread; returns the
/// envelope addr, a fresh one-time credential and the shared server
/// handle (to issue further credentials — local-transport credentials are
/// single-use, AC-IPC-001). The Managed runtime host resolves
/// environments from the default (real user) catalog path, which the
/// runtime integration tests never touch — they use
/// spawn_daemon_with_catalog.
#[allow(dead_code)]
pub fn spawn_daemon() -> (SocketAddr, Credential, Arc<DaemonServer>) {
    let server = Arc::new(
        DaemonServer::bind(Limits::default(), dsh_daemon::credential::CLAIM_PORT)
            .expect("bind daemon server"),
    );
    let addr = server.addr();
    let credential = server.issue_credential(Duration::from_secs(300));
    let serve_server = Arc::clone(&server);
    thread::spawn(move || serve_loop(serve_server));
    (addr, credential, server)
}

/// Like spawn_daemon but with an explicit environment-catalog path
/// (M6-C2 runtime integration tests isolate the catalog in a temp
/// directory).
#[allow(dead_code)]
pub fn spawn_daemon_with_catalog(
    catalog_path: std::path::PathBuf,
) -> (SocketAddr, Credential, Arc<DaemonServer>) {
    let server = Arc::new(
        DaemonServer::bind_with_catalog(
            Limits::default(),
            dsh_daemon::credential::CLAIM_PORT,
            catalog_path,
        )
        .expect("bind daemon server"),
    );
    let addr = server.addr();
    let credential = server.issue_credential(Duration::from_secs(300));
    let serve_server = Arc::clone(&server);
    thread::spawn(move || serve_loop(serve_server));
    (addr, credential, server)
}

fn serve_loop(server: Arc<DaemonServer>) {
    let mut served: HashSet<u64> = HashSet::new();
    loop {
        for conn in server.connections() {
            if served.insert(conn.id()) {
                let server = Arc::clone(&server);
                thread::spawn(move || server.serve_connection(conn));
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}
