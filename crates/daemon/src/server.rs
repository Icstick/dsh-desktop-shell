//! Envelope server of the daemon (ADR-0019 decision 5).
//!
//! Ported from `crates/external-api-example/src/server.rs` (the M5-B2
//! reference closed loop) with its test semantics kept, and the static
//! `GrantPolicy` replaced by the **broker-driven** authorization chain
//! (ADR-0018 decision 7 / M5-E1, `crates/supervisor`):
//!
//! - `Hello` → `Broker::broker_grant_from_negotiation`: the negotiated
//!   capabilities become broker grants + bounded leases owned by the
//!   participant (agent id = `component|facet`), with the ADR-0018
//!   activation semantics (fresh activation supersedes the previous one,
//!   generation change revokes the old leases).
//! - `Invocation` → `Broker::enforce_dispatch` (the ADR-0014 gate:
//!   grant, owner, generation, scope, valid lease) before the capability
//!   handler in [`crate::capabilities`] runs.
//!
//! M6-C1: the daemon **really hosts the terminal capability** — the PTY
//! registry (`crate::terminal`) plus the daemon-internal event router
//! (`crate::events`, M6-B1 TODO⑤): output events flow registry →
//! bridge thread → router → per-connection subscriber → wire, addressed
//! by session id (never crossing sessions/connections).
//!
//! M6-C3: the daemon **really hosts the browser session state** — the
//! browser `SessionRegistry` (`crate::browser`, ADR-0019 decision 2:
//! state authority in the daemon, rendering in the Shell) with lifecycle
//! events (`browser.session-created` / `browser.session-closed`) pushed
//! through the same router as envelope Events.
//!
//! M6-C2: the daemon **really hosts the Managed DSH runtime** — the DSH
//! process tree (`crate::runtime`, ADR-0019 decision 3) with the
//! environment catalog read from the daemon data directory; the Shell
//! talks to it through the `runtime.*` envelope methods.
//!
//! Remaining M6-C/D TODOs:
//! - connection-scoped lease revocation on disconnect (broker `revoke`
//!   with `LeaseRevocationReason::Disconnect`) — TODO(M6-C)
//! - fixed-port envelope bind: `dsh-local-transport` only binds a random
//!   loopback port; the daemon owns the fixed claim port 37771 and the
//!   real port travels in the credential file. — TODO(M6-C)

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use dsh_local_transport::{Credential, Limits, LocalServer, ServerConn};
use dsh_supervisor::{
    AgentBridgeError, AgentConformanceState, AgentLeaseConstraints, AgentNegotiationResult, Broker,
    BrokerError, CapabilityId, Scope, SystemClock,
};

use crate::browser::{BrowserEventPayload, BrowserHost};
use crate::capabilities::{
    BROWSER_API_VERSION, BROWSER_KIND, CapabilityContext, DaemonMethodError, DaemonStatusSnapshot,
    TERMINAL_API_VERSION, TERMINAL_KIND, dispatch as dispatch_capability,
    supports as catalog_supports,
};
use crate::credential::CredentialFile;

use crate::envelope::{
    AgreementPayload, Envelope, EnvelopeKind, ErrorCode, HelloPayload, ID_MAX_LEN, ID_MIN_LEN,
    PROTOCOL, Participant, ProtocolCoordinate, ProtocolError, UnavailableCapability,
    UnavailableReason, new_activation_id, new_message_id, now_timestamp, validate_envelope,
};
use crate::events::{EventRouter, RouterEvent};
use crate::runtime::{ManagedRuntimeHost, default_catalog_path};
use crate::scheduler::Scheduler;
use crate::terminal::{
    EVENT_DRAIN_INTERVAL, TERMINAL_OUTPUT_EVENT, TerminalHost, TerminalOutputEvent, now_unix_ms,
};

/// Server-side identity used in every envelope the daemon sends.
pub const SERVER_COMPONENT: &str = "dsh-desktop-shell";
pub const SERVER_FACET: &str = "daemon";

/// The human Shell participant — the only identity allowed on the
/// broker-relaxed path (REVIEW-M6-DAEMON HIGH-2): a credential-
/// authenticated Shell negotiation that conflicts with the single-owner
/// broker grant still succeeds at the protocol level, because the human
/// operator owns the daemon; any other participant stays fail-closed.
pub const SHELL_COMPONENT: &str = "dsh-desktop-shell";
pub const SHELL_FACET: &str = "shell";

/// Default lease offered on negotiation (seconds). The broker derives
/// `expires_at = now + max_seconds`; the daemon re-negotiates per
/// connection (ADR-0018 decision 1, no Agreement caching).
pub const LEASE_MAX_SECONDS: u64 = 3600;

/// One negotiated activation on a connection (session-layer view; the
/// broker holds the authoritative grant/lease state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub activation_id: String,
    /// Participant identity the broker owns the grants for
    /// (`component-facet`; the schema-compatible form of the envelope
    /// participant, M6-C1 — the agentId pattern `^[A-Za-z0-9_-]+$`
    /// forbids the `|` separator).
    pub agent_id: String,
    /// Broker generation of this activation.
    pub generation: u64,
    pub granted: Vec<ProtocolCoordinate>,
    pub hello_id: String,
}

/// Per-connection protocol state: negotiated activations, seen message ids
/// (id-replay rejection) and the server generation counter.
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

/// The daemon envelope server: local-transport endpoint + envelope
/// negotiation/dispatch + broker-driven authorization.
///
/// No `Debug` derive: the broker (`dsh_supervisor::Broker`) is not `Debug`.
pub struct DaemonServer {
    transport: LocalServer,
    broker: Arc<Mutex<Broker<SystemClock>>>,
    scheduler: Arc<Scheduler>,
    /// Daemon-owned PTY host (M6-C1).
    terminal: Arc<TerminalHost>,
    /// Directory the credential file is (re)written into. The startup
    /// credential is consumed by the first handshake; after every
    /// disconnect the daemon re-issues and atomically rewrites the file so
    /// a Shell restart can re-attach to the surviving daemon (M6 core
    /// semantics; REVIEW-M6-DAEMON HIGH-1).
    credential_dir: Option<PathBuf>,
    /// Daemon-owned browser session host (M6-C3).
    browser: Arc<BrowserHost>,
    /// Daemon-owned Managed runtime host (M6-C2: DSH process tree).
    runtime: Arc<ManagedRuntimeHost>,
    /// Daemon event router (M6-B1 TODO⑤, wired in M6-C1).
    events: Arc<EventRouter>,
    claim_port: u16,
    started_at: SystemTime,
}

impl DaemonServer {
    /// Bind the envelope server (dynamic loopback port; the fixed claim
    /// port is owned by the single-instance guard) and build the broker.
    /// Bind the envelope server (dynamic loopback port; the fixed claim
    /// port is owned by the single-instance guard) and build the broker.
    /// The Managed runtime host resolves environments from the default
    /// catalog path (the daemon data directory).
    pub fn bind(limits: Limits, claim_port: u16) -> io::Result<Self> {
        Self::bind_with_catalog(limits, claim_port, default_catalog_path())
    }

    /// Bind with an explicit environment-catalog path (M6-C2: tests
    /// isolate the catalog in a temp directory; the binary uses the
    /// default data-directory catalog).
    pub fn bind_with_catalog(
        limits: Limits,
        claim_port: u16,
        catalog_path: std::path::PathBuf,
    ) -> io::Result<Self> {
        let server = Self {
            transport: LocalServer::bind(limits)?,
            broker: Arc::new(Mutex::new(Broker::<SystemClock>::new())),
            scheduler: Arc::new(Scheduler::new()),
            terminal: Arc::new(TerminalHost::new()),
            credential_dir: catalog_path.parent().map(PathBuf::from),
            browser: Arc::new(BrowserHost::new()),
            runtime: Arc::new(ManagedRuntimeHost::new(catalog_path)),
            events: EventRouter::spawn(),
            claim_port,
            started_at: SystemTime::now(),
        };
        server.start_terminal_event_bridge();
        Ok(server)
    }

    /// The bound loopback address external tools connect to.
    pub fn addr(&self) -> SocketAddr {
        self.transport.addr()
    }

    /// Issue a one-time ephemeral credential (local-transport auth).
    pub fn issue_credential(&self, ttl: Duration) -> Credential {
        self.transport.issue_credential(ttl)
    }

    /// Shared broker handle (observability/tests).
    pub fn broker(&self) -> Arc<Mutex<Broker<SystemClock>>> {
        Arc::clone(&self.broker)
    }

    /// Take one currently authenticated connection, if any (poll).
    pub fn take_connection(&self) -> Option<ServerConn> {
        self.transport.connections().into_iter().next()
    }

    /// All currently authenticated connections (the serve loop dedups by
    /// connection id before spawning per-connection threads).
    pub fn connections(&self) -> Vec<ServerConn> {
        self.transport.connections()
    }

    /// Serve one authenticated connection until the peer disconnects.
    ///
    /// Each connection registers an event subscriber (M6-C1); a dedicated
    /// writer thread drains the subscriber queue onto the wire so the recv
    /// loop stays blocking. On teardown the writer stops and the subscriber
    /// is unregistered: sessions of a dead connection keep running
    /// (resource survival is why the daemon exists, ADR-0008) and their
    /// events stop flowing until a later handover re-subscribes them
    /// (M6-C4).
    pub fn serve_connection(&self, conn: ServerConn) {
        let conn = Arc::new(conn);
        let stop = Arc::new(AtomicBool::new(false));
        let subscriber = self.events.register();
        let connection_key = subscriber.key();

        // Writer thread: queued events → wire (bounded queue; a dead
        // client makes send fail and the writer exits).
        let writer = {
            let conn = Arc::clone(&conn);
            let stop = Arc::clone(&stop);
            let template = self.event_template();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Some(event) = subscriber.recv_timeout(EVENT_WRITER_POLL)
                        && conn.send_json(&template.build(&event)).is_err()
                    {
                        break;
                    }
                }
                subscriber.unsubscribe_all();
            })
        };

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
                            stop.store(true, Ordering::Relaxed);
                            let _ = writer.join();
                            return;
                        }
                    }
                    continue;
                }
            };
            for reply in self.handle_envelope(&mut state, envelope, connection_key) {
                if conn.send_json(&reply).is_err() {
                    stop.store(true, Ordering::Relaxed);
                    let _ = writer.join();
                    return;
                }
            }
        }
        stop.store(true, Ordering::Relaxed);
        let _ = writer.join();
        // Re-issue the bootstrap credential after a disconnect so the next
        // Shell start re-attaches to this surviving daemon (HIGH-1 fix:
        // one-time credentials are consumed by the first handshake; the
        // file must carry a fresh one for the next Shell).
        self.reissue_credential_file();
        // TODO(M6-C): revoke this connection session leases on disconnect
        // (broker `revoke` with `LeaseRevocationReason::Disconnect`); the
        // lease TTL bounds them until then.
    }

    /// Re-issue the bootstrap credential and atomically rewrite the
    /// credential file (best-effort: a failure leaves the previous file,
    /// which the Shell's retry loop re-reads). Only meaningful when the
    /// server was bound with a data directory.
    fn reissue_credential_file(&self) {
        let Some(dir) = self.credential_dir.clone() else {
            return;
        };
        let credential = self
            .transport
            .issue_credential(Duration::from_secs(LEASE_MAX_SECONDS));
        let file = CredentialFile::new(
            crate::DAEMON_VERSION,
            std::process::id(),
            self.claim_port,
            self.transport.addr().port(),
            credential.token(),
            credential.expires_at(),
            SystemTime::now(),
        );
        if let Err(error) = file.write_to(&dir) {
            eprintln!("dsh-daemon: cannot reissue the credential file: {error}");
        }
    }

    /// Handle one validated envelope against session state; returns the
    /// envelopes to send back. Pure (no I/O) so tests can drive the
    /// protocol directly. `connection_key` is the caller event-subscriber
    /// key (session subscriptions and ownership are connection-scoped).
    pub fn handle_envelope(
        &self,
        state: &mut SessionState,
        envelope: Envelope,
        connection_key: u64,
    ) -> Vec<Envelope> {
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
            EnvelopeKind::Invocation => self.handle_invocation(state, envelope, connection_key),
            // Agreement/Result/Event from the peer are validated and then
            // ignored: the daemon never sends Invocations, and Events are
            // asynchronous by design (routing arrives in M6-C).
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
        // The broker owner id is the wire agentId form (M6-C1): the
        // terminal agent facts schema constrains agentId to
        // `^[A-Za-z0-9_-]+$`, so the `|` of the raw component|facet is
        // replaced by `-`.
        let agent_id = format!(
            "{}-{}",
            envelope.participant.component, envelope.participant.facet
        );

        // Daemon policy: grant exactly the requested capabilities the
        // daemon implements; everything else is policy_denied (the
        // broker-driven upgrade of the example GrantPolicy).
        let mut granted = Vec::new();
        let mut unavailable = Vec::new();
        for support in &hello.supports {
            if catalog_supports(support) {
                granted.push(support.clone());
            } else {
                unavailable.push(UnavailableCapability {
                    coordinate: support.clone(),
                    reason: UnavailableReason::PolicyDenied,
                });
            }
        }

        // Broker grant/lease registration (M5-E1 chain). An empty grant
        // set never reaches the bridge (fail-closed: NothingGranted).
        let mut generation = 0u64;
        if !granted.is_empty() {
            let result = AgentNegotiationResult {
                activation_id: activation_id.clone(),
                agreed: true,
                granted: granted.iter().map(coordinate_to_capability).collect(),
                conformance: AgentConformanceState::Known,
                lease_constraints: Some(AgentLeaseConstraints::new(LEASE_MAX_SECONDS)),
                scope: Scope::default(),
            };
            let mut broker = self.broker.lock().expect("broker lock poisoned");
            match broker.broker_grant_from_negotiation(&agent_id, result) {
                Ok(agent_grant) => {
                    generation = agent_grant.generation;
                }
                // M6-C1 terminal decision: the broker is the single-owner
                // agent-authorization authority (ADR-0014 — one grant per
                // capability). When another participant already holds one
                // of the requested grants, the negotiation still succeeds
                // at the protocol level: the Agreement reflects the daemon
                // catalog, and human capability use is authorized by the
                // authenticated connection alone (credential; the
                // activation then carries no broker state and the
                // invocation gate skips the broker check — see
                // handle_invocation). The incumbent grant owner keeps its
                // agent authority; a later same-participant negotiation
                // supersedes via the generation bump (ADR-0018 decision 1).
                Err(AgentBridgeError::Broker(BrokerError::Conflict)) => {
                    // Broker-relaxed path is Shell-only (HIGH-2): the human
                    // Shell is authorized by the credential-authenticated
                    // connection. Any other participant that conflicts with
                    // the single-owner grant stays fail-closed (nothing
                    // granted; the invocations are then rejected at the
                    // grant check below).
                    let shell = envelope.participant.component == SHELL_COMPONENT
                        && envelope.participant.facet == SHELL_FACET;
                    if !shell {
                        unavailable.extend(granted.iter().map(|coordinate| {
                            UnavailableCapability {
                                coordinate: coordinate.clone(),
                                reason: UnavailableReason::PolicyDenied,
                            }
                        }));
                        granted.clear();
                    }
                }
                // Any other bridge failure stays fail-closed (nothing
                // granted).
                Err(_) => {
                    unavailable.extend(granted.iter().map(|coordinate| UnavailableCapability {
                        coordinate: coordinate.clone(),
                        reason: UnavailableReason::PolicyDenied,
                    }));
                    granted.clear();
                }
            }
        }

        state.activations.insert(
            activation_id.clone(),
            Activation {
                activation_id: activation_id.clone(),
                agent_id,
                generation,
                granted: granted.clone(),
                hello_id: envelope.id.clone(),
            },
        );

        let mut participant = self.participant(None);
        participant.activation_id = Some(activation_id.clone());
        let generation = state.next_generation;
        state.next_generation += 1;
        let lease_constraints = if granted.is_empty() {
            None
        } else {
            Some(crate::envelope::LeaseConstraints {
                max_seconds: Some(LEASE_MAX_SECONDS),
                approval_required: None,
            })
        };
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
                    lease_constraints,
                })
                .expect("agreement payload serializes"),
            ),
            error: None,
        }]
    }

    fn handle_invocation(
        &self,
        state: &mut SessionState,
        envelope: Envelope,
        connection_key: u64,
    ) -> Vec<Envelope> {
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
        let Some(activation) = state.activations.get(&activation_id).cloned() else {
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
                    "capability {}/{} is not granted by the Agreement for activation \"{}\"",
                    capability.api_version, capability.kind, activation_id
                ),
                false,
            )];
        }

        // 3) Broker dispatch gate (ADR-0014): owner, generation, scope,
        //    valid lease — enforced again at dispatch time even though the
        //    bridge validated the same inputs (defense-in-depth,
        //    intentional). A broker-relaxed activation (M6-C1: its
        //    negotiation conflicted with the single-owner grant) carries
        //    no broker state (generation 0): its authorization is the
        //    daemon-issued Agreement itself — the credential-authenticated
        //    human path. The gate still runs for every broker-backed
        //    participant.
        // Broker-relaxed activations (generation 0) are Shell-only
        // (HIGH-2 defense-in-depth: handle_hello already fails non-Shell
        // conflicts closed, but the gate must not silently widen if that
        // ever regresses).
        if activation.generation == 0
            && activation.agent_id != format!("{SHELL_COMPONENT}-{SHELL_FACET}")
        {
            return vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                Some(&capability),
                Some(&method),
                ErrorCode::Unauthorized,
                "broker-relaxed activations are Shell-only",
                false,
            )];
        }
        if activation.generation != 0 {
            let broker = self.broker.lock().expect("broker lock poisoned");
            if let Err(error) = broker.enforce_dispatch(
                &CapabilityId {
                    api_version: capability.api_version.clone(),
                    kind: capability.kind.clone(),
                },
                &activation.agent_id,
                activation.generation,
                &Scope::default(),
            ) {
                let (code, retryable) = broker_error_mapping(error);
                return vec![self.error_result(
                    state,
                    &envelope.id,
                    &envelope.id,
                    Some(&capability),
                    Some(&method),
                    code,
                    &error.to_string(),
                    retryable,
                )];
            }
        }

        // 4) Execute the capability handler.
        let context = CapabilityContext {
            snapshot: self.status_snapshot(),
            terminal: Arc::clone(&self.terminal),
            browser: Arc::clone(&self.browser),
            runtime: Arc::clone(&self.runtime),
            events: Arc::clone(&self.events),
            broker: Arc::clone(&self.broker),
            scheduler: Arc::clone(&self.scheduler),
            connection_id: connection_key,
        };
        let result = dispatch_capability(
            &context,
            &capability,
            &method,
            envelope
                .payload
                .as_ref()
                .unwrap_or(&serde_json::Value::Null),
        );
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
            Err(DaemonMethodError::MethodNotFound { capability, method }) => {
                vec![self.error_result(
                    state,
                    &envelope.id,
                    &envelope.id,
                    Some(&capability),
                    Some(&method),
                    ErrorCode::Unavailable,
                    &format!(
                        "method \"{}\" is not implemented for {}/{}",
                        method, capability.api_version, capability.kind
                    ),
                    false,
                )]
            }
            Err(DaemonMethodError::InvalidPayload { message, .. }) => vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                Some(&capability),
                Some(&method),
                ErrorCode::MalformedMessage,
                &message,
                false,
            )],
            Err(DaemonMethodError::Conflict { message, .. }) => vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                Some(&capability),
                Some(&method),
                ErrorCode::Conflict,
                &message,
                false,
            )],
            Err(DaemonMethodError::MethodFailed {
                code,
                message,
                retryable,
            }) => vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                Some(&capability),
                Some(&method),
                code,
                &message,
                retryable,
            )],
        }
    }

    /// Snapshot of daemon facts for `daemon.status` (resource counters:
    /// terminals is real since M6-C1; browsers/runtimes are placeholders
    /// until M6-C2/C3).
    fn status_snapshot(&self) -> DaemonStatusSnapshot {
        let stats = self.transport.stats();
        let activations = self
            .broker
            .lock()
            .expect("broker lock poisoned")
            .agent_activation_count();
        DaemonStatusSnapshot {
            version: crate::DAEMON_VERSION,
            pid: std::process::id(),
            started_at: crate::envelope::now_timestamp_like(self.started_at),
            uptime_seconds: self.started_at.elapsed().map(|d| d.as_secs()).unwrap_or(0),
            claim_port: self.claim_port,
            port: self.transport.addr().port(),
            connections: stats.active,
            credentials_issued: stats.credentials_issued,
            activations,
            scheduler: self.scheduler.stats(),
            terminals: self.terminal.session_count(),
            browsers: self.browser.session_count(),
            managed_runtimes: self.runtime.managed_runtimes(),
        }
    }

    /// Drain the PTY registry event queue into the event router (registry
    /// → router → per-connection subscriber → wire). One bridge per
    /// daemon; runs for the process lifetime (30 ms drain, M5 parity).
    fn start_terminal_event_bridge(&self) {
        let host = Arc::clone(&self.terminal);
        let events = Arc::clone(&self.events);
        std::thread::spawn(move || {
            loop {
                if let Some(event) = host.registry().recv_event_timeout(EVENT_DRAIN_INTERVAL) {
                    events.publish(&RouterEvent::Terminal(event));
                }
            }
        });
    }

    /// Immutable fields of the Event envelopes the daemon pushes (built
    /// once per connection; Strings are Send + Sync so the writer thread
    /// shares it). Terminal output and browser lifecycle events use their
    /// own capability/method per variant.
    fn event_template(&self) -> EventEnvelopeTemplate {
        EventEnvelopeTemplate {
            protocol: PROTOCOL.into(),
            participant: self.participant(None),
            terminal: ProtocolCoordinate {
                api_version: TERMINAL_API_VERSION.into(),
                kind: TERMINAL_KIND.into(),
            },
            browser: ProtocolCoordinate {
                api_version: BROWSER_API_VERSION.into(),
                kind: BROWSER_KIND.into(),
            },
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

/// Poll interval of the per-connection event writer (bounds teardown
/// latency; the subscriber queue is drained at most this often).
const EVENT_WRITER_POLL: Duration = Duration::from_millis(100);

/// Immutable fields of the Event envelopes the daemon pushes; built once
/// per connection and shared with the writer thread.
#[derive(Clone)]
struct EventEnvelopeTemplate {
    protocol: String,
    participant: Participant,
    terminal: ProtocolCoordinate,
    browser: ProtocolCoordinate,
}

impl EventEnvelopeTemplate {
    /// Build one Event envelope (frame-valid per envelope.schema.json:
    /// kind Event, capability + method + object payload, no error) for
    /// the routed event variant.
    fn build(&self, event: &RouterEvent) -> Envelope {
        match event {
            RouterEvent::Terminal(output) => Envelope {
                protocol: self.protocol.clone(),
                id: new_message_id(),
                kind: EnvelopeKind::Event,
                reply_to: None,
                participant: self.participant.clone(),
                timestamp: now_timestamp(),
                generation: 0,
                capability: Some(self.terminal.clone()),
                method: Some(TERMINAL_OUTPUT_EVENT.into()),
                payload: Some(
                    serde_json::to_value(TerminalOutputEvent {
                        schema_version: crate::terminal::SCHEMA_VERSION,
                        session_id: output.session_id.clone(),
                        seq: output.seq,
                        data: output.data.clone(),
                        timestamp_unix_ms: now_unix_ms(),
                    })
                    .expect("terminal output event serializes"),
                ),
                error: None,
            },
            RouterEvent::Browser(lifecycle) => {
                let payload = BrowserEventPayload::from(lifecycle);
                Envelope {
                    protocol: self.protocol.clone(),
                    id: new_message_id(),
                    kind: EnvelopeKind::Event,
                    reply_to: None,
                    participant: self.participant.clone(),
                    timestamp: now_timestamp(),
                    generation: 0,
                    capability: Some(self.browser.clone()),
                    method: Some(lifecycle.kind.event_method().to_string()),
                    payload: Some(
                        serde_json::to_value(payload).expect("browser lifecycle event serializes"),
                    ),
                    error: None,
                }
            }
        }
    }
}

/// Map a broker gate rejection to envelope error semantics.
fn broker_error_mapping(error: BrokerError) -> (ErrorCode, bool) {
    match error {
        BrokerError::UnknownCapability | BrokerError::UnknownProvider => {
            (ErrorCode::Unavailable, true)
        }
        BrokerError::GenerationMismatch => (ErrorCode::StaleGeneration, false),
        BrokerError::Conflict => (ErrorCode::Conflict, false),
        BrokerError::NotGranted
        | BrokerError::LeaseExpired
        | BrokerError::LeaseRevoked
        | BrokerError::ScopeMismatch => (ErrorCode::Unauthorized, false),
    }
}

/// Protocol coordinate → broker capability id (field-wise mirror).
fn coordinate_to_capability(coordinate: &ProtocolCoordinate) -> CapabilityId {
    CapabilityId {
        api_version: coordinate.api_version.clone(),
        kind: coordinate.kind.clone(),
    }
}
