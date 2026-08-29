//! Supervised loopback TCP server with ephemeral-credential authentication.
//!
//! The server owns the endpoint lifecycle: it binds a random port on
//! `127.0.0.1`, issues one-time ephemeral credentials, authenticates every
//! connection with a framed handshake, and supervises each connection with
//! frame limits, read/write deadlines and a concurrency cap
//! (AC-IPC-001 / AC-IPC-002).

use std::collections::HashMap;
use std::io::{self, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use crate::credential::{AuthError, Credential, CredentialIssuer};
use crate::error::{TransportError, is_timeout_kind};
use crate::framing::{FrameReadError, encode_frame, read_frame};
use crate::handshake::{ClientHello, ServerHello};
use crate::limits::Limits;

/// Sleep between non-blocking accept attempts.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Idle tick of connection workers (also the write-queue service interval).
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Bound on per-connection queued frames in either direction.
const QUEUED_FRAMES: usize = 16;

/// Observable supervision counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerStats {
    /// TCP connections accepted (including rejected ones).
    pub accepted: u64,
    /// Handshakes that authenticated successfully.
    pub authenticated: u64,
    /// Handshakes rejected (invalid / replay / stale / malformed credential).
    pub rejected_auth: u64,
    /// Connections refused because the concurrency limit was reached.
    pub rejected_busy: u64,
    /// Established connections that ended with a clean peer EOF.
    pub closed_peer: u64,
    /// Established connections that ended with a framing violation.
    pub closed_protocol: u64,
    /// Established connections that ended on a read or write deadline.
    pub closed_timeout: u64,
    /// Established connections that ended on another I/O error.
    pub closed_io: u64,
    /// Live connections right now.
    pub active: usize,
    /// Ephemeral credentials issued so far.
    pub credentials_issued: u64,
    /// Credentials consumed by a successful handshake.
    pub credentials_consumed: u64,
}

/// A loopback server listening on a random `127.0.0.1` port.
#[derive(Debug)]
pub struct LocalServer {
    state: Arc<ServerState>,
    addr: SocketAddr,
    accept_handle: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct ServerState {
    limits: Limits,
    shutdown: AtomicBool,
    next_conn_id: AtomicU64,
    credentials: Mutex<HashMap<String, IssuedCredential>>,
    conns: Mutex<HashMap<u64, ServerConn>>,
    limiter: ConnLimiter,
    stats: Mutex<ServerStats>,
    issuer: CredentialIssuer,
}

#[derive(Debug)]
struct IssuedCredential {
    expires_at: SystemTime,
    used: bool,
}

/// Simple atomic connection-count limiter (AC-IPC-002 concurrency cap).
#[derive(Debug)]
struct ConnLimiter {
    current: AtomicUsize,
    max: usize,
}

impl ConnLimiter {
    fn new(max: usize) -> Self {
        Self {
            current: AtomicUsize::new(0),
            max,
        }
    }

    fn try_acquire(&self) -> bool {
        let mut observed = self.current.load(Ordering::Relaxed);
        loop {
            if observed >= self.max {
                return false;
            }
            match self.current.compare_exchange_weak(
                observed,
                observed + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => observed = actual,
            }
        }
    }

    fn release(&self) {
        self.current.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Server-side handle to one authenticated connection.
#[derive(Debug, Clone)]
pub struct ServerConn {
    id: u64,
    peer: SocketAddr,
    max_frame: usize,
    write_tx: mpsc::SyncSender<WriteRequest>,
    read_rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

/// A payload queued to the connection worker, with an acknowledgement channel.
#[derive(Debug)]
struct WriteRequest {
    payload: Vec<u8>,
    ack: mpsc::Sender<Result<(), TransportError>>,
}

/// Why an established connection ended.
enum EndReason {
    PeerClosed,
    Protocol,
    Timeout,
    Io,
}

/// Outcome of the handshake phase.
enum HandshakeResult {
    Authenticated,
    Rejected(AuthError),
    Closed,
    TimedOut,
    Protocol,
    Io,
}

impl LocalServer {
    /// Bind a new server to a random `127.0.0.1` port and start accepting.
    pub fn bind(limits: Limits) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let state = Arc::new(ServerState {
            limits,
            shutdown: AtomicBool::new(false),
            next_conn_id: AtomicU64::new(1),
            credentials: Mutex::new(HashMap::new()),
            conns: Mutex::new(HashMap::new()),
            limiter: ConnLimiter::new(limits.max_connections),
            stats: Mutex::new(ServerStats::default()),
            issuer: CredentialIssuer::new(),
        });
        let accept_handle = {
            let accept_state = Arc::clone(&state);
            thread::spawn(move || accept_loop(accept_state, listener))
        };
        Ok(Self {
            state,
            addr,
            accept_handle: Some(accept_handle),
        })
    }

    /// The bound loopback address clients connect to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Issue a fresh one-time ephemeral credential valid for `ttl`.
    pub fn issue_credential(&self, ttl: Duration) -> Credential {
        let credential = self.state.issuer.issue(ttl);
        {
            let mut registry = self.state.credentials.lock().unwrap();
            // Bounded sweep: drop consumed or expired entries so the registry
            // cannot grow without bound (FH-2, AC-IPC-002 cleanup).
            let now = SystemTime::now();
            registry.retain(|_, entry| !entry.used && entry.expires_at > now);
            registry.insert(
                credential.token().to_string(),
                IssuedCredential {
                    expires_at: credential.expires_at(),
                    used: false,
                },
            );
        }
        self.state.stats.lock().unwrap().credentials_issued += 1;
        credential
    }

    /// Snapshot of the supervision counters.
    pub fn stats(&self) -> ServerStats {
        let mut stats = *self.state.stats.lock().unwrap();
        stats.active = self.state.limiter.current.load(Ordering::Relaxed);
        stats
    }

    /// Handles to the currently authenticated connections.
    pub fn connections(&self) -> Vec<ServerConn> {
        self.state.conns.lock().unwrap().values().cloned().collect()
    }

    /// Stop the accept loop. Idempotent; also called from `Drop`.
    pub fn stop(&mut self) {
        if let Some(handle) = self.accept_handle.take() {
            self.state.shutdown.store(true, Ordering::Relaxed);
            let _ = handle.join();
        }
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stop();
    }
}

impl ServerConn {
    /// Unique connection id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Peer address of the connected client.
    pub fn peer(&self) -> SocketAddr {
        self.peer
    }

    /// Block until the client sends one frame; `None` once the client is gone.
    pub fn recv(&self) -> Option<Vec<u8>> {
        self.read_rx.lock().ok()?.recv().ok()
    }

    /// Like [`recv`](Self::recv) but gives up after `timeout`.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Vec<u8>> {
        self.read_rx.lock().ok()?.recv_timeout(timeout).ok()
    }

    /// Send one frame to the client (rejected locally when over the limit).
    pub fn send(&self, payload: &[u8]) -> Result<(), TransportError> {
        if payload.len() > self.max_frame {
            return Err(TransportError::Oversized {
                declared: payload.len() as u64,
                max: self.max_frame,
            });
        }
        let (ack_tx, ack_rx) = mpsc::channel();
        self.write_tx
            .send(WriteRequest {
                payload: payload.to_vec(),
                ack: ack_tx,
            })
            .map_err(|_| TransportError::Closed)?;
        // Bounded queue: the worker drains it every tick; if the client stops
        // draining, the worker's write deadline closes the connection and this
        // receive unblocks with an error.
        ack_rx.recv().map_err(|_| TransportError::Closed)?
    }

    /// Serialize and send one message to the client.
    pub fn send_json<T: serde::Serialize>(&self, value: &T) -> Result<(), TransportError> {
        self.send(&serde_json::to_vec(value)?)
    }

    /// Block for one deserialized message; `Ok(None)` once the client is gone.
    pub fn recv_json<T: serde::de::DeserializeOwned>(&self) -> Result<Option<T>, TransportError> {
        match self.recv() {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Like [`recv_json`](Self::recv_json) but gives up after `timeout`.
    pub fn recv_json_timeout<T: serde::de::DeserializeOwned>(
        &self,
        timeout: Duration,
    ) -> Result<Option<T>, TransportError> {
        match self.recv_timeout(timeout) {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }
}

fn accept_loop(state: Arc<ServerState>, listener: TcpListener) {
    loop {
        if state.shutdown.load(Ordering::Relaxed) {
            return;
        }
        match listener.accept() {
            Ok((stream, peer)) => {
                state.stats.lock().unwrap().accepted += 1;
                if state.limiter.try_acquire() {
                    let worker_state = Arc::clone(&state);
                    thread::spawn(move || connection_worker(worker_state, stream, peer));
                } else {
                    state.stats.lock().unwrap().rejected_busy += 1;
                    reject_busy(Arc::clone(&state), stream);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL_INTERVAL),
            Err(_) => thread::sleep(ACCEPT_POLL_INTERVAL),
        }
    }
}

/// Best-effort reply to a connection refused because the server is busy.
/// Written inline (bounded by the handshake deadline) so a flood of
/// rejected connections cannot spawn one thread each (FH-2, AC-IPC-002).
fn reject_busy(state: Arc<ServerState>, mut stream: TcpStream) {
    let _ = stream.set_write_timeout(Some(state.limits.handshake_deadline));
    let _ = write_handshake_reply(&mut stream, false, Some("busy"));
}

/// Runs one accepted connection: handshake, then supervised message loop.
fn connection_worker(state: Arc<ServerState>, mut stream: TcpStream, peer: SocketAddr) {
    let limits = state.limits;
    let id = state.next_conn_id.fetch_add(1, Ordering::Relaxed);

    // --- Handshake phase (bounded by handshake_deadline) ---
    let _ = stream.set_read_timeout(Some(limits.handshake_deadline));
    let _ = stream.set_write_timeout(Some(limits.handshake_deadline));

    match perform_handshake(&state, &mut stream) {
        HandshakeResult::Authenticated => {}
        HandshakeResult::Rejected(auth_error) => {
            let _ = write_handshake_reply(&mut stream, false, Some(reason_str(auth_error)));
            state.stats.lock().unwrap().rejected_auth += 1;
            state.limiter.release();
            return;
        }
        HandshakeResult::Closed => {
            state.stats.lock().unwrap().closed_peer += 1;
            state.limiter.release();
            return;
        }
        HandshakeResult::TimedOut => {
            state.stats.lock().unwrap().closed_timeout += 1;
            state.limiter.release();
            return;
        }
        HandshakeResult::Protocol => {
            let _ = write_handshake_reply(&mut stream, false, Some("malformed"));
            state.stats.lock().unwrap().closed_protocol += 1;
            state.limiter.release();
            return;
        }
        HandshakeResult::Io => {
            state.stats.lock().unwrap().closed_io += 1;
            state.limiter.release();
            return;
        }
    }

    state.stats.lock().unwrap().authenticated += 1;

    // --- Established phase (per-frame read deadline + write deadline) ---
    let _ = stream.set_write_timeout(Some(limits.write_deadline));

    // Bounded queues (FH-2, AC-IPC-002): a client that never drains its
    // frames or responses must not grow memory without bound.
    let (read_tx, read_rx) = mpsc::sync_channel::<Vec<u8>>(QUEUED_FRAMES);
    let (write_tx, write_rx) = mpsc::sync_channel::<WriteRequest>(QUEUED_FRAMES);
    let conn = ServerConn {
        id,
        peer,
        max_frame: limits.max_frame_bytes,
        write_tx,
        read_rx: Arc::new(Mutex::new(read_rx)),
    };
    state.conns.lock().unwrap().insert(id, conn.clone());

    // Register before acknowledging so a client never observes a missing conn.
    if !write_handshake_reply(&mut stream, true, None) {
        state.conns.lock().unwrap().remove(&id);
        state.limiter.release();
        state.stats.lock().unwrap().closed_io += 1;
        return;
    }

    let end = worker_loop(&mut stream, &read_tx, &write_rx, limits);

    state.conns.lock().unwrap().remove(&id);
    state.limiter.release();
    match end {
        EndReason::PeerClosed => state.stats.lock().unwrap().closed_peer += 1,
        EndReason::Protocol => state.stats.lock().unwrap().closed_protocol += 1,
        EndReason::Timeout => state.stats.lock().unwrap().closed_timeout += 1,
        EndReason::Io => state.stats.lock().unwrap().closed_io += 1,
    }
}

/// Reads the client hello and checks it against the credential registry.
fn perform_handshake(state: &ServerState, stream: &mut TcpStream) -> HandshakeResult {
    match read_frame(stream, state.limits.max_frame_bytes) {
        Ok(Some(bytes)) => match serde_json::from_slice::<ClientHello>(&bytes) {
            Ok(hello) => match authenticate(state, &hello.token) {
                Ok(()) => HandshakeResult::Authenticated,
                Err(auth_error) => HandshakeResult::Rejected(auth_error),
            },
            Err(_) => HandshakeResult::Rejected(AuthError::Malformed),
        },
        Ok(None) => HandshakeResult::Closed,
        Err(FrameReadError::Io(e)) if is_timeout_kind(e.kind()) => HandshakeResult::TimedOut,
        Err(FrameReadError::Io(_)) => HandshakeResult::Io,
        Err(FrameReadError::Frame(_)) => HandshakeResult::Protocol,
    }
}

/// Validate an ephemeral credential: format, expiry, single-use (AC-IPC-001).
fn authenticate(state: &ServerState, token: &str) -> Result<(), AuthError> {
    if !CredentialIssuer::is_valid_format(token) {
        return Err(AuthError::Malformed);
    }
    let mut registry = state.credentials.lock().unwrap();
    let now = SystemTime::now();
    let Some(entry) = registry.get_mut(token) else {
        return Err(AuthError::Invalid);
    };
    if entry.expires_at <= now {
        registry.remove(token);
        return Err(AuthError::Stale);
    }
    if entry.used {
        return Err(AuthError::Replay);
    }
    entry.used = true;
    state.stats.lock().unwrap().credentials_consumed += 1;
    Ok(())
}

fn reason_str(auth_error: AuthError) -> &'static str {
    match auth_error {
        AuthError::Invalid => "invalid",
        AuthError::Replay => "replay",
        AuthError::Stale => "stale",
        AuthError::Malformed => "malformed",
    }
}

/// Write a framed `ServerHello`. Returns `false` when the write failed.
fn write_handshake_reply(stream: &mut TcpStream, accepted: bool, reason: Option<&str>) -> bool {
    let reply = ServerHello {
        accepted,
        reason: reason.map(str::to_string),
    };
    match serde_json::to_vec(&reply) {
        Ok(bytes) => write_frame_all(stream, &bytes).is_ok(),
        Err(_) => false,
    }
}

/// Supervised message loop: framing check, read deadline, write queue (AC-IPC-002).
fn worker_loop(
    stream: &mut TcpStream,
    read_tx: &mpsc::SyncSender<Vec<u8>>,
    write_rx: &mpsc::Receiver<WriteRequest>,
    limits: Limits,
) -> EndReason {
    let mut last_activity = Instant::now();
    loop {
        // Poll reads in small ticks so queued writes stay serviced while idle.
        let remaining = limits.read_deadline.saturating_sub(last_activity.elapsed());
        let tick = remaining.min(WORKER_POLL_INTERVAL);
        if stream.set_read_timeout(Some(tick)).is_err() {
            return EndReason::Io;
        }
        match read_frame(stream, limits.max_frame_bytes) {
            Ok(Some(frame)) => {
                last_activity = Instant::now();
                // Non-blocking enqueue: an app that never drains its receive
                // queue is a slow consumer; close it rather than stall the
                // worker and defeat the read deadline (AC-IPC-002).
                if read_tx.try_send(frame).is_err() {
                    return EndReason::Timeout;
                }
            }
            Ok(None) => return EndReason::PeerClosed,
            Err(FrameReadError::Io(e)) if is_timeout_kind(e.kind()) => {
                if last_activity.elapsed() >= limits.read_deadline {
                    return EndReason::Timeout;
                }
            }
            Err(FrameReadError::Io(_)) => return EndReason::Io,
            Err(FrameReadError::Frame(_)) => return EndReason::Protocol,
        }

        // Drain the pending write queue without blocking the read loop.
        loop {
            match write_rx.try_recv() {
                Ok(request) => {
                    let write_result = write_frame_all(stream, &request.payload);
                    let _ = request.ack.send(
                        write_result
                            .as_ref()
                            .map_err(|e| {
                                TransportError::Io(io::Error::new(e.kind(), e.to_string()))
                            })
                            .copied(),
                    );
                    if let Err(e) = write_result {
                        return if is_timeout_kind(e.kind()) {
                            EndReason::Timeout
                        } else {
                            EndReason::Io
                        };
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
    }
}

fn write_frame_all(stream: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    stream.write_all(&encode_frame(payload))
}
