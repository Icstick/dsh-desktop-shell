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
//! M6-C (0.2.1): connection-scoped lease revocation on disconnect is
//! wired in [`DaemonServer::serve_connection`] teardown — every lease a
//! connection negotiated is revoked with `LeaseRevocationReason::Disconnect`
//! (broker `revoke_agent_grants` with the reason parameter, crates/supervisor).

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use dsh_local_transport::{Credential, Limits, LocalServer, ServerConn};
use dsh_supervisor::{
    AgentBridgeError, AgentConformanceState, AgentLeaseConstraints, AgentNegotiationResult, Broker,
    BrokerError, CapabilityId, LeaseRevocationReason, Scope, SystemClock,
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

/// Bootstrap-credential refresh lead (BLOCK-M8E-BOOTSTRAP-STUCK fix): the
/// daemon rewrites the credential file when the file token has less than
/// this much lifetime left. A daemon that has been idle past a full lease
/// would otherwise leave the Shell with a stale token and no re-issue
/// path (re-issue happened only on disconnect) - the Shell retry loop
/// then fails forever and the GUI stays stuck in bootstrap.
pub const BOOTSTRAP_REFRESH_LEAD: Duration = Duration::from_secs(60);

/// Control-plane peer-identity policy (ADR-0022).
///
/// The credential proves possession of a one-time token; it says nothing
/// about which process is on the other end. On carriers with kernel support
/// (the Windows named pipe) the daemon can additionally check *which
/// binary* connected - that is what distinguishes the Shell from another
/// same-user process holding the token (audit H-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerIdentityPolicy {
    /// Legacy/test posture: a credential-authenticated Shell claim is
    /// honoured without a kernel identity check. Keep this out of the
    /// production entry point (main.rs uses strict by default).
    Off,
    /// Strict posture: the Shell claim additionally requires a
    /// kernel-provided peer identity whose image path matches
    /// `expected_shell`. A missing identity (identity-less TCP carrier),
    /// a missing expected path, or a path mismatch fails closed - the
    /// claim degrades to Participant.
    Strict { expected_shell: Option<String> },
}

impl PeerIdentityPolicy {
    /// Strict policy pinned to one expected Shell image path.
    pub fn strict(expected_shell: impl Into<String>) -> Self {
        Self::Strict {
            expected_shell: Some(expected_shell.into()),
        }
    }

    /// Whether this connection may hold Shell control-plane authority.
    ///
    /// Fail-closed by construction: only an identity that resolves to the
    /// configured expected path passes; everything else (including "no
    /// expected path configured") is refused (ADR-0022 decision 3).
    fn allows(&self, identity: Option<&dsh_local_transport::PeerIdentity>) -> bool {
        match self {
            Self::Off => true,
            Self::Strict { expected_shell } => match (identity, expected_shell) {
                (Some(identity), Some(expected)) => identity.image_matches(expected),
                _ => false,
            },
        }
    }
}

/// Authority class of one activation, computed by the daemon at Hello
/// (ADR-0021 decision 1).
///
/// The class is a **server-side** fact derived from the connection's
/// authenticated handshake plus the participant's claim — never a value
/// the peer sends. It replaces the string comparisons that used to be
/// re-evaluated at dispatch time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationAuthority {
    /// The human control plane: the participant claims the Shell identity
    /// (`SHELL_COMPONENT`/`SHELL_FACET`) on a credential-authenticated
    /// connection. Only this class may use the broker-relaxed human path
    /// (ADR-0021 decision 2/3).
    ShellControl,
    /// Everything else: an ordinary participant (agent automation, an
    /// adapter, a tool). Never relaxed.
    Participant,
}

/// One negotiated activation on a connection (session-layer view; the
/// broker holds the authoritative grant/lease state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub activation_id: String,
    /// Authority class computed at Hello (ADR-0021).
    pub authority: ActivationAuthority,
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
    /// Authority class already established on this connection, if any
    /// (ADR-0021 decision 2: one connection cannot mix identity classes).
    authority: Option<ActivationAuthority>,
    /// Kernel-provided identity of the connecting process, when the carrier
    /// can provide one (ADR-0022; None on identity-less TCP). Read once at
    /// connection start and used by the Hello authority computation.
    identity: Option<dsh_local_transport::PeerIdentity>,
    seen_ids: HashSet<String>,
    next_generation: u64,
}

impl SessionState {
    /// New per-connection state carrying the carrier-reported identity.
    pub fn new(identity: Option<dsh_local_transport::PeerIdentity>) -> Self {
        Self {
            identity,
            ..Self::default()
        }
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
    /// Control-plane peer-identity policy (ADR-0022).
    policy: PeerIdentityPolicy,
    /// Endpoint of the attached peer-identity carrier (ADR-0022), published
    /// in the credential file so the Shell can prefer it over identity-less
    /// TCP.
    carrier: Option<CarrierEndpoint>,
    started_at: SystemTime,
    /// Expiry of the token in the on-disk credential file (recorded on
    /// every issue/reissue; the freshness maintenance compares against
    /// it so the Shell always finds a usable credential).
    file_credential_expiry: Mutex<Option<SystemTime>>,
}

/// Endpoint of the attached peer-identity carrier (ADR-0022), as published
/// in the credential file.
///
/// The Shell prefers this endpoint over TCP because only a connection over
/// it carries a kernel-provided peer identity, and that identity is what the
/// strict policy checks before granting control-plane authority. Exactly one
/// variant exists per platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarrierEndpoint {
    /// Windows named pipe name (without the `\\.\pipe\\` prefix).
    NamedPipe(String),
    /// Unix domain socket path.
    UnixSocket(PathBuf),
}

impl CarrierEndpoint {
    /// Human-readable endpoint for banners and logs.
    pub fn describe(&self) -> String {
        match self {
            Self::NamedPipe(name) => format!("pipe:{name}"),
            Self::UnixSocket(path) => format!("uds:{}", path.display()),
        }
    }
}

/// Per-process nonce for carrier endpoint names: keeps two daemons of the
/// same pid (tests) apart and keeps the endpoint from being guessable. Not a
/// secret - the token plus the kernel identity remain the actual gate.
fn carrier_nonce() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0)
}

/// Attach the peer-identity carrier next to the TCP endpoint (ADR-0022).
///
/// Windows: a named pipe carrying a per-process nonce; the Shell learns the
/// name from the credential file. The endpoint is a location hint, not a
/// secret.
#[cfg(windows)]
fn attach_peer_identity_carrier(
    transport: &mut LocalServer,
    limits: Limits,
    _dir: Option<&Path>,
) -> io::Result<Option<CarrierEndpoint>> {
    let name = format!(
        "dsh-desktop-daemon-{}-{:x}",
        std::process::id(),
        carrier_nonce()
    );
    let listener = dsh_local_transport::named_pipe::NamedPipeListener::bind(&name, &limits)?;
    transport.attach_carrier(listener);
    Ok(Some(CarrierEndpoint::NamedPipe(name)))
}

/// Unix twin of the above: a domain socket inside the daemon data directory
/// (already owner-only, `0700`), named with a per-process nonce so parallel
/// daemons never collide.
///
/// A missing data directory, a path the kernel cannot represent in
/// `sockaddr_un` or a bind failure leaves the daemon on identity-less TCP:
/// the carrier fails soft, authority still fails closed (every Shell claim
/// then degrades to Participant) and the reason is reported at startup.
#[cfg(unix)]
fn attach_peer_identity_carrier(
    transport: &mut LocalServer,
    limits: Limits,
    dir: Option<&Path>,
) -> io::Result<Option<CarrierEndpoint>> {
    let Some(dir) = dir else {
        return Ok(None);
    };
    let path = dir.join(format!(
        "daemon-{}-{:x}.sock",
        std::process::id(),
        carrier_nonce()
    ));
    match dsh_local_transport::uds::UdsListener::bind(&path, &limits) {
        Ok(listener) => {
            transport.attach_carrier(listener);
            Ok(Some(CarrierEndpoint::UnixSocket(path)))
        }
        Err(error) => {
            eprintln!(
                "dsh-daemon: warning: cannot attach the UDS peer-identity carrier at {}: {error}",
                path.display()
            );
            Ok(None)
        }
    }
}

#[cfg(not(any(windows, unix)))]
fn attach_peer_identity_carrier(
    _transport: &mut LocalServer,
    _limits: Limits,
    _dir: Option<&Path>,
) -> io::Result<Option<CarrierEndpoint>> {
    Ok(None)
}

impl DaemonServer {
    /// Bind the envelope server on the fixed loopback envelope port and
    /// build the broker (ADR-0019 decision 5: fixed-port envelope;
    /// 0.2.1 M6-C closes the random-port + credential indirection - the
    /// daemon owns the port itself, which is also the single-instance
    /// authority and the Shell presence probe). Port 0 requests an
    /// OS-assigned port (test isolation). The Managed runtime host
    /// resolves environments from the default catalog path (the daemon
    /// data directory).
    pub fn bind(limits: Limits, claim_port: u16) -> io::Result<Self> {
        Self::bind_with_catalog(limits, claim_port, default_catalog_path()?)
    }

    /// Bind with an explicit environment-catalog path (M6-C2: tests
    /// isolate the catalog in a temp directory; the binary uses the
    /// default data-directory catalog). Uses [`PeerIdentityPolicy::Off`]:
    /// library/test posture; the production entry point calls
    /// [`bind_with_policy`] with a strict policy.
    pub fn bind_with_catalog(
        limits: Limits,
        claim_port: u16,
        catalog_path: std::path::PathBuf,
    ) -> io::Result<Self> {
        Self::bind_with_policy(limits, claim_port, catalog_path, PeerIdentityPolicy::Off)
    }

    /// Bind with an explicit control-plane peer-identity policy (ADR-0022).
    /// The peer-identity carrier (Windows named pipe / Unix domain socket)
    /// is attached next to the TCP endpoint; its endpoint travels in the
    /// credential file.
    pub fn bind_with_policy(
        limits: Limits,
        claim_port: u16,
        catalog_path: std::path::PathBuf,
        policy: PeerIdentityPolicy,
    ) -> io::Result<Self> {
        let mut transport = LocalServer::bind_on(
            SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), claim_port),
            limits,
        )?;
        // The carrier endpoint lives next to the credential file (the same
        // owner-only directory); resolved before the catalog path is moved
        // into the runtime host below.
        let credential_dir = catalog_path.parent().map(PathBuf::from);
        // ADR-0022: attach the peer-identity carrier when the platform
        // provides one (the helper keeps the platform split in a single
        // place, so the Unix build never sees an unused `mut`).
        let carrier =
            attach_peer_identity_carrier(&mut transport, limits, credential_dir.as_deref())?;
        // Port 0 = OS-assigned: record the actual port so the credential
        // file and diagnostics carry the real endpoint (tests pass 0).
        let actual_port = transport.addr().port();
        let server = Self {
            transport,
            policy,
            carrier,
            broker: Arc::new(Mutex::new(Broker::<SystemClock>::new())),
            scheduler: Arc::new(Scheduler::new()),
            terminal: Arc::new(TerminalHost::new()),
            credential_dir,
            browser: Arc::new(BrowserHost::new()),
            runtime: Arc::new(ManagedRuntimeHost::new(catalog_path)),
            events: EventRouter::spawn(),
            claim_port: if claim_port == 0 {
                actual_port
            } else {
                claim_port
            },
            started_at: SystemTime::now(),
            file_credential_expiry: Mutex::new(None),
        };
        server.start_terminal_event_bridge();
        Ok(server)
    }

    /// The bound loopback address external tools connect to.
    pub fn addr(&self) -> SocketAddr {
        self.transport.addr()
    }

    /// Endpoint of the attached peer-identity carrier (ADR-0022), when one
    /// exists: a named pipe on Windows, a domain socket on Unix.
    pub fn carrier_endpoint(&self) -> Option<&CarrierEndpoint> {
        self.carrier.as_ref()
    }

    /// The control-plane peer-identity policy in force (ADR-0022).
    pub fn peer_identity_policy(&self) -> &PeerIdentityPolicy {
        &self.policy
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

        // Control-plane audit (ADR-0021 decision 4 / ADR-0022): every
        // accepted connection records its carrier peer and, when the
        // carrier provides one, the kernel identity of the connecting
        // process. No token, URL or user data is logged.
        eprintln!(
            "dsh-daemon: connection {} accepted peer={} identity={}",
            connection_key,
            conn.peer(),
            match conn.identity() {
                Some(identity) => format!(
                    "pid={} image={}",
                    identity.pid,
                    identity.image_path.as_deref().unwrap_or("<unresolved>")
                ),
                None => "none (identity-less carrier)".to_string(),
            }
        );
        // Kernel-provided identity of this connection (ADR-0022): present
        // on the named-pipe carrier, None on identity-less TCP.
        let mut state = SessionState::new(conn.identity().cloned());
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
        // A disconnected Shell can no longer manage the browser sessions it
        // created: release its ownership so the next Shell connection can
        // close/hand over the surviving sessions (ownerless sessions are
        // admitted by check_owner; WI-M9-BROWSER-TABS).
        self.browser.release_connection_ownership(connection_key);
        // Revoke every broker lease this connection negotiated (M6-C,
        // 0.2.1): the lease TTL no longer bounds a disconnected
        // participant - the grant dies with its connection, fail-closed.
        // A reconnect negotiates a fresh activation at the next broker
        // generation (ADR-0018 decision 1); the revocation record stays
        // durable (first record wins).
        {
            let mut broker = self.broker.lock().expect("broker lock poisoned");
            for activation_id in state.activations.keys() {
                broker.revoke_agent_grants(activation_id, LeaseRevocationReason::Disconnect);
            }
        }
    }

    /// Issue a fresh bootstrap credential and atomically rewrite the
    /// credential file. Records the file token expiry for the freshness
    /// maintenance. Only meaningful when the server was bound with a data
    /// directory (otherwise `Ok(None)`). `ttl` is the credential
    /// lifetime (callers pass [LEASE_MAX_SECONDS]; tests use short TTLs to
    /// exercise the maintenance threshold).
    pub fn issue_bootstrap_credential_file(
        &self,
        ttl: Duration,
    ) -> io::Result<Option<CredentialFile>> {
        let Some(dir) = self.credential_dir.clone() else {
            return Ok(None);
        };
        let credential = self.transport.issue_credential(ttl);
        let mut file = CredentialFile::new(
            crate::DAEMON_VERSION,
            std::process::id(),
            self.claim_port,
            self.transport.addr().port(),
            credential.token(),
            credential.expires_at(),
            SystemTime::now(),
        );
        // ADR-0022: publish the peer-identity carrier endpoint so the Shell
        // can prefer it (kernel identity -> control-plane authority under
        // the strict policy). At most one variant is ever present.
        match &self.carrier {
            Some(CarrierEndpoint::NamedPipe(name)) => {
                file = file.with_pipe_name(name);
            }
            Some(CarrierEndpoint::UnixSocket(path)) => {
                file = file.with_socket_path(path.to_string_lossy().into_owned());
            }
            None => {}
        }
        file.write_to(&dir)?;
        *self
            .file_credential_expiry
            .lock()
            .expect("file credential expiry lock poisoned") = Some(credential.expires_at());
        Ok(Some(file))
    }

    /// Re-issue the bootstrap credential after a disconnect so the next
    /// Shell start re-attaches to this surviving daemon (HIGH-1 fix:
    /// one-time credentials are consumed by the first handshake; the file
    /// must carry a fresh one for the next Shell). Best-effort: a failure
    /// leaves the previous file, which the Shell's retry loop re-reads.
    fn reissue_credential_file(&self) {
        if let Err(error) =
            self.issue_bootstrap_credential_file(Duration::from_secs(LEASE_MAX_SECONDS))
        {
            eprintln!("dsh-daemon: cannot reissue the credential file: {error}");
        }
    }

    /// Freshness maintenance for the bootstrap credential file: rewrite
    /// it when the recorded token is missing or has less than
    /// [BOOTSTRAP_REFRESH_LEAD] lifetime left, or when the file itself
    /// has gone missing. Call periodically from the serve loop (the
    /// Shell's connect retry then always finds a usable token).
    pub fn maintain_bootstrap_credential(&self) {
        let file_missing = match &self.credential_dir {
            Some(dir) => CredentialFile::read_from(dir).is_err(),
            None => false,
        };
        let expiring = {
            let expiry = self
                .file_credential_expiry
                .lock()
                .expect("file credential expiry lock poisoned");
            match *expiry {
                Some(expiry) => {
                    // Stable alternative to the unstable
                    // saturating_duration_since: an already-expired token
                    // is Duration::ZERO (inside the refresh window).
                    let remaining = expiry
                        .duration_since(SystemTime::now())
                        .unwrap_or(Duration::ZERO);
                    remaining <= BOOTSTRAP_REFRESH_LEAD
                }
                None => true,
            }
        };
        if (file_missing || expiring)
            && let Err(error) =
                self.issue_bootstrap_credential_file(Duration::from_secs(LEASE_MAX_SECONDS))
        {
            eprintln!("dsh-daemon: cannot refresh the credential file: {error}");
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

        // Authority class (ADR-0021 decision 1/2/3 + ADR-0022): computed
        // once, here, from the connection's authenticated handshake, the
        // participant claim, and - under a strict policy - the
        // kernel-provided peer identity. The envelope layer only ever
        // reaches `handle_hello` for a connection that already presented a
        // valid credential (see `serve_connection`), so a claim cannot be
        // honoured on an unauthenticated channel; strict mode additionally
        // requires that the *process* on the other end resolves to the
        // expected Shell image.
        let authority = if envelope.participant.component == SHELL_COMPONENT
            && envelope.participant.facet == SHELL_FACET
            && self.policy.allows(state.identity.as_ref())
        {
            ActivationAuthority::ShellControl
        } else {
            ActivationAuthority::Participant
        };
        // Control-plane audit (ADR-0021 decision 4): the authority verdict
        // is recorded once per activation with the claimed participant - a
        // Shell claim that failed the identity gate shows up here as
        // Participant (the observable signal that the gate fired).
        eprintln!(
            "dsh-daemon: activation {} participant={}|{} authority={:?}",
            activation_id, envelope.participant.component, envelope.participant.facet, authority
        );
        // Fail closed on mixed identities: one connection establishes one
        // class. Re-negotiating the same class is fine (generation bump),
        // switching is not.
        if let Some(established) = state.authority
            && established != authority
        {
            return vec![self.error_result(
                state,
                &envelope.id,
                &envelope.id,
                None,
                None,
                ErrorCode::Unauthorized,
                "this connection already established a different participant authority; open a new connection",
                false,
            )];
        }

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
                    // ADR-0021 decision 2: the relaxed path is a property
                    // of the server-computed authority class, not of a
                    // string comparison repeated at each decision point.
                    if authority != ActivationAuthority::ShellControl {
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
                authority,
                agent_id,
                generation,
                granted: granted.clone(),
                hello_id: envelope.id.clone(),
            },
        );
        state.authority = Some(authority);

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
        if activation.generation == 0 && activation.authority != ActivationAuthority::ShellControl {
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
