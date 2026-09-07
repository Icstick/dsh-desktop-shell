//! Daemon connection layer of the Shell (M6-C4, ADR-0019 decision 3/5).
//!
//! The Shell talks to the daemon (crates/daemon) through the unified
//! external API: dsh-local-transport authentication + envelope
//! negotiation (Hello -> Agreement) + envelope Invocation/Result, with
//! daemon Events bridged onto the existing Shell frontend events
//! (terminal://output / browser://event - names unchanged, the frontend
//! is untouched).
//!
//! Responsibilities:
//!
//! - [DaemonConnector] - the command-side seam: every proxied tauri
//!   command calls invoke(capability, method, payload); tests inject
//!   mocks (fail-closed: no daemon -> UNAVAILABLE).
//! - [DaemonClient] - the real envelope client: connect + negotiate +
//!   a worker thread that owns the transport (correlates Results with
//!   pending Invocations by id, forwards Events to the bridge channel).
//! - [connect_shell] / [start_background] - startup: probe the claim
//!   port, spawn the daemon binary when it is not running, wait for the
//!   credential file (<= 10 s), connect + negotiate, and bridge events
//!   to the frontend. The Shell never disconnects the daemon on close
//!   (M6 core semantics: resources survive Shell restarts, ADR-0008);
//!   the process exit just drops the TCP connection.
//!
//! M6-C4 notes:
//! - The credential file is one-time: a Shell restart re-reads the file
//!   the daemon re-issued (AC-IPC-001). Reconnect after a mid-session
//!   credential consumption is a daemon-side TODO (M6-C) - the Shell
//!   retries startup until the first successful connection, then stays.
//!   The installed connector is an [AutoReconnectConnector]: a
//!   connection-level invoke failure reconnects once (connect_shell +
//!   fresh event bridge) and retries that invocation; Remote errors are
//!   daemon business answers and surface as-is (known-gap fix,
//!   CURRENT.md 2026-09-04).
//! - The event bridge keeps the M3/M4 event names and payload shapes:
//!   the daemon terminal.output / browser.session-* envelope Events are
//!   re-emitted verbatim on terminal://output / browser://event.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dsh_daemon::credential::{CLAIM_PORT, CredentialFile, data_dir};
use dsh_daemon::envelope::{
    AgreementPayload, Envelope, EnvelopeKind, ErrorCode, HelloPayload, PROTOCOL, Participant,
    ProtocolCoordinate, new_message_id, now_timestamp, validate_envelope,
};
use dsh_local_transport::{Credential, Limits, LocalClient, TransportError};
use tauri::{AppHandle, Emitter};

/// Capabilities the Shell negotiates (ADR-0019 decision 5; the daemon
/// grants exactly the supported subset). daemon/scheduler are part of
/// the participant surface; terminal/browser/runtime are the proxied
/// resource capabilities.
pub fn shell_capabilities() -> Vec<ProtocolCoordinate> {
    vec![
        ProtocolCoordinate {
            api_version: dsh_daemon::capabilities::TERMINAL_API_VERSION.into(),
            kind: dsh_daemon::capabilities::TERMINAL_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: dsh_daemon::capabilities::BROWSER_API_VERSION.into(),
            kind: dsh_daemon::capabilities::BROWSER_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: dsh_daemon::capabilities::RUNTIME_API_VERSION.into(),
            kind: dsh_daemon::capabilities::RUNTIME_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: dsh_daemon::capabilities::DAEMON_API_VERSION.into(),
            kind: dsh_daemon::capabilities::DAEMON_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: dsh_daemon::scheduler::SCHEDULER_API_VERSION.into(),
            kind: dsh_daemon::scheduler::SCHEDULER_KIND.into(),
        },
    ]
}

/// Shell participant identity on the wire (the daemon broker owner becomes
/// dsh-desktop-shell-shell).
pub const SHELL_COMPONENT: &str = "dsh-desktop-shell";
pub const SHELL_FACET: &str = "shell";

/// Bounded wait for the credential file after (re)starting the daemon.
pub const CREDENTIAL_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
/// Credential file poll interval.
const CREDENTIAL_POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Retry interval of the background connect loop (the daemon may not be
/// started yet when the Shell launches).
const CONNECT_RETRY_INTERVAL: Duration = Duration::from_secs(2);
/// Base gap between automatic reconnect attempts (doubles after each
/// failed attempt up to [RECONNECT_MAX_INTERVAL]; a successful reconnect
/// resets it).
const RECONNECT_BASE_INTERVAL: Duration = Duration::from_secs(2);
/// Upper bound of the reconnect backoff.
const RECONNECT_MAX_INTERVAL: Duration = Duration::from_secs(30);
/// Per-Invocation reply deadline (runtime.start/status may block on
/// readiness for the crate START_TIMEOUT).
const INVOKE_TIMEOUT: Duration = Duration::from_secs(60);
/// Bounded event queue between the client worker and the bridge thread
/// (drop on overflow, mirroring the daemon-side AC-IPC-002 pattern).
const EVENT_QUEUE_CAPACITY: usize = 512;
/// Claim-port probe deadline.
const CLAIM_PROBE_TIMEOUT: Duration = Duration::from_millis(300);
/// Worker poll interval used to notice the stop flag.
const WORKER_POLL: Duration = Duration::from_millis(100);
/// Windows CREATE_NO_WINDOW: the daemon is a console binary and must not
/// pop a console window when spawned by the GUI Shell.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ----------------------------------------------------------------------
// Command-side seam
// ----------------------------------------------------------------------

/// Why a daemon invocation failed (the Shell-side error every command
/// module maps into its own command error contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonCommandError {
    /// No connection installed (daemon unavailable at startup).
    NotConnected,
    /// Transport failure (connect/send/recv/framing).
    Transport(String),
    /// The daemon did not answer within [INVOKE_TIMEOUT].
    Timeout,
    /// The daemon answered with a payload that does not match the
    /// expected wire shape.
    #[allow(dead_code)]
    MalformedResponse(String),
    /// The daemon returned a protocol error Result.
    Remote {
        code: ErrorCode,
        message: String,
        retryable: bool,
    },
}

impl fmt::Display for DaemonCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "daemon is not connected"),
            Self::Transport(message) => write!(f, "daemon transport: {message}"),
            Self::Timeout => write!(f, "daemon invocation timed out"),
            Self::MalformedResponse(message) => {
                write!(f, "daemon response is malformed: {message}")
            }
            Self::Remote {
                code,
                message,
                retryable,
            } => {
                write!(f, "daemon error {code} (retryable={retryable}): {message}")
            }
        }
    }
}

impl std::error::Error for DaemonCommandError {}

impl DaemonCommandError {
    /// Wire code string of the error (used by the command error mapping).
    pub fn wire_code(&self) -> String {
        match self {
            Self::Remote { code, .. } => code.to_string(),
            Self::NotConnected
            | Self::Transport(_)
            | Self::Timeout
            | Self::MalformedResponse(_) => ErrorCode::Unavailable.to_string(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::NotConnected => "The daemon is not connected.".to_string(),
            Self::Transport(message) => format!("Daemon transport failed: {message}"),
            Self::Timeout => "The daemon did not answer in time.".to_string(),
            Self::MalformedResponse(message) => format!("Daemon response is malformed: {message}"),
            Self::Remote { message, .. } => message.clone(),
        }
    }

    pub fn retryable(&self) -> bool {
        match self {
            Self::Remote { retryable, .. } => *retryable,
            Self::NotConnected | Self::Transport(_) | Self::Timeout => true,
            Self::MalformedResponse(_) => false,
        }
    }

    /// True when the failure means the installed connection itself is
    /// unusable (the auto-reconnect wrapper reacts to these). Remote and
    /// MalformedResponse are daemon-side answers - not connection
    /// failures.
    fn is_connection_class(&self) -> bool {
        matches!(
            self,
            Self::NotConnected | Self::Transport(_) | Self::Timeout
        )
    }
}

/// The command-side seam: invoke one granted capability method and await
/// its Result payload. Implemented by the real [DaemonClient] and by
/// mocks in tests.
pub trait DaemonConnector: Send + Sync {
    fn invoke(
        &self,
        capability: ProtocolCoordinate,
        method: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, DaemonCommandError>;
}

/// App-managed slot for the live connector. The background startup task
/// installs the real client once connected; command handlers fail closed
/// (UNAVAILABLE) while the slot is empty.
#[derive(Clone, Default)]
pub struct DaemonClientState {
    slot: Arc<Mutex<Option<Arc<dyn DaemonConnector>>>>,
}

impl DaemonClientState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install the live connector (idempotent: the first successful
    /// connection wins).
    pub fn install(&self, connector: Arc<dyn DaemonConnector>) {
        if let Ok(mut slot) = self.slot.lock() {
            *slot = Some(connector);
        }
    }

    /// The installed connector, if the daemon is connected.
    pub fn connector(&self) -> Option<Arc<dyn DaemonConnector>> {
        self.slot.lock().ok().and_then(|slot| slot.clone())
    }

    #[allow(dead_code)]
    pub fn is_connected(&self) -> bool {
        self.connector().is_some()
    }
}

// ----------------------------------------------------------------------
// Real envelope client
// ----------------------------------------------------------------------

/// One queued client command for the worker thread.
enum ClientCommand {
    Invoke {
        envelope: Envelope,
        reply: mpsc::SyncSender<ClientReply>,
    },
}

/// The worker's answer for one queued invocation: a daemon Result
/// envelope, or a local marker when the transport failed before the
/// invocation could be sent. The marker surfaces as a Transport error
/// (a connection failure), never as a Remote daemon answer.
enum ClientReply {
    Result(Envelope),
    TransportClosed,
}

/// The real envelope client: owns the negotiated connection through a
/// worker thread (single owner of the transport; Results are correlated
/// to pending Invocations by id, Events are forwarded to the bridge).
#[derive(Debug)]
pub struct DaemonClient {
    commands: mpsc::Sender<ClientCommand>,
    activation_id: String,
    participant: Participant,
    generation: AtomicU64,
    #[allow(dead_code)]
    stop: Arc<AtomicBool>,
}

/// Failures of [DaemonClient::connect] / [connect_shell].
#[derive(Debug)]
pub enum DaemonStartupError {
    SpawnFailed(String),
    CredentialTimeout,
    CredentialIo(io::Error),
    InvalidCredential(String),
    Transport(TransportError),
    Negotiation(String),
}

impl fmt::Display for DaemonStartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(message) => write!(f, "cannot start the daemon: {message}"),
            Self::CredentialTimeout => write!(f, "the daemon credential file never appeared"),
            Self::CredentialIo(error) => write!(f, "cannot read the daemon credential: {error}"),
            Self::InvalidCredential(message) => write!(f, "invalid daemon credential: {message}"),
            Self::Transport(error) => write!(f, "daemon transport: {error}"),
            Self::Negotiation(message) => write!(f, "daemon negotiation failed: {message}"),
        }
    }
}

impl std::error::Error for DaemonStartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CredentialIo(error) => Some(error),
            Self::Transport(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TransportError> for DaemonStartupError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

impl DaemonClient {
    /// Connect to the envelope server, negotiate (Hello -> Agreement) and
    /// spawn the worker thread. Returns the client and the event channel
    /// the caller bridges to the frontend.
    pub fn connect(
        addr: SocketAddr,
        credential: &Credential,
        limits: &Limits,
    ) -> Result<(Self, mpsc::Receiver<Envelope>), DaemonStartupError> {
        let transport = LocalClient::connect(addr, credential, limits)?;
        Self::connect_transport(transport)
    }

    /// Negotiate + spawn the worker over an already-authenticated
    /// transport (the daemon serve loop must be running before the Hello
    /// is answered; tests use this seam).
    pub fn connect_transport(
        mut transport: LocalClient,
    ) -> Result<(Self, mpsc::Receiver<Envelope>), DaemonStartupError> {
        let instance_id = format!(
            "shell-{:016x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let mut participant = Participant {
            component: SHELL_COMPONENT.into(),
            facet: SHELL_FACET.into(),
            activation_id: None,
        };
        let hello = Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind: EnvelopeKind::Hello,
            reply_to: None,
            participant: participant.clone(),
            timestamp: now_timestamp(),
            generation: 0,
            capability: None,
            method: None,
            payload: Some(
                serde_json::to_value(HelloPayload {
                    instance_id,
                    supports: shell_capabilities(),
                    requires: Vec::new(),
                })
                .expect("hello payload serializes"),
            ),
            error: None,
        };
        transport
            .send_json(&hello)
            .map_err(DaemonStartupError::from)?;
        // Async daemon Events may arrive before the Agreement (the writer
        // thread pushes them independently; ordering differs by platform).
        // Drain them instead of failing the negotiation.
        let reply = loop {
            let envelope = transport
                .recv_json::<Envelope>()
                .map_err(DaemonStartupError::from)?
                .ok_or(DaemonStartupError::Transport(TransportError::Closed))?;
            if envelope.kind != EnvelopeKind::Event {
                break envelope;
            }
        };
        if reply.kind != EnvelopeKind::Agreement {
            return Err(DaemonStartupError::Negotiation(format!(
                "expected Agreement, got {:?}",
                reply.kind
            )));
        }
        if reply.reply_to.as_deref() != Some(hello.id.as_str()) {
            return Err(DaemonStartupError::Negotiation(
                "Agreement does not correlate with the Hello".into(),
            ));
        }
        if let Err(issues) = validate_envelope(&reply) {
            return Err(DaemonStartupError::Negotiation(format!(
                "Agreement failed frame validation: {}",
                issues
                    .iter()
                    .map(|issue| format!("{}: {}", issue.path, issue.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        let payload: AgreementPayload = reply
            .payload
            .clone()
            .and_then(|payload| serde_json::from_value(payload).ok())
            .ok_or_else(|| {
                DaemonStartupError::Negotiation(
                    "Agreement payload is not an agreementPayload".into(),
                )
            })?;
        if payload.granted.is_empty() {
            return Err(DaemonStartupError::Negotiation(
                "the daemon granted no capabilities".into(),
            ));
        }
        participant.activation_id = Some(payload.activation_id.clone());

        let (command_tx, command_rx) = mpsc::channel::<ClientCommand>();
        let (event_tx, event_rx) = mpsc::sync_channel::<Envelope>(EVENT_QUEUE_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        std::thread::spawn({
            let stop = Arc::clone(&stop);
            move || worker_loop(transport, command_rx, event_tx, stop)
        });

        Ok((
            Self {
                commands: command_tx,
                activation_id: payload.activation_id,
                participant,
                generation: AtomicU64::new(1),
                stop,
            },
            event_rx,
        ))
    }

    /// The negotiated activation id.
    pub fn activation_id(&self) -> &str {
        &self.activation_id
    }

    /// Stop the worker thread (best-effort; the worker notices within one
    /// poll interval). The daemon keeps running either way.
    #[allow(dead_code)]
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl DaemonConnector for DaemonClient {
    fn invoke(
        &self,
        capability: ProtocolCoordinate,
        method: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, DaemonCommandError> {
        let invocation = Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind: EnvelopeKind::Invocation,
            reply_to: None,
            participant: self.participant.clone(),
            timestamp: now_timestamp(),
            generation: self.generation.fetch_add(1, Ordering::Relaxed),
            capability: Some(capability),
            method: Some(method.to_string()),
            payload: Some(payload),
            error: None,
        };
        let (reply_tx, reply_rx) = mpsc::sync_channel::<ClientReply>(1);
        self.commands
            .send(ClientCommand::Invoke {
                envelope: invocation,
                reply: reply_tx,
            })
            .map_err(|_| DaemonCommandError::Transport("client worker is gone".into()))?;
        match reply_rx.recv_timeout(INVOKE_TIMEOUT) {
            Ok(ClientReply::Result(reply)) => match reply.error {
                Some(error) => Err(DaemonCommandError::Remote {
                    code: error.code,
                    message: error.message,
                    retryable: error.retryable,
                }),
                None => Ok(reply.payload.unwrap_or_else(|| serde_json::json!({}))),
            },
            // The worker answered locally: the transport failed before the
            // invocation was sent. A connection failure, not a daemon
            // business answer (the auto-reconnect wrapper reacts to it).
            Ok(ClientReply::TransportClosed) => Err(DaemonCommandError::Transport(
                "client transport closed before the invocation was sent".into(),
            )),
            // A timed-out invocation leaves its pending entry in the worker
            // until the (late) reply arrives - bounded, only on daemon
            // misbehavior.
            Err(mpsc::RecvTimeoutError::Timeout) => Err(DaemonCommandError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(DaemonCommandError::Transport(
                "client worker is gone".into(),
            )),
        }
    }
}

// ----------------------------------------------------------------------
// Connection-level auto-reconnect (known-gap fix, CURRENT.md 2026-09-04)
// ----------------------------------------------------------------------

/// Reconnect factory: builds a replacement connector from scratch. The
/// production factory runs the full [connect_shell] startup and spawns a
/// fresh event bridge; tests inject scripted connectors.
type ConnectorFactory =
    Box<dyn Fn() -> Result<Arc<dyn DaemonConnector>, DaemonStartupError> + Send + Sync>;

/// Backoff of reconnect attempts: one attempt per interval, doubling
/// after each failed attempt up to the cap; a successful reconnect
/// resets the interval and allows the next attempt immediately.
struct BackoffState {
    cap: Duration,
    base: Duration,
    interval: Duration,
    allowed_after: Instant,
}

impl BackoffState {
    fn new(base: Duration) -> Self {
        Self {
            cap: base.max(RECONNECT_MAX_INTERVAL),
            base,
            interval: base,
            allowed_after: Instant::now(),
        }
    }

    /// Reserve one attempt now. False when the previous attempt is still
    /// inside its backoff window (the caller surfaces the original error
    /// without calling the factory).
    fn begin(&mut self) -> bool {
        let now = Instant::now();
        if now < self.allowed_after {
            return false;
        }
        self.allowed_after = now + self.interval;
        self.interval = (self.interval * 2).min(self.cap);
        true
    }

    fn reset(&mut self) {
        self.allowed_after = Instant::now();
        self.interval = self.base;
    }
}

/// Connection-level auto-reconnect wrapper installed by [start_background]
/// around the first live connection. Invocations delegate to the current
/// inner connector; when one fails with a connection-class error
/// ([DaemonCommandError::is_connection_class] - Remote is a daemon
/// business answer, not a connection failure), the wrapper reconnects
/// once through the [ConnectorFactory], swaps the inner connector and
/// retries that invocation.
///
/// Concurrency:
/// - Healthy invocations are never serialized: each call clones the
///   current connector under a short lock and invokes outside of it.
/// - Reconnects are single-flight (a Mutex try-lock). While one
///   reconnect runs, other failing invocations fail immediately with
///   their original error (fail-closed, retryable) instead of queueing
///   behind a potentially slow reconnect.
/// - After a successful reconnect, invocations that failed on the old
///   connector retry once against the fresh one without reconnecting
///   again (epoch check).
/// - Failed reconnect attempts are backoff-limited (daemon down: at most
///   one attempt per window instead of one per invocation).
pub struct AutoReconnectConnector {
    inner: Mutex<Arc<dyn DaemonConnector>>,
    /// Bumped on every connector replacement; pairs snapshots with the
    /// inner connector they belong to.
    epoch: AtomicU64,
    reconnect: ConnectorFactory,
    reconnect_lock: Mutex<()>,
    backoff: Mutex<BackoffState>,
}

impl AutoReconnectConnector {
    /// Wrap the first live connection with the default backoff.
    pub fn new(initial: Arc<dyn DaemonConnector>, reconnect: ConnectorFactory) -> Self {
        Self::with_base_interval(initial, reconnect, RECONNECT_BASE_INTERVAL)
    }

    fn with_base_interval(
        initial: Arc<dyn DaemonConnector>,
        reconnect: ConnectorFactory,
        base: Duration,
    ) -> Self {
        Self {
            inner: Mutex::new(initial),
            epoch: AtomicU64::new(0),
            reconnect,
            reconnect_lock: Mutex::new(()),
            backoff: Mutex::new(BackoffState::new(base)),
        }
    }

    /// Current connector and the epoch of the replacement it belongs to
    /// (both read under the inner lock, so the pair is consistent).
    fn snapshot(&self) -> (Arc<dyn DaemonConnector>, u64) {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        (Arc::clone(&inner), self.epoch.load(Ordering::Relaxed))
    }

    /// Swap in a freshly connected connector (new epoch).
    fn install_fresh(&self, fresh: Arc<dyn DaemonConnector>) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *inner = fresh;
        self.epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// Recovery of one connection-class failure that ran on the connector
    /// of `failed_epoch`: single-flight reconnect, then one retry of the
    /// failed invocation.
    fn recover(
        &self,
        failed_epoch: u64,
        original: DaemonCommandError,
        capability: ProtocolCoordinate,
        method: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, DaemonCommandError> {
        // Single-flight gate: at most one reconnect at a time. Failing
        // invocations during a reconnect answer immediately with their
        // original (retryable) error; the next invocation uses the fresh
        // connection.
        let Ok(_gate) = self.reconnect_lock.try_lock() else {
            return Err(original);
        };
        let (current, epoch) = self.snapshot();
        if epoch != failed_epoch {
            // Another invocation already replaced the connection while
            // this one was failing: retry once against the live connector.
            return current.invoke(capability, &method, payload);
        }
        if !self
            .backoff
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .begin()
        {
            // A previous reconnect attempt failed recently (daemon likely
            // still down): surface the original error without hammering
            // connect_shell.
            return Err(original);
        }
        match (self.reconnect)() {
            Ok(fresh) => {
                self.backoff
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .reset();
                self.install_fresh(Arc::clone(&fresh));
                // The one retry of the failed invocation.
                fresh.invoke(capability, &method, payload)
            }
            Err(_) => Err(original),
        }
    }
}

impl DaemonConnector for AutoReconnectConnector {
    fn invoke(
        &self,
        capability: ProtocolCoordinate,
        method: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, DaemonCommandError> {
        let (connector, epoch) = self.snapshot();
        match connector.invoke(capability.clone(), method, payload.clone()) {
            Err(error) if error.is_connection_class() => {
                self.recover(epoch, error, capability, method.to_string(), payload)
            }
            outcome => outcome,
        }
    }
}
/// The client worker thread: single owner of the transport. Queued
/// Invocations are sent (pending by id), then the socket is read:
/// Results dispatch to the pending reply channel, Events forward to the
/// bridge channel. A dead connection makes the next send/recv fail and
/// the worker exits.
fn worker_loop(
    mut transport: LocalClient,
    commands: mpsc::Receiver<ClientCommand>,
    events: mpsc::SyncSender<Envelope>,
    stop: Arc<AtomicBool>,
) {
    let mut pending: HashMap<String, mpsc::SyncSender<ClientReply>> = HashMap::new();
    'outer: while !stop.load(Ordering::Relaxed) {
        // Drain queued commands first (unbounded queue; the worker sends
        // them in order and the daemon answers in order).
        loop {
            match commands.try_recv() {
                Ok(ClientCommand::Invoke { envelope, reply }) => {
                    let id = envelope.id.clone();
                    if transport.send_json(&envelope).is_err() {
                        let _ = reply.send(ClientReply::TransportClosed);
                        break 'outer;
                    }
                    pending.insert(id, reply);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break 'outer,
            }
        }
        // Poll the socket (bounded) so the stop flag and queued commands
        // are noticed even while the daemon is quiet.
        match transport.recv_timeout(WORKER_POLL) {
            Ok(Some(bytes)) => {
                if let Ok(envelope) = serde_json::from_slice::<Envelope>(&bytes) {
                    match envelope.kind {
                        EnvelopeKind::Event => {
                            let _ = events.try_send(envelope);
                        }
                        EnvelopeKind::Result => {
                            if let Some(id) = envelope.reply_to.clone()
                                && let Some(reply) = pending.remove(&id)
                            {
                                let _ = reply.send(ClientReply::Result(envelope));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(None) => {
                // Timeout (no data) or closed socket: a closed connection
                // is detected on the next send (invoke) or by the stop flag.
            }
            Err(_) => break,
        }
    }
}

// ----------------------------------------------------------------------
// Startup orchestration
// ----------------------------------------------------------------------

/// Options of the startup connect (tests override data dir / exe / spawn).
#[derive(Debug, Clone)]
pub struct StartupOptions {
    /// Directory the daemon writes daemon-credential.json into.
    pub data_dir: PathBuf,
    /// Single-instance claim port (liveness probe).
    pub claim_port: u16,
    /// Explicit daemon binary; None discovers it next to the Shell exe.
    pub daemon_exe: Option<PathBuf>,
    /// Spawn the daemon when the claim probe fails (disabled in tests).
    pub spawn_daemon: bool,
}

impl Default for StartupOptions {
    fn default() -> Self {
        Self {
            data_dir: data_dir(),
            claim_port: CLAIM_PORT,
            daemon_exe: None,
            spawn_daemon: true,
        }
    }
}

/// Probe the claim port: the daemon is running when the TCP connect
/// succeeds (the port is owned by the single-instance guard).
pub fn probe_claim_port(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, CLAIM_PROBE_TIMEOUT).is_ok()
}

/// Locate the daemon executable: DSH_DAEMON_EXE override, then the
/// directory of the Shell executable (dsh-desktop-daemon[.exe], the dev
/// build sits next to the Shell in target/debug).
pub fn resolve_daemon_executable() -> Result<PathBuf, DaemonStartupError> {
    if let Ok(exe) = std::env::var("DSH_DAEMON_EXE")
        && !exe.is_empty()
    {
        let path = PathBuf::from(exe);
        if path.is_file() {
            return Ok(path);
        }
    }
    let names: &[&str] = if cfg!(windows) {
        &["dsh-desktop-daemon.exe", "dsh-daemon.exe"]
    } else {
        &["dsh-desktop-daemon", "dsh-daemon"]
    };
    if let Ok(current) = std::env::current_exe()
        && let Some(dir) = current.parent()
    {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(DaemonStartupError::SpawnFailed(
        "cannot locate the daemon executable (DSH_DAEMON_EXE or the Shell directory)".into(),
    ))
}

/// Spawn the daemon binary (console app; CREATE_NO_WINDOW on Windows so
/// the GUI Shell never pops a console). The child outlives the Shell.
fn spawn_daemon(exe: Option<&Path>) -> Result<(), DaemonStartupError> {
    let exe = match exe {
        Some(path) => path.to_path_buf(),
        None => resolve_daemon_executable()?,
    };
    let mut command = std::process::Command::new(&exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().map(|_| ()).map_err(|error| {
        DaemonStartupError::SpawnFailed(format!("cannot spawn {}: {error}", exe.display()))
    })
}

/// Poll the credential file until it appears (the daemon writes it before
/// serving; the one-time credential is consumed by the first handshake).
pub fn wait_for_credential(
    dir: &Path,
    timeout: Duration,
) -> Result<CredentialFile, DaemonStartupError> {
    let deadline = Instant::now() + timeout;
    loop {
        match CredentialFile::read_from(dir) {
            Ok(file) => return Ok(file),
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(CREDENTIAL_POLL_INTERVAL);
            }
            Err(error) => {
                return Err(if Instant::now() < deadline {
                    DaemonStartupError::CredentialIo(error)
                } else {
                    DaemonStartupError::CredentialTimeout
                });
            }
        }
    }
}

/// Parse the RFC 3339 UTC timestamps the daemon writes (always
/// YYYY-MM-DDTHH:MM:SS[.fff]Z); anything else fails closed.
pub fn parse_rfc3339_utc(value: &str) -> Result<SystemTime, String> {
    let bytes = value.as_bytes();
    let err = || format!("not a UTC RFC 3339 timestamp: {value:?}");
    if bytes.len() < 20 || bytes[10] != b'T' || bytes[13] != b':' || bytes[16] != b':' {
        return Err(err());
    }
    let digit = |range: std::ops::Range<usize>| -> Result<u64, String> {
        let text = &value[range];
        if text.bytes().all(|b| b.is_ascii_digit()) {
            text.parse().map_err(|_| err())
        } else {
            Err(err())
        }
    };
    let year = digit(0..4)?;
    let month = digit(5..7)?;
    let day = digit(8..10)?;
    let hour = digit(11..13)?;
    let minute = digit(14..16)?;
    let second = digit(17..19)?;
    let mut millis: u32 = 0;
    let mut i = 19;
    if bytes.get(i) == Some(&b'.') {
        let start = i + 1;
        let mut end = start;
        while bytes.get(end).is_some_and(|b| b.is_ascii_digit()) {
            end += 1;
        }
        if end == start {
            return Err(err());
        }
        let fraction = &value[start..end];
        let parsed: u64 = fraction.parse().map_err(|_| err())?;
        millis = (parsed * 1000 / 10u64.pow(fraction.len() as u32)) as u32;
        i = end;
    }
    if bytes.get(i) != Some(&b'Z') || i + 1 != bytes.len() {
        return Err(err());
    }
    if month == 0 || month > 12 || day == 0 || day > 31 || hour > 23 || minute > 59 || second > 59 {
        return Err(err());
    }
    let days = days_from_civil(year as i64, month as u32, day as u32);
    let seconds = days * 86_400 + (hour * 3600 + minute * 60 + second) as i64;
    Ok(UNIX_EPOCH + Duration::new(seconds.max(0) as u64, millis * 1_000_000))
}

/// Days since 1970-01-01 for a civil date (inverse of the daemon
/// civil_from_days, Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Full startup connect: probe the claim port (spawn the daemon when it is
/// not running), wait for the credential file, connect + negotiate, and
/// return the client with its event channel.
pub fn connect_shell(
    options: &StartupOptions,
) -> Result<(DaemonClient, mpsc::Receiver<Envelope>), DaemonStartupError> {
    if !probe_claim_port(options.claim_port) {
        if options.spawn_daemon {
            spawn_daemon(options.daemon_exe.as_deref())?;
        } else {
            return Err(DaemonStartupError::SpawnFailed(
                "the daemon is not running and spawning is disabled".into(),
            ));
        }
    }
    let credential_file = wait_for_credential(&options.data_dir, CREDENTIAL_WAIT_TIMEOUT)?;
    let expires_at = parse_rfc3339_utc(&credential_file.credential.expires_at)
        .map_err(DaemonStartupError::InvalidCredential)?;
    let credential = Credential::new(credential_file.credential.token.clone(), expires_at);
    let addr = SocketAddr::from(([127, 0, 0, 1], credential_file.port));
    DaemonClient::connect(addr, &credential, &Limits::default())
}

/// Background startup task (lib.rs setup): retry until the first
/// successful connection, then install an [AutoReconnectConnector]
/// around it and return. The Shell never disconnects the daemon on close
/// (M6 core semantics); when the connection dies later (daemon crash,
/// restart or upgrade), connection-level invoke failures reconnect
/// automatically once and retry the invocation (known-gap fix,
/// CURRENT.md 2026-09-04).
pub fn start_background(app: AppHandle, state: DaemonClientState, options: StartupOptions) {
    std::thread::spawn(move || {
        loop {
            match connect_shell(&options) {
                Ok((client, events)) => {
                    eprintln!(
                        "[daemon-client] connected (activation {})",
                        client.activation_id()
                    );
                    spawn_event_bridge(app.clone(), events);
                    let reconnect = {
                        let app = app.clone();
                        let options = options.clone();
                        Box::new(
                            move || -> Result<Arc<dyn DaemonConnector>, DaemonStartupError> {
                                connect_shell(&options).map(|(client, events)| {
                                    eprintln!(
                                        "[daemon-client] reconnected (activation {})",
                                        client.activation_id()
                                    );
                                    spawn_event_bridge(app.clone(), events);
                                    Arc::new(client) as Arc<dyn DaemonConnector>
                                })
                            },
                        )
                    };
                    state.install(Arc::new(AutoReconnectConnector::new(
                        Arc::new(client),
                        reconnect,
                    )));
                    return;
                }
                Err(error) => {
                    eprintln!(
                        "[daemon-client] connection attempt failed: {error}; retrying in {CONNECT_RETRY_INTERVAL:?}"
                    );
                    std::thread::sleep(CONNECT_RETRY_INTERVAL);
                }
            }
        }
    });
}

// ----------------------------------------------------------------------
// Event bridge (daemon Events -> Shell frontend events)
// ----------------------------------------------------------------------

/// Map one daemon Event envelope to the Shell frontend event: the
/// terminal.output event keeps its M3 name terminal://output and the
/// browser lifecycle events keep the M4 name browser://event - the
/// payloads are forwarded verbatim (same specs shapes), so the frontend
/// needs no change.
pub fn daemon_event_target(envelope: &Envelope) -> Option<(&'static str, serde_json::Value)> {
    if envelope.kind != EnvelopeKind::Event {
        return None;
    }
    let capability = envelope.capability.as_ref()?;
    let method = envelope.method.as_deref()?;
    let payload = envelope.payload.clone()?;
    match (capability.kind.as_str(), method) {
        ("Terminal", "terminal.output") => Some(("terminal://output", payload)),
        ("Browser", _) => Some(("browser://event", payload)),
        _ => None,
    }
}

/// Bridge thread: daemon Events -> app.emit on the Shell frontend
/// events (payload passthrough).
pub fn spawn_event_bridge(app: AppHandle, events: mpsc::Receiver<Envelope>) {
    std::thread::spawn(move || {
        while let Ok(envelope) = events.recv() {
            let Some((event_name, payload)) = daemon_event_target(&envelope) else {
                continue;
            };
            let _ = app.emit(event_name, payload);
        }
    });
}

// ----------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use dsh_daemon::envelope::now_timestamp_like;
    use dsh_daemon::server::DaemonServer;
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;

    /// Scripted answer for one invocation.
    type MockHandler = Box<
        dyn FnMut(
                &ProtocolCoordinate,
                &str,
                serde_json::Value,
            ) -> Result<serde_json::Value, DaemonCommandError>
            + Send,
    >;

    /// Deterministic mock connector: records invocations and answers
    /// through a scripted handler (shared by the command module tests).
    #[derive(Clone)]
    pub(crate) struct MockConnector {
        calls: Arc<Mutex<Vec<(String, String, serde_json::Value)>>>,
        handler: Arc<Mutex<MockHandler>>,
    }

    impl MockConnector {
        pub fn new(
            handler: impl FnMut(
                &ProtocolCoordinate,
                &str,
                serde_json::Value,
            ) -> Result<serde_json::Value, DaemonCommandError>
            + Send
            + 'static,
        ) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                handler: Arc::new(Mutex::new(Box::new(handler))),
            }
        }

        /// Answer every invocation with one canned payload.
        pub fn ok(payload: serde_json::Value) -> Self {
            Self::new(move |_, _, _| Ok(payload.clone()))
        }

        /// Fail every invocation with one canned error.
        pub fn error(error: DaemonCommandError) -> Self {
            Self::new(move |_, _, _| Err(error.clone()))
        }

        /// Answer invocations sequentially (the last response repeats).
        pub fn sequential(responses: Vec<Result<serde_json::Value, DaemonCommandError>>) -> Self {
            let responses = Arc::new(Mutex::new(responses));
            Self::new(move |_, _, _| {
                let mut queue = responses.lock().expect("responses lock");
                if queue.len() > 1 {
                    queue.remove(0)
                } else {
                    queue[0].clone()
                }
            })
        }

        /// Recorded invocations as (kind, method, payload).
        pub fn calls(&self) -> Vec<(String, String, serde_json::Value)> {
            self.calls.lock().expect("calls lock").clone()
        }
    }

    impl DaemonConnector for MockConnector {
        fn invoke(
            &self,
            capability: ProtocolCoordinate,
            method: &str,
            payload: serde_json::Value,
        ) -> Result<serde_json::Value, DaemonCommandError> {
            self.calls.lock().expect("calls lock").push((
                capability.kind.clone(),
                method.to_string(),
                payload.clone(),
            ));
            self.handler.lock().expect("handler lock")(&capability, method, payload)
        }
    }

    fn event_envelope(kind: &str, method: &str, payload: serde_json::Value) -> Envelope {
        Envelope {
            protocol: PROTOCOL.into(),
            id: new_message_id(),
            kind: EnvelopeKind::Event,
            reply_to: None,
            participant: Participant {
                component: "dsh-desktop-shell".into(),
                facet: "daemon".into(),
                activation_id: None,
            },
            timestamp: now_timestamp(),
            generation: 0,
            capability: Some(ProtocolCoordinate {
                api_version: "terminal.dsh-desktop.local/v1alpha1".into(),
                kind: kind.into(),
            }),
            method: Some(method.into()),
            payload: Some(payload),
            error: None,
        }
    }

    #[test]
    fn event_target_keeps_frontend_event_names() {
        let terminal = event_envelope(
            "Terminal",
            "terminal.output",
            serde_json::json!({ "sessionId": "pty-1", "data": "hi" }),
        );
        let (name, payload) = daemon_event_target(&terminal).expect("terminal event");
        assert_eq!(name, "terminal://output");
        assert_eq!(payload["data"], "hi");

        for method in ["browser.session-created", "browser.session-closed"] {
            let browser = event_envelope(
                "Browser",
                method,
                serde_json::json!({ "sessionId": "brw-1", "kind": "created" }),
            );
            let (name, payload) = daemon_event_target(&browser).expect("browser event");
            assert_eq!(name, "browser://event");
            assert_eq!(payload["sessionId"], "brw-1");
        }

        // Non-event kinds and unknown capabilities are dropped.
        let result = Envelope {
            kind: EnvelopeKind::Result,
            ..event_envelope("Terminal", "terminal.output", serde_json::json!({}))
        };
        assert!(daemon_event_target(&result).is_none());
        let unknown = event_envelope("Unknown", "something", serde_json::json!({}));
        assert!(daemon_event_target(&unknown).is_none());
    }

    #[test]
    fn wait_for_credential_polls_until_file_appears() {
        let dir = std::env::temp_dir().join(format!("dsh-client-cred-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let now = SystemTime::now();
        let file = CredentialFile::new(
            "0.1.0-test",
            1234,
            CLAIM_PORT,
            50_001,
            "lt_test_token",
            now + Duration::from_secs(3600),
            now,
        );
        std::thread::spawn({
            let dir = dir.clone();
            let file = file.clone();
            move || {
                std::thread::sleep(Duration::from_millis(300));
                file.write_to(&dir).expect("writes credential");
            }
        });
        let read = wait_for_credential(&dir, Duration::from_secs(5)).expect("credential");
        assert_eq!(read.port, 50_001);
        assert_eq!(read.credential.token, "lt_test_token");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wait_for_credential_times_out() {
        let dir = std::env::temp_dir().join(format!("dsh-client-cred-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let error = wait_for_credential(&dir, Duration::from_millis(300)).expect_err("timeout");
        assert!(matches!(error, DaemonStartupError::CredentialTimeout));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rfc3339_utc_parse_roundtrips_daemon_format() {
        let now = SystemTime::now();
        let formatted = now_timestamp_like(now);
        let parsed = parse_rfc3339_utc(&formatted).expect("parses daemon format");
        // Millisecond precision survives the round trip.
        let drift = parsed.duration_since(now).unwrap_or_default().as_millis();
        assert!(drift <= 1, "roundtrip drift {drift}ms for {formatted}");
        assert!(parse_rfc3339_utc("2026-08-31T09:30:00Z").is_ok());
        assert!(parse_rfc3339_utc("not-a-date").is_err());
        assert!(parse_rfc3339_utc("2026-13-01T00:00:00.000Z").is_err());
        assert!(parse_rfc3339_utc("2026-08-31T09:30:00+08:00").is_err());
        assert_eq!(
            parse_rfc3339_utc("1970-01-01T00:00:00.000Z").expect("epoch"),
            UNIX_EPOCH
        );
    }

    #[test]
    fn connector_state_fails_closed_when_empty() {
        let state = DaemonClientState::new();
        assert!(!state.is_connected());
        assert!(state.connector().is_none());
        state.install(Arc::new(MockConnector::ok(serde_json::json!({}))));
        assert!(state.is_connected());
    }

    // ------------------------------------------------------------------
    // Auto-reconnect (known-gap fix, CURRENT.md 2026-09-04)
    // ------------------------------------------------------------------

    fn terminal_coordinate() -> ProtocolCoordinate {
        ProtocolCoordinate {
            api_version: dsh_daemon::capabilities::TERMINAL_API_VERSION.into(),
            kind: dsh_daemon::capabilities::TERMINAL_KIND.into(),
        }
    }

    fn terminal_status_payload() -> serde_json::Value {
        serde_json::json!({})
    }

    #[test]
    fn connection_class_failures_are_classified() {
        assert!(DaemonCommandError::NotConnected.is_connection_class());
        assert!(DaemonCommandError::Transport("x".into()).is_connection_class());
        assert!(DaemonCommandError::Timeout.is_connection_class());
        assert!(!DaemonCommandError::MalformedResponse("x".into()).is_connection_class());
        assert!(
            !DaemonCommandError::Remote {
                code: ErrorCode::Unavailable,
                message: "x".into(),
                retryable: true,
            }
            .is_connection_class()
        );
    }

    #[test]
    fn transport_failure_reconnects_and_retries_once() {
        let dead = Arc::new(MockConnector::error(DaemonCommandError::Transport(
            "boom".into(),
        )));
        let fresh = Arc::new(MockConnector::ok(serde_json::json!({ "ok": true })));
        let wrapper = AutoReconnectConnector::with_base_interval(
            Arc::clone(&dead) as Arc<dyn DaemonConnector>,
            Box::new({
                let fresh = Arc::clone(&fresh);
                move || Ok(Arc::clone(&fresh) as Arc<dyn DaemonConnector>)
            }),
            Duration::ZERO,
        );
        let result = wrapper.invoke(
            terminal_coordinate(),
            dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
            terminal_status_payload(),
        );
        assert_eq!(
            result.expect("retried invoke"),
            serde_json::json!({ "ok": true })
        );
        assert_eq!(dead.calls().len(), 1, "dead connector invoked once");
        assert_eq!(fresh.calls().len(), 1, "fresh connector retried once");
    }

    #[test]
    fn timeout_failure_reconnects_and_retries_once() {
        let dead = MockConnector::error(DaemonCommandError::Timeout);
        let fresh = Arc::new(MockConnector::ok(serde_json::json!({ "ok": true })));
        let wrapper = AutoReconnectConnector::with_base_interval(
            Arc::new(dead),
            Box::new({
                let fresh = Arc::clone(&fresh);
                move || Ok(Arc::clone(&fresh) as Arc<dyn DaemonConnector>)
            }),
            Duration::ZERO,
        );
        let result = wrapper.invoke(
            terminal_coordinate(),
            dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
            terminal_status_payload(),
        );
        assert_eq!(
            result.expect("retried invoke"),
            serde_json::json!({ "ok": true })
        );
    }

    #[test]
    fn remote_error_surfaces_without_reconnect() {
        let error = DaemonCommandError::Remote {
            code: ErrorCode::Unavailable,
            message: "daemon says no".into(),
            retryable: true,
        };
        let attempts = Arc::new(AtomicUsize::new(0));
        let wrapper = AutoReconnectConnector::with_base_interval(
            Arc::new(MockConnector::error(error.clone())),
            Box::new({
                let attempts = Arc::clone(&attempts);
                move || {
                    attempts.fetch_add(1, Ordering::Relaxed);
                    Err(DaemonStartupError::SpawnFailed("never".into()))
                }
            }),
            Duration::ZERO,
        );
        let result = wrapper.invoke(
            terminal_coordinate(),
            dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
            terminal_status_payload(),
        );
        assert_eq!(result.expect_err("remote error"), error);
        assert_eq!(
            attempts.load(Ordering::Relaxed),
            0,
            "no reconnect for Remote"
        );
    }

    #[test]
    fn failed_reconnect_returns_original_error_and_backs_off() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let wrapper = AutoReconnectConnector::with_base_interval(
            Arc::new(MockConnector::error(DaemonCommandError::Transport(
                "boom".into(),
            ))),
            Box::new({
                let attempts = Arc::clone(&attempts);
                move || {
                    attempts.fetch_add(1, Ordering::Relaxed);
                    Err(DaemonStartupError::SpawnFailed("daemon is down".into()))
                }
            }),
            // Huge window: the second failure arrives inside the backoff.
            Duration::from_secs(3600),
        );
        let invoke = || {
            wrapper.invoke(
                terminal_coordinate(),
                dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                terminal_status_payload(),
            )
        };
        let expected = DaemonCommandError::Transport("boom".into());
        assert_eq!(invoke().expect_err("first attempt"), expected);
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
        assert_eq!(invoke().expect_err("second attempt"), expected);
        assert_eq!(
            attempts.load(Ordering::Relaxed),
            1,
            "no reconnect inside the backoff window"
        );
    }

    #[test]
    fn reconnect_retries_after_the_backoff_window() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let wrapper = AutoReconnectConnector::with_base_interval(
            Arc::new(MockConnector::error(DaemonCommandError::Transport(
                "boom".into(),
            ))),
            Box::new({
                let attempts = Arc::clone(&attempts);
                move || {
                    attempts.fetch_add(1, Ordering::Relaxed);
                    Err(DaemonStartupError::SpawnFailed("daemon is down".into()))
                }
            }),
            Duration::from_millis(100),
        );
        let invoke = || {
            wrapper.invoke(
                terminal_coordinate(),
                dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                terminal_status_payload(),
            )
        };
        let expected = DaemonCommandError::Transport("boom".into());
        assert_eq!(invoke().expect_err("first attempt"), expected);
        assert_eq!(invoke().expect_err("inside window"), expected);
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(invoke().expect_err("after window"), expected);
        assert_eq!(attempts.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn concurrent_connection_failures_reconnect_single_flight() {
        let fresh = Arc::new(MockConnector::ok(serde_json::json!({ "ok": true })));
        let attempts = Arc::new(AtomicUsize::new(0));
        let wrapper = Arc::new(AutoReconnectConnector::with_base_interval(
            Arc::new(MockConnector::error(DaemonCommandError::Transport(
                "boom".into(),
            ))),
            Box::new({
                let attempts = Arc::clone(&attempts);
                let fresh = Arc::clone(&fresh);
                move || {
                    attempts.fetch_add(1, Ordering::Relaxed);
                    // Slow factory: widens the single-flight window so the
                    // other threads observe a reconnect in progress.
                    std::thread::sleep(Duration::from_millis(150));
                    Ok(Arc::clone(&fresh) as Arc<dyn DaemonConnector>)
                }
            }),
            Duration::ZERO,
        ));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let wrapper = Arc::clone(&wrapper);
            handles.push(std::thread::spawn(move || {
                wrapper.invoke(
                    terminal_coordinate(),
                    dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                    terminal_status_payload(),
                )
            }));
        }
        let results: Vec<Result<serde_json::Value, DaemonCommandError>> = handles
            .into_iter()
            .map(|handle| handle.join().expect("worker"))
            .collect();
        assert_eq!(attempts.load(Ordering::Relaxed), 1, "exactly one reconnect");
        assert!(
            results.iter().any(Result::is_ok),
            "reconnected invocation retried"
        );
        for result in &results {
            if let Err(error) = result {
                assert_eq!(error, &DaemonCommandError::Transport("boom".into()));
            }
        }
    }

    // ------------------------------------------------------------------
    // Integration: real in-process daemon (DaemonServer) + real client
    // ------------------------------------------------------------------

    const TEST_CLAIM_PORT: u16 = 39_101;

    struct TestDaemon {
        server: Arc<DaemonServer>,
        _claim: TcpListener,
    }

    fn spawn_in_process_daemon(claim_port: u16) -> TestDaemon {
        let server = Arc::new(
            DaemonServer::bind(Limits::default(), claim_port).expect("binds envelope server"),
        );
        let claim = TcpListener::bind(("127.0.0.1", claim_port)).expect("binds claim probe");
        TestDaemon {
            server,
            _claim: claim,
        }
    }

    fn serve_one(test: &TestDaemon) {
        let server = Arc::clone(&test.server);
        let conn = server
            .take_connection()
            .expect("authenticated connection appears");
        std::thread::spawn(move || server.serve_connection(conn));
    }

    /// Serve every connection as it appears, deduped by connection id
    /// (mirrors the daemon main.rs serve loop; needed by the connect_shell
    /// path where the handshake and the negotiation happen inside one
    /// call).
    fn serve_all(test: &TestDaemon) {
        let server = Arc::clone(&test.server);
        std::thread::spawn(move || {
            let mut served = std::collections::HashSet::new();
            loop {
                for conn in server.connections() {
                    if served.insert(conn.id()) {
                        let server = Arc::clone(&server);
                        std::thread::spawn(move || server.serve_connection(conn));
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
    }

    #[test]
    fn client_negotiates_invokes_and_receives_events() {
        let daemon = spawn_in_process_daemon(TEST_CLAIM_PORT);
        let credential = daemon.server.issue_credential(Duration::from_secs(3600));
        let transport = dsh_local_transport::LocalClient::connect(
            daemon.server.addr(),
            &credential,
            &Limits::default(),
        )
        .expect("handshake");
        serve_one(&daemon);
        let (client, events) =
            DaemonClient::connect_transport(transport).expect("connect + negotiate");
        assert!(client.activation_id().starts_with("act-"));

        // terminal.status on an empty host: empty session list.
        let status = client
            .invoke(
                ProtocolCoordinate {
                    api_version: dsh_daemon::capabilities::TERMINAL_API_VERSION.into(),
                    kind: dsh_daemon::capabilities::TERMINAL_KIND.into(),
                },
                dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                serde_json::json!({}),
            )
            .expect("terminal.status");
        assert_eq!(status["count"], 0);
        assert_eq!(status["sessions"].as_array().map(Vec::len), Some(0));

        // browser.create publishes a session-created event to the bridge.
        let created = client
            .invoke(
                ProtocolCoordinate {
                    api_version: dsh_daemon::capabilities::BROWSER_API_VERSION.into(),
                    kind: dsh_daemon::capabilities::BROWSER_KIND.into(),
                },
                dsh_daemon::capabilities::BROWSER_CREATE_METHOD,
                serde_json::json!({ "schemaVersion": 1, "mode": "human_surface" }),
            )
            .expect("browser.create");
        let session_id = created["sessionId"]
            .as_str()
            .expect("session id")
            .to_string();
        assert!(session_id.starts_with("brw-"));

        let event = events
            .recv_timeout(Duration::from_secs(2))
            .expect("created event routed to the client");
        assert_eq!(event.kind, EnvelopeKind::Event);
        assert_eq!(event.method.as_deref(), Some("browser.session-created"));
        assert_eq!(
            event.payload.as_ref().and_then(|p| p["sessionId"].as_str()),
            Some(session_id.as_str())
        );

        // browser.list sees the daemon-owned session.
        let listed = client
            .invoke(
                ProtocolCoordinate {
                    api_version: dsh_daemon::capabilities::BROWSER_API_VERSION.into(),
                    kind: dsh_daemon::capabilities::BROWSER_KIND.into(),
                },
                dsh_daemon::capabilities::BROWSER_LIST_METHOD,
                serde_json::json!({}),
            )
            .expect("browser.list");
        assert_eq!(listed["browsers"].as_array().map(Vec::len), Some(1));

        // browser.close publishes the closed event.
        client
            .invoke(
                ProtocolCoordinate {
                    api_version: dsh_daemon::capabilities::BROWSER_API_VERSION.into(),
                    kind: dsh_daemon::capabilities::BROWSER_KIND.into(),
                },
                dsh_daemon::capabilities::BROWSER_CLOSE_METHOD,
                serde_json::json!({ "schemaVersion": 1, "sessionId": session_id }),
            )
            .expect("browser.close");
        let event = events
            .recv_timeout(Duration::from_secs(2))
            .expect("closed event routed to the client");
        assert_eq!(event.method.as_deref(), Some("browser.session-closed"));

        // runtime.status for an environment outside the catalog fails
        // fail-closed with UNAVAILABLE.
        let error = client
            .invoke(
                ProtocolCoordinate {
                    api_version: dsh_daemon::capabilities::RUNTIME_API_VERSION.into(),
                    kind: dsh_daemon::capabilities::RUNTIME_KIND.into(),
                },
                dsh_daemon::capabilities::RUNTIME_STATUS_METHOD,
                serde_json::json!({ "schemaVersion": 1, "environmentId": "not-in-catalog" }),
            )
            .expect_err("unknown environment");
        assert!(matches!(
            error,
            DaemonCommandError::Remote {
                code: ErrorCode::Unavailable,
                ..
            }
        ));
        client.shutdown();
    }

    #[test]
    fn connect_shell_reads_credential_file_and_connects() {
        let claim_port = TEST_CLAIM_PORT + 1;
        let daemon = spawn_in_process_daemon(claim_port);
        let dir = std::env::temp_dir().join(format!("dsh-client-startup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        serve_all(&daemon);

        // The credential file carries an unused one-time credential (the
        // one-time property is AC-IPC-001: a second handshake with the
        // same credential is rejected as replay).
        let credential = daemon.server.issue_credential(Duration::from_secs(3600));
        let file = CredentialFile::new(
            "0.1.0-test",
            1234,
            claim_port,
            daemon.server.addr().port(),
            credential.token(),
            credential.expires_at(),
            SystemTime::now(),
        );
        file.write_to(&dir).expect("writes credential file");

        // Full startup path: claim probe + credential read + RFC 3339
        // expiry parse + envelope connect/negotiate + first invocation.
        let (client, _events) = connect_shell(&StartupOptions {
            data_dir: dir.clone(),
            claim_port,
            daemon_exe: None,
            spawn_daemon: false,
        })
        .expect("startup connect");
        assert!(client.activation_id().starts_with("act-"));
        let status = client
            .invoke(
                ProtocolCoordinate {
                    api_version: dsh_daemon::capabilities::TERMINAL_API_VERSION.into(),
                    kind: dsh_daemon::capabilities::TERMINAL_KIND.into(),
                },
                dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                serde_json::json!({}),
            )
            .expect("terminal.status over the startup connection");
        assert_eq!(status["count"], 0);
        client.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn connect_shell_fails_closed_when_no_daemon_and_spawn_disabled() {
        // Claim probe fails (no listener) and spawning is disabled: the
        // startup must fail closed instead of hanging.
        let claim_port = TEST_CLAIM_PORT + 2;
        let dir = std::env::temp_dir().join(format!("dsh-client-nodaemon-{}", std::process::id()));
        let error = connect_shell(&StartupOptions {
            data_dir: dir.clone(),
            claim_port,
            daemon_exe: None,
            spawn_daemon: false,
        })
        .expect_err("no daemon");
        assert!(matches!(error, DaemonStartupError::SpawnFailed(_)));
    }
    /// HIGH-1 regression (REVIEW-M6-DAEMON): after the Shell disconnects,
    /// the surviving daemon re-issues the bootstrap credential file; the
    /// next connect_shell re-reads it and re-attaches. This is the M6
    /// core exit criterion — Shell restart must not lose the daemon.
    #[test]
    fn reconnect_reads_reissued_credential_after_disconnect() {
        let claim_port = TEST_CLAIM_PORT + 3;
        let dir = std::env::temp_dir().join(format!("dsh-client-reconnect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let catalog = dir.join("environments.json");
        let server = Arc::new(
            DaemonServer::bind_with_catalog(Limits::default(), claim_port, catalog)
                .expect("binds envelope server"),
        );
        let claim = TcpListener::bind(("127.0.0.1", claim_port)).expect("binds claim probe");

        // main.rs startup shape: the daemon wrote a bootstrap credential
        // file before serving.
        let startup = server.issue_credential(Duration::from_secs(3600));
        CredentialFile::new(
            "0.1.0-test",
            std::process::id(),
            claim_port,
            server.addr().port(),
            startup.token(),
            startup.expires_at(),
            SystemTime::now(),
        )
        .write_to(&dir)
        .expect("writes credential file");
        let original_token = CredentialFile::read_from(&dir)
            .expect("read credential file")
            .credential
            .token;

        // Serve every connection (mirrors daemon main.rs).
        {
            let server = Arc::clone(&server);
            std::thread::spawn(move || {
                let mut served = std::collections::HashSet::new();
                loop {
                    for conn in server.connections() {
                        if served.insert(conn.id()) {
                            let server = Arc::clone(&server);
                            std::thread::spawn(move || server.serve_connection(conn));
                        }
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
        }
        let options = StartupOptions {
            data_dir: dir.clone(),
            claim_port,
            daemon_exe: None,
            spawn_daemon: false,
        };

        // First Shell start: connects with the startup credential.
        let (client, _events) = connect_shell(&options).expect("first startup connect");
        assert!(client.activation_id().starts_with("act-"));
        client.shutdown();

        // Disconnect → the daemon re-issues and rewrites the file.
        let deadline = Instant::now() + Duration::from_secs(5);
        let fresh_token = loop {
            if let Ok(file) = CredentialFile::read_from(&dir)
                && file.credential.token != original_token
            {
                break Some(file.credential.token);
            }
            if Instant::now() >= deadline {
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(
            fresh_token.is_some(),
            "daemon re-issued the credential file"
        );

        // A restarted Shell re-reads the file and re-attaches to the
        // surviving daemon (the consumed startup token would be replay).
        let (client2, _events2) = connect_shell(&options).expect("reconnect after restart");
        assert!(client2.activation_id().starts_with("act-"));
        let status = client2
            .invoke(
                ProtocolCoordinate {
                    api_version: dsh_daemon::capabilities::TERMINAL_API_VERSION.into(),
                    kind: dsh_daemon::capabilities::TERMINAL_KIND.into(),
                },
                dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                serde_json::json!({}),
            )
            .expect("terminal.status over the reconnected startup connection");
        assert_eq!(status["count"], 0);
        client2.shutdown();
        drop(claim);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End-to-end auto-reconnect: the inner connection dies (client
    /// shutdown simulates the daemon-side loss the Shell observes), the
    /// daemon re-issues the credential file, and the next invocation
    /// reconnects through [connect_shell] and is retried successfully.
    #[test]
    fn auto_reconnect_replaces_dead_connection_and_retries_invoke() {
        let claim_port = TEST_CLAIM_PORT + 4;
        let dir =
            std::env::temp_dir().join(format!("dsh-client-autoreconnect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let catalog = dir.join("environments.json");
        let server = Arc::new(
            DaemonServer::bind_with_catalog(Limits::default(), claim_port, catalog)
                .expect("binds envelope server"),
        );
        let claim = TcpListener::bind(("127.0.0.1", claim_port)).expect("binds claim probe");
        let startup = server.issue_credential(Duration::from_secs(3600));
        CredentialFile::new(
            "0.1.0-test",
            std::process::id(),
            claim_port,
            server.addr().port(),
            startup.token(),
            startup.expires_at(),
            SystemTime::now(),
        )
        .write_to(&dir)
        .expect("writes credential file");
        let original_token = CredentialFile::read_from(&dir)
            .expect("read credential file")
            .credential
            .token;

        // Serve every connection (mirrors daemon main.rs).
        {
            let server = Arc::clone(&server);
            std::thread::spawn(move || {
                let mut served = std::collections::HashSet::new();
                loop {
                    for conn in server.connections() {
                        if served.insert(conn.id()) {
                            let server = Arc::clone(&server);
                            std::thread::spawn(move || server.serve_connection(conn));
                        }
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
        }
        let options = StartupOptions {
            data_dir: dir.clone(),
            claim_port,
            daemon_exe: None,
            spawn_daemon: false,
        };

        let (client, _events) = connect_shell(&options).expect("first startup connect");
        let first_client = Arc::new(client);
        let attempts = Arc::new(AtomicUsize::new(0));
        let wrapper = AutoReconnectConnector::with_base_interval(
            Arc::clone(&first_client) as Arc<dyn DaemonConnector>,
            Box::new({
                let options = options.clone();
                let attempts = Arc::clone(&attempts);
                move || -> Result<Arc<dyn DaemonConnector>, DaemonStartupError> {
                    attempts.fetch_add(1, Ordering::Relaxed);
                    connect_shell(&options)
                        .map(|(client, _events)| Arc::new(client) as Arc<dyn DaemonConnector>)
                }
            }),
            Duration::ZERO,
        );
        let status = wrapper
            .invoke(
                terminal_coordinate(),
                dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                terminal_status_payload(),
            )
            .expect("first invoke over the startup connection");
        assert_eq!(status["count"], 0);

        // The connection dies: the worker exits and drops the transport;
        // the daemon serve loop observes the disconnect and re-issues the
        // credential file for the next handshake.
        first_client.shutdown();
        let deadline = Instant::now() + Duration::from_secs(5);
        let fresh_token = loop {
            if let Ok(file) = CredentialFile::read_from(&dir)
                && file.credential.token != original_token
            {
                break Some(file.credential.token);
            }
            if Instant::now() >= deadline {
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(
            fresh_token.is_some(),
            "daemon re-issued the credential file after the disconnect"
        );

        // The next invocation fails connection-class on the dead client,
        // auto-reconnects through connect_shell and is retried on the
        // fresh connection.
        let status = wrapper
            .invoke(
                terminal_coordinate(),
                dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                terminal_status_payload(),
            )
            .expect("auto-reconnected invoke");
        assert_eq!(status["count"], 0);
        assert_eq!(attempts.load(Ordering::Relaxed), 1, "one reconnect");

        // The wrapper keeps serving from the fresh connection.
        let status = wrapper
            .invoke(
                terminal_coordinate(),
                dsh_daemon::capabilities::TERMINAL_STATUS_METHOD,
                terminal_status_payload(),
            )
            .expect("fresh connection still serves");
        assert_eq!(status["count"], 0);
        assert_eq!(attempts.load(Ordering::Relaxed), 1, "no further reconnect");
        drop(claim);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
