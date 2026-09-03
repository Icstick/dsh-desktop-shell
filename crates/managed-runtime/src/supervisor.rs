//! The Managed DSH runtime supervisor: the P0 process-manager core
//! (MOD-PROCESS-MANAGER, ADR-0008/0012/0013/0019 decision 3), migrated
//! out of the tauri Shell into the standalone dsh-managed-runtime crate
//! in M6-C2.
//!
//! Owns exactly one DSH process tree at a time (retained handle +
//! generation + tree cleanup + endpoint release semantics, ADR-0019
//! decision 3 MOD-PROCESS-MANAGER extraction requirements):
//!
//! - generation-guarded lifecycle: start/status/stop/restart against a
//!   ManagedEnvironment from the persisted environment catalog;
//! - readiness publication gates: the owned generation must emit an
//!   exact loopback endpoint and answer a bounded HTTP probe (FM-1)
//!   before the state becomes Healthy;
//! - crash recovery: window-bounded auto-restart with a hard budget that
//!   latches Safe Stop (AC-REC-001);
//! - process-tree ownership: Windows Job Object with KILL_ON_JOB_CLOSE /
//!   unix process groups, so a stop or a daemon exit cannot leak DSH
//!   children;
//! - surface-binding security (ADR-0012/0013): a verified binding is
//!   only handed out when retained tree, generation, verified endpoint
//!   and the private bootstrap URL all line up; callers never supply
//!   endpoint or URL.
//!
//! This crate deliberately does NOT depend on tauri (the daemon and the
//! Shell share it); the unsafe Windows Job Object code is confined to
//! the process-tree section below.

use std::env;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::environment::{HarnessMode, ManagedEnvironment};

const SCHEMA_VERSION: u8 = 1;
const OUTPUT_MARKER: &str = "dsh web:";
const MAX_OUTPUT_LINE_BYTES: usize = 2048;
const START_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const ENDPOINT_RELEASE_TIMEOUT: Duration = Duration::from_secs(1);
const RECOVERY_BUDGET: usize = 3;
const RECOVERY_WINDOW: Duration = Duration::from_secs(60);
const RESTART_BACKOFF: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedRuntimeStartRequest {
    schema_version: u8,
    environment_id: String,
}

impl ManagedRuntimeStartRequest {
    pub fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub fn environment_id(&self) -> &str {
        &self.environment_id
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedRuntimeStatusRequest {
    schema_version: u8,
    environment_id: String,
}

impl ManagedRuntimeStatusRequest {
    pub fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub fn environment_id(&self) -> &str {
        &self.environment_id
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedRuntimeRestartRequest {
    schema_version: u8,
    environment_id: String,
    expected_generation: u64,
}

impl ManagedRuntimeRestartRequest {
    pub fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub fn environment_id(&self) -> &str {
        &self.environment_id
    }

    pub fn expected_generation(&self) -> u64 {
        self.expected_generation
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedRuntimeStopRequest {
    schema_version: u8,
    environment_id: String,
    expected_generation: u64,
}

impl ManagedRuntimeStopRequest {
    pub fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub fn environment_id(&self) -> &str {
        &self.environment_id
    }

    pub fn expected_generation(&self) -> u64 {
        self.expected_generation
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeReport {
    schema_version: u8,
    environment_id: String,
    ownership: &'static str,
    state: ManagedState,
    generation: u64,
    instance_id: Option<String>,
    process_ownership: ProcessOwnership,
    lifecycle_mutation: &'static str,
    readiness: Readiness,
    endpoint: Option<ManagedEndpoint>,
    stop_disposition: StopDisposition,
    recovery: Option<RecoveryReport>,
    observed_at_unix_ms: u64,
    evidence: Vec<RuntimeEvidence>,
}

impl ManagedRuntimeReport {
    pub fn runtime_state(&self) -> &'static str {
        self.state.as_str()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ManagedState {
    Stopped,
    Starting,
    Healthy,
    Stopping,
    Crashed,
    SafeStop,
}

impl ManagedState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Healthy => "healthy",
            Self::Stopping => "stopping",
            Self::Crashed => "crashed",
            Self::SafeStop => "safe_stop",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProcessOwnership {
    None,
    Owned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Readiness {
    NotStarted,
    Waiting,
    Verified,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum StopDisposition {
    NotRequested,
    Graceful,
    Forced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecoveryState {
    crash_count: usize,
    window_start_unix_ms: u64,
    last_crash_at_unix_ms: Option<u64>,
    safe_stop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryReport {
    crash_count: usize,
    window_start_unix_ms: u64,
    budget: usize,
    safe_stop: bool,
    last_crash_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct ManagedEndpoint {
    scheme: &'static str,
    host: &'static str,
    port: u16,
    source: &'static str,
    verification: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSurfaceBinding {
    generation: u64,
    port: u16,
    bootstrap_url: Url,
}

impl VerifiedSurfaceBinding {
    /// Build a binding from verified parts (M6-C4: the Shell re-derives the
    /// binding from the daemon `runtime.status` report — the daemon is the
    /// supervisor authority since M6-C2; only the daemon may *verify* a
    /// binding, the Shell only re-materializes the verified report).
    pub fn new(generation: u64, port: u16, bootstrap_url: Url) -> Self {
        Self {
            generation,
            port,
            bootstrap_url,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn url(&self) -> Url {
        self.bootstrap_url.clone()
    }
}

#[derive(Debug)]
struct ParsedCandidate {
    endpoint: ManagedEndpoint,
    bootstrap_url: Option<Url>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct RuntimeEvidence {
    code: &'static str,
    severity: &'static str,
    message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedRuntimeError {
    NotManaged,
    InvalidEnvironment,
    UnsupportedSource,
    NodeOverrideUnsupported,
    SpawnUnavailable,
    ProcessTreeUnavailable,
    Conflict,
    StaleGeneration,
    CandidateInvalid,
    CandidatePortMismatch,
    ProcessExited,
    ReadinessTimeout,
    StopFailed,
    EndpointStillReachable,
    SurfaceBindingUnavailable,
    StateUnavailable,
    ClockUnavailable,
}

#[derive(Clone, Default)]
pub struct ManagedRuntimeState {
    inner: Arc<Mutex<Supervisor>>,
}

impl ManagedRuntimeState {
    /// The environment id the Supervisor currently owns, if any (the
    /// Supervisor owns exactly one environment at a time, FM-4; the id
    /// survives a clean stop so the ownership slot stays visible).
    pub fn current_environment_id(&self) -> Option<String> {
        self.inner
            .lock()
            .ok()
            .and_then(|supervisor| supervisor.environment_id.clone())
    }

    /// Whether a live generation is currently retained (a process tree
    /// exists — Starting/Healthy/Stopping; not Stopped/SafeStop after
    /// cleanup). Used for daemon.status resources.managedRuntimes.
    pub fn has_live_generation(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|supervisor| supervisor.process.is_some())
    }
}

#[derive(Default)]
struct Supervisor {
    environment_id: Option<String>,
    state: Option<ManagedState>,
    generation: u64,
    instance_id: Option<String>,
    endpoint: Option<ManagedEndpoint>,
    bootstrap_url: Option<Url>,
    stop_disposition: Option<StopDisposition>,
    evidence: Vec<RuntimeEvidence>,
    process: Option<ManagedProcess>,
    retained_spec: Option<LaunchSpec>,
    auto_restart_on_crash: bool,
    recovery: Option<RecoveryState>,
}

/// The structured launch recipe derived from a ManagedEnvironment
/// (executable, argv, cwd, environment, expected port). Public because
/// tests and future daemon callers start generations from specs.
#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub executable: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub environment: Vec<(OsString, OsString)>,
    pub expected_port: Option<u16>,
}

struct ManagedProcess {
    child: Child,
    tree: ProcessTree,
}

pub fn start_managed_environment(
    state: &ManagedRuntimeState,
    environment: &ManagedEnvironment,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    let spec = build_launch_spec(environment)?;
    let auto_restart_on_crash = environment
        .policy()
        .and_then(|policy| policy.auto_restart_on_crash())
        .unwrap_or(false);
    {
        let mut supervisor = lock_supervisor(state)?;
        supervisor.refresh_exit();
        if supervisor.process.is_some() {
            if supervisor.environment_id.as_deref() == Some(environment.id()) {
                return supervisor.report(environment.id());
            }
            return Err(ManagedRuntimeError::Conflict);
        }
        supervisor.auto_restart_on_crash = auto_restart_on_crash;
        supervisor.retained_spec = Some(spec.clone());
        supervisor.recovery = None;
    }
    start_with_spec(state, environment.id(), spec, START_TIMEOUT)
}

pub fn get_managed_runtime_status(
    state: &ManagedRuntimeState,
    environment: &ManagedEnvironment,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    let spec = {
        let mut supervisor = lock_supervisor(state)?;
        supervisor.refresh_exit();
        // The Supervisor owns exactly one environment at a time; reporting
        // or auto-restarting another environment's recipe under this id would
        // mislabel state and spawn the wrong launch recipe (FM-4).
        if supervisor
            .environment_id
            .as_deref()
            .is_some_and(|id| id != environment.id())
        {
            return Err(ManagedRuntimeError::Conflict);
        }
        supervisor
            .auto_restart_eligible()
            .then(|| supervisor.retained_spec.clone())
            .flatten()
    };
    if let Some(spec) = spec {
        thread::sleep(RESTART_BACKOFF);
        return start_with_spec(state, environment.id(), spec, START_TIMEOUT);
    }
    let supervisor = lock_supervisor(state)?;
    supervisor.report(environment.id())
}

// Security invariant (ADR-0012/0013): a Surface binding is handed out only
// when the current retained process tree, generation, verified endpoint and
// private bootstrap URL all line up. Callers never supply endpoint or URL.
pub fn verified_surface_binding(
    state: &ManagedRuntimeState,
    environment: &ManagedEnvironment,
    expected_generation: u64,
) -> Result<VerifiedSurfaceBinding, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    if expected_generation == 0 {
        return Err(ManagedRuntimeError::StaleGeneration);
    }

    let spec = {
        let mut supervisor = lock_supervisor(state)?;
        supervisor.refresh_exit();
        // Same environment cross-check as status (FM-4): never auto-restart
        // another environment's recipe while looking up this one's binding.
        if supervisor
            .environment_id
            .as_deref()
            .is_some_and(|id| id != environment.id())
        {
            return Err(ManagedRuntimeError::Conflict);
        }
        supervisor
            .auto_restart_eligible()
            .then(|| supervisor.retained_spec.clone())
            .flatten()
    };
    if let Some(spec) = spec {
        thread::sleep(RESTART_BACKOFF);
        let _ = start_with_spec(state, environment.id(), spec, START_TIMEOUT);
    }
    let mut supervisor = lock_supervisor(state)?;
    supervisor.refresh_exit();
    if supervisor.generation != expected_generation {
        return Err(ManagedRuntimeError::StaleGeneration);
    }
    if supervisor.environment_id.as_deref() != Some(environment.id())
        || supervisor.state != Some(ManagedState::Healthy)
        || supervisor.process.is_none()
    {
        return Err(ManagedRuntimeError::SurfaceBindingUnavailable);
    }
    let endpoint = supervisor
        .endpoint
        .ok_or(ManagedRuntimeError::SurfaceBindingUnavailable)?;
    let bootstrap_url = supervisor
        .bootstrap_url
        .clone()
        .ok_or(ManagedRuntimeError::SurfaceBindingUnavailable)?;

    Ok(VerifiedSurfaceBinding {
        generation: supervisor.generation,
        port: endpoint.port,
        bootstrap_url,
    })
}

pub fn stop_managed_environment(
    state: &ManagedRuntimeState,
    environment: &ManagedEnvironment,
    expected_generation: u64,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }

    let (process, endpoint) = {
        let mut supervisor = lock_supervisor(state)?;
        supervisor.refresh_exit();
        if supervisor
            .environment_id
            .as_deref()
            .is_some_and(|id| id != environment.id())
        {
            return Err(ManagedRuntimeError::Conflict);
        }
        if expected_generation == 0 || expected_generation != supervisor.generation {
            return Err(ManagedRuntimeError::StaleGeneration);
        }

        if supervisor.process.is_none() {
            supervisor.state = Some(ManagedState::Stopped);
            supervisor.instance_id = None;
            supervisor.endpoint = None;
            supervisor.bootstrap_url = None;
            supervisor.stop_disposition = Some(StopDisposition::NotRequested);
            supervisor.recovery = None;
            supervisor.evidence = vec![evidence(
                "MANAGED_ALREADY_STOPPED",
                "info",
                "The requested Managed generation has no retained process tree.",
            )];
            return supervisor.report(environment.id());
        }

        supervisor.state = Some(ManagedState::Stopping);
        supervisor.bootstrap_url = None;
        supervisor.evidence = vec![evidence(
            "MANAGED_STOPPING",
            "info",
            "The retained Managed process tree is stopping.",
        )];
        (
            supervisor.process.take().expect("checked process"),
            supervisor.endpoint.take(),
        )
    };

    let disposition = match process.stop() {
        Ok(disposition) => disposition,
        Err(_) => {
            let mut supervisor = lock_supervisor(state)?;
            if supervisor.generation == expected_generation
                && supervisor.environment_id.as_deref() == Some(environment.id())
            {
                supervisor.state = Some(ManagedState::Crashed);
                supervisor.endpoint = None;
                supervisor.bootstrap_url = None;
                supervisor.stop_disposition = Some(StopDisposition::NotRequested);
                supervisor.evidence = vec![evidence(
                    "MANAGED_STOP_FAILED",
                    "error",
                    "The retained process-tree stop did not complete cleanly.",
                )];
            }
            return Err(ManagedRuntimeError::StopFailed);
        }
    };
    let endpoint_released = endpoint.is_none_or(|value| endpoint_is_released(value.port));

    let mut supervisor = lock_supervisor(state)?;
    if supervisor.generation != expected_generation
        || supervisor.environment_id.as_deref() != Some(environment.id())
    {
        return Err(ManagedRuntimeError::StaleGeneration);
    }
    supervisor.state = Some(ManagedState::Stopped);
    supervisor.instance_id = None;
    supervisor.endpoint = None;
    supervisor.bootstrap_url = None;
    supervisor.stop_disposition = Some(disposition);
    supervisor.recovery = None;
    supervisor.evidence = vec![evidence(
        "MANAGED_PROCESS_TREE_STOPPED",
        "info",
        "The retained Managed process tree stopped and its endpoint was checked.",
    )];
    let report = supervisor.report(environment.id())?;
    if !endpoint_released {
        return Err(ManagedRuntimeError::EndpointStillReachable);
    }
    Ok(report)
}

pub fn restart_managed_environment(
    state: &ManagedRuntimeState,
    environment: &ManagedEnvironment,
    request: ManagedRuntimeRestartRequest,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    let (process, endpoint, retained_spec) = {
        let mut supervisor = lock_supervisor(state)?;
        supervisor.refresh_exit();
        if supervisor
            .environment_id
            .as_deref()
            .is_some_and(|id| id != environment.id())
        {
            return Err(ManagedRuntimeError::Conflict);
        }
        if request.expected_generation == 0 || request.expected_generation != supervisor.generation
        {
            return Err(ManagedRuntimeError::StaleGeneration);
        }
        let process = supervisor.process.take();
        let endpoint = supervisor.endpoint.take();
        supervisor.state = Some(ManagedState::Stopping);
        supervisor.bootstrap_url = None;
        supervisor.recovery = None;
        supervisor.evidence = vec![evidence(
            "MANAGED_RESTARTING",
            "info",
            "The exact current generation is stopping before a new generation starts.",
        )];
        (process, endpoint, supervisor.retained_spec.clone())
    };

    let spec = match retained_spec {
        Some(spec) => spec,
        None => build_launch_spec(environment)?,
    };

    if let Some(process) = process {
        let _ = process.stop();
        if let Some(endpoint) = endpoint
            && !endpoint_is_released(endpoint.port)
        {
            let deadline = Instant::now() + ENDPOINT_RELEASE_TIMEOUT;
            while !endpoint_is_released(endpoint.port) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(25));
            }
        }
    }

    start_with_spec(state, environment.id(), spec, START_TIMEOUT)
}

pub fn start_with_spec(
    state: &ManagedRuntimeState,
    environment_id: &str,
    spec: LaunchSpec,
    timeout: Duration,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    let expected_port = spec.expected_port;
    let (generation, receiver) = {
        let mut supervisor = lock_supervisor(state)?;
        supervisor.refresh_exit();
        if supervisor.process.is_some() {
            if supervisor.environment_id.as_deref() == Some(environment_id) {
                return supervisor.report(environment_id);
            }
            return Err(ManagedRuntimeError::Conflict);
        }

        let next_generation = supervisor
            .generation
            .checked_add(1)
            .ok_or(ManagedRuntimeError::StateUnavailable)?;
        let launched_at = unix_ms()?;
        let instance_id = format!("managed-{next_generation}-{launched_at}");
        let (process, receiver) = ManagedProcess::spawn(spec)?;

        supervisor.environment_id = Some(environment_id.to_string());
        supervisor.state = Some(ManagedState::Starting);
        supervisor.generation = next_generation;
        supervisor.instance_id = Some(instance_id);
        supervisor.endpoint = None;
        supervisor.bootstrap_url = None;
        supervisor.stop_disposition = Some(StopDisposition::NotRequested);
        supervisor.evidence = vec![evidence(
            "MANAGED_PROCESS_SPAWNED",
            "info",
            "A new Managed generation was created with a retained process-tree handle.",
        )];
        supervisor.process = Some(process);
        (next_generation, receiver)
    };

    let deadline = Instant::now() + timeout;
    let candidate = loop {
        if !generation_is_alive(state, environment_id, generation)? {
            return fail_start(
                state,
                environment_id,
                generation,
                ManagedRuntimeError::ProcessExited,
            );
        }
        if let Some(port) = expected_port {
            // Configured fixed port: probe it directly. Modern DSH builds
            // (>= 0.1.2-alpha) print no readiness marker, so marker-waiting
            // would always time out on them.
            if http_status_line_probe(loopback_address(port)) {
                break fixed_port_candidate(port);
            }
        } else if let Ok(value) = receiver.try_recv() {
            // Auto port: the owned generation must publish its exact
            // loopback endpoint marker.
            break parse_candidate(&value, None).inspect_err(|error| {
                let _ = fail_start(state, environment_id, generation, *error);
            })?;
        }
        if Instant::now() >= deadline {
            return fail_start(
                state,
                environment_id,
                generation,
                ManagedRuntimeError::ReadinessTimeout,
            );
        }
        thread::sleep(Duration::from_millis(20));
    };

    // Bounded HTTP-level confirmation for marker-derived candidates
    // (fixed-port candidates already passed the probe above).
    loop {
        if !generation_is_alive(state, environment_id, generation)? {
            return fail_start(
                state,
                environment_id,
                generation,
                ManagedRuntimeError::ProcessExited,
            );
        }
        if http_status_line_probe(loopback_address(candidate.endpoint.port)) {
            break;
        }
        if Instant::now() >= deadline {
            return fail_start(
                state,
                environment_id,
                generation,
                ManagedRuntimeError::ReadinessTimeout,
            );
        }
        thread::sleep(Duration::from_millis(40));
    }

    let mut supervisor = lock_supervisor(state)?;
    supervisor.refresh_exit();
    if supervisor.environment_id.as_deref() != Some(environment_id)
        || supervisor.generation != generation
        || supervisor.process.is_none()
    {
        return Err(ManagedRuntimeError::StaleGeneration);
    }
    supervisor.state = Some(ManagedState::Healthy);
    supervisor.endpoint = Some(candidate.endpoint.clone());
    supervisor.bootstrap_url = candidate.bootstrap_url.clone();
    supervisor.evidence = vec![if candidate.bootstrap_url.is_some() {
        evidence(
            "MANAGED_ENDPOINT_VERIFIED",
            "info",
            "The owned generation emitted an exact loopback endpoint and accepted a bounded TCP connection.",
        )
    } else {
        evidence(
            "MANAGED_ENDPOINT_TCP_VERIFIED",
            "info",
            "The owned generation answered the bounded HTTP probe on the configured loopback endpoint (no token-bearing marker).",
        )
    }];
    supervisor.report(environment_id)
}

fn fail_start(
    state: &ManagedRuntimeState,
    environment_id: &str,
    generation: u64,
    error: ManagedRuntimeError,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    let process = {
        let mut supervisor = lock_supervisor(state)?;
        if supervisor.environment_id.as_deref() != Some(environment_id)
            || supervisor.generation != generation
        {
            return Err(ManagedRuntimeError::StaleGeneration);
        }
        if supervisor.process.is_some() {
            supervisor.record_crash();
        }
        let safe_stop = supervisor
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.safe_stop);
        supervisor.state = Some(if safe_stop {
            ManagedState::SafeStop
        } else {
            ManagedState::Crashed
        });
        supervisor.endpoint = None;
        supervisor.bootstrap_url = None;
        supervisor.stop_disposition = Some(StopDisposition::NotRequested);
        supervisor.evidence = if safe_stop {
            vec![evidence(
                "MANAGED_SAFE_STOP",
                "error",
                "Recovery budget exhausted; the Managed generation entered Safe Stop without auto-restart.",
            )]
        } else {
            vec![evidence(
                "MANAGED_READINESS_FAILED",
                "error",
                "The Managed generation failed before endpoint publication and was cleaned up.",
            )]
        };
        supervisor.process.take()
    };
    if let Some(process) = process {
        let _ = process.stop();
    }
    Err(error)
}

/// Bounded HTTP-level readiness probe (FM-1): beyond a bare TCP connect,
/// the candidate must answer a `GET /` like an HTTP server (status line
/// starts with `HTTP/1.`). This keeps a non-DSH loopback service that
/// squats the port from receiving the bootstrap token as a published
/// endpoint. DSH answers 303 (token root) or 200 (legacy root); both pass.
fn http_status_line_probe(address: SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(CONNECT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONNECT_TIMEOUT));
    if stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut status_line = [0u8; 16];
    let mut read = 0;
    while read < status_line.len() {
        match stream.read(&mut status_line[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(_) => break,
        }
    }
    status_line.starts_with(b"HTTP/1.")
}

fn generation_is_alive(
    state: &ManagedRuntimeState,
    environment_id: &str,
    generation: u64,
) -> Result<bool, ManagedRuntimeError> {
    let mut supervisor = lock_supervisor(state)?;
    supervisor.refresh_exit();
    Ok(supervisor.environment_id.as_deref() == Some(environment_id)
        && supervisor.generation == generation
        && supervisor.process.is_some())
}

/// Relative CLI entry of a deepseek-harness checkout (ADR-0020 decision 2).
const REPO_CLI_ENTRY_REL: &str = "apps/cli/src/bin.ts";
/// Relative TypeScript loader of a deepseek-harness checkout (root script
/// `dsh` = `node --import <loader> <entry> ...`).
const REPO_TS_LOADER_REL: &str = "scripts/register-tsx-esm.mjs";

fn build_launch_spec(environment: &ManagedEnvironment) -> Result<LaunchSpec, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    if !environment.is_valid() {
        return Err(ManagedRuntimeError::InvalidEnvironment);
    }
    let harness_path = PathBuf::from(environment.harness_path());
    let node_path = environment
        .node_path()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    // nodePath only makes sense for repository sources and must point at an
    // existing absolute executable.
    if environment.harness_mode() != HarnessMode::Repository && node_path.is_some() {
        return Err(ManagedRuntimeError::NodeOverrideUnsupported);
    }
    if let Some(path) = &node_path
        && (!path.is_absolute() || !path.is_file())
    {
        return Err(ManagedRuntimeError::NodeOverrideUnsupported);
    }

    let port = environment.managed_expected_port();
    let port_argument = port.unwrap_or(0).to_string();

    if environment.harness_mode() == HarnessMode::Repository {
        // D5 / ADR-0020: a repository source is a source-checkout DIRECTORY.
        // The recipe runs the TypeScript CLI entry through the repo TS
        // loader, with the repository root (or the configured cwd) as the
        // working directory:
        //   node --import <repo>/scripts/register-tsx-esm.mjs \
        //        <repo>/apps/cli/src/bin.ts web --host ... --port N --no-open
        let repo = harness_path;
        let entry = repo.join(REPO_CLI_ENTRY_REL);
        let loader = repo.join(REPO_TS_LOADER_REL);
        if !repo.is_dir() || !entry.is_file() || !loader.is_file() {
            return Err(ManagedRuntimeError::UnsupportedSource);
        }
        let executable = resolve_node(node_path)?;
        // Windows node rejects a bare absolute path for --import (the path
        // is treated as a URL and fails with ERR_UNSUPPORTED_ESM_URL_SCHEME
        // "protocol d:"); the official recipe uses a relative specifier from
        // the repo cwd. A file:// URL is cwd-independent and works on every
        // platform, so the loader is always passed that way.
        let loader_url = Url::from_file_path(&loader)
            .map_err(|_| ManagedRuntimeError::UnsupportedSource)?;
        let mut args = vec![
            OsString::from("--import"),
            OsString::from(loader_url.as_str()),
            entry.into_os_string(),
        ];
        push_dsh_arguments(&mut args, environment, &port_argument);
        let cwd = environment
            .harness_cwd()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| repo.clone());
        return Ok(LaunchSpec {
            executable,
            args,
            cwd: Some(cwd),
            environment: dsh_home_environment(environment),
            expected_port: port,
        });
    }

    // Executable / command sources run the harness path or command directly.
    let mut args = Vec::new();
    push_dsh_arguments(&mut args, environment, &port_argument);
    Ok(LaunchSpec {
        executable: harness_path.into_os_string(),
        args,
        cwd: environment.harness_cwd().map(PathBuf::from),
        environment: dsh_home_environment(environment),
        expected_port: port,
    })
}

/// Appends the Supervisor-owned dsh web invocation arguments
/// (profile/web/loopback/port/no-open) after any source-specific prefix.
fn push_dsh_arguments(
    args: &mut Vec<OsString>,
    environment: &ManagedEnvironment,
    port_argument: &str,
) {
    // DSH CLI shape (verified 2026-09-02): `dsh web` is an alias of
    // `dsh --profile web`; other profiles boot with `dsh --profile <name> <app flags>`
    // (the legacy `--profile <name> web` form is rejected with "unknown
    // option --profile"). The legacy "default" profile maps to the web command.
    let profile = environment.profile();
    if profile == "default" {
        args.push(OsString::from("web"));
    } else {
        args.push(OsString::from("--profile"));
        args.push(OsString::from(profile));
    }
    args.push(OsString::from("--host"));
    args.push(OsString::from("127.0.0.1"));
    args.push(OsString::from("--port"));
    args.push(OsString::from(port_argument));
    args.push(OsString::from("--no-open"));
    args.extend(environment.harness_args().iter().map(OsString::from));
}

fn dsh_home_environment(environment: &ManagedEnvironment) -> Vec<(OsString, OsString)> {
    vec![(
        OsString::from("DSH_HOME"),
        OsString::from(environment.dsh_home()),
    )]
}

/// Node executable for a repository recipe: the configured nodePath wins;
/// otherwise PATH is probed on Windows (spawn does not search PATH there),
/// while Unix spawns the bare `node` name and lets the OS resolve it.
fn resolve_node(node_path: Option<PathBuf>) -> Result<OsString, ManagedRuntimeError> {
    if let Some(path) = node_path {
        return Ok(path.into_os_string());
    }
    // Probe PATH on every platform so a missing node reports the same clear
    // error instead of an opaque spawn failure (Unix spawn would otherwise
    // resolve "node" lazily through PATH).
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    find_on_path(name)
        .map(PathBuf::into_os_string)
        .ok_or(ManagedRuntimeError::NodeOverrideUnsupported)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// A candidate for a configured fixed port: modern DSH builds print no
/// readiness marker, so the supervisor probes the configured loopback
/// endpoint directly. No token-bearing URL exists on this path, which keeps
/// the authenticated Surface binding unavailable (fail-closed).
fn fixed_port_candidate(port: u16) -> ParsedCandidate {
    ParsedCandidate {
        endpoint: ManagedEndpoint {
            scheme: "http",
            host: "127.0.0.1",
            port,
            source: "managed_config",
            verification: "owned_generation_tcp",
        },
        bootstrap_url: None,
    }
}

fn loopback_address(port: u16) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
}

fn parse_candidate(
    candidate: &str,
    expected_port: Option<u16>,
) -> Result<ParsedCandidate, ManagedRuntimeError> {
    if candidate.is_empty() || candidate.len() > MAX_OUTPUT_LINE_BYTES {
        return Err(ManagedRuntimeError::CandidateInvalid);
    }
    let url = Url::parse(candidate).map_err(|_| ManagedRuntimeError::CandidateInvalid)?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.fragment().is_some()
    {
        return Err(ManagedRuntimeError::CandidateInvalid);
    }
    if let Some(query) = url.query() {
        let token = query
            .strip_prefix("token=")
            .ok_or(ManagedRuntimeError::CandidateInvalid)?;
        if token.len() != 43
            || !token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err(ManagedRuntimeError::CandidateInvalid);
        }
    }
    let port = url.port().ok_or(ManagedRuntimeError::CandidateInvalid)?;
    if port < 1024 || raw_authority(candidate) != Some(format!("127.0.0.1:{port}").as_str()) {
        return Err(ManagedRuntimeError::CandidateInvalid);
    }
    if expected_port.is_some_and(|expected| expected != port) {
        return Err(ManagedRuntimeError::CandidatePortMismatch);
    }
    Ok(ParsedCandidate {
        endpoint: ManagedEndpoint {
            scheme: "http",
            host: "127.0.0.1",
            port,
            source: "managed_process_output",
            verification: "owned_generation_output_and_tcp",
        },
        bootstrap_url: Some(url),
    })
}

fn raw_authority(candidate_url: &str) -> Option<&str> {
    let (_, remainder) = candidate_url.split_once("://")?;
    let end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    Some(&remainder[..end])
}

fn endpoint_is_released(port: u16) -> bool {
    let deadline = Instant::now() + ENDPOINT_RELEASE_TIMEOUT;
    let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    loop {
        if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn lock_supervisor(
    state: &ManagedRuntimeState,
) -> Result<MutexGuard<'_, Supervisor>, ManagedRuntimeError> {
    state
        .inner
        .lock()
        .map_err(|_| ManagedRuntimeError::StateUnavailable)
}

fn unix_ms() -> Result<u64, ManagedRuntimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ManagedRuntimeError::ClockUnavailable)?
        .as_millis()
        .try_into()
        .map_err(|_| ManagedRuntimeError::ClockUnavailable)
}

fn evidence(code: &'static str, severity: &'static str, message: &'static str) -> RuntimeEvidence {
    RuntimeEvidence {
        code,
        severity,
        message,
    }
}

impl Supervisor {
    fn refresh_exit(&mut self) {
        let exited = self
            .process
            .as_mut()
            .is_some_and(|process| process.try_wait().map_or(true, |status| status.is_some()));
        if exited {
            self.record_crash();
            let safe_stop = self
                .recovery
                .as_ref()
                .is_some_and(|recovery| recovery.safe_stop);
            self.process.take();
            self.state = Some(if safe_stop {
                ManagedState::SafeStop
            } else {
                ManagedState::Crashed
            });
            self.endpoint = None;
            self.bootstrap_url = None;
            self.stop_disposition = Some(StopDisposition::NotRequested);
            self.evidence = if safe_stop {
                vec![evidence(
                    "MANAGED_SAFE_STOP",
                    "error",
                    "Recovery budget exhausted; the Managed generation entered Safe Stop without auto-restart.",
                )]
            } else {
                vec![evidence(
                    "MANAGED_PROCESS_EXITED",
                    "error",
                    "The retained Managed process exited before an explicit stop completed.",
                )]
            };
        }
    }

    // Recovery accounting is window-bounded: crashes older than RECOVERY_WINDOW
    // slide the window, and reaching the budget latches Safe Stop so no
    // auto-restart can loop forever (AC-REC-001).
    fn record_crash(&mut self) {
        let now = unix_ms().unwrap_or(0);
        let recovery = self.recovery.get_or_insert(RecoveryState {
            crash_count: 0,
            window_start_unix_ms: now,
            last_crash_at_unix_ms: None,
            safe_stop: false,
        });
        let window_ms = RECOVERY_WINDOW.as_millis() as u64;
        if now.saturating_sub(recovery.window_start_unix_ms) > window_ms {
            recovery.crash_count = 0;
            recovery.window_start_unix_ms = now;
            recovery.safe_stop = false;
        }
        recovery.crash_count += 1;
        recovery.last_crash_at_unix_ms = Some(now);
        if recovery.crash_count >= RECOVERY_BUDGET {
            recovery.safe_stop = true;
        }
    }

    fn auto_restart_eligible(&self) -> bool {
        self.state == Some(ManagedState::Crashed)
            && self.auto_restart_on_crash
            && self
                .recovery
                .as_ref()
                .is_some_and(|recovery| !recovery.safe_stop)
            && self.retained_spec.is_some()
    }

    fn report(&self, environment_id: &str) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
        let state = self.state.unwrap_or(ManagedState::Stopped);
        let process_ownership = if self.process.is_some() {
            ProcessOwnership::Owned
        } else {
            ProcessOwnership::None
        };
        let readiness = match state {
            ManagedState::Stopped => Readiness::NotStarted,
            ManagedState::Starting | ManagedState::Stopping => Readiness::Waiting,
            ManagedState::Healthy => Readiness::Verified,
            ManagedState::Crashed | ManagedState::SafeStop => Readiness::Failed,
        };
        let default_evidence = match state {
            ManagedState::Stopped => evidence(
                "MANAGED_STOPPED",
                "info",
                "No Managed process tree is currently retained.",
            ),
            ManagedState::Starting => evidence(
                "MANAGED_STARTING",
                "info",
                "The current Managed generation is waiting for verified readiness.",
            ),
            ManagedState::Healthy => evidence(
                "MANAGED_ENDPOINT_VERIFIED",
                "info",
                "The current Managed endpoint passed publication gates.",
            ),
            ManagedState::Stopping => evidence(
                "MANAGED_STOPPING",
                "info",
                "The retained Managed process tree is stopping.",
            ),
            ManagedState::Crashed => evidence(
                "MANAGED_PROCESS_EXITED",
                "error",
                "The current Managed generation is not running.",
            ),
            ManagedState::SafeStop => evidence(
                "MANAGED_SAFE_STOP",
                "error",
                "Recovery budget exhausted; the Managed generation entered Safe Stop.",
            ),
        };
        Ok(ManagedRuntimeReport {
            schema_version: SCHEMA_VERSION,
            environment_id: environment_id.to_string(),
            ownership: "managed",
            state,
            generation: self.generation,
            instance_id: self
                .instance_id
                .clone()
                .filter(|_| !matches!(state, ManagedState::Stopped | ManagedState::SafeStop)),
            process_ownership,
            lifecycle_mutation: "allowed",
            readiness,
            endpoint: self.endpoint,
            stop_disposition: self
                .stop_disposition
                .unwrap_or(StopDisposition::NotRequested),
            recovery: self.recovery.map(|recovery| RecoveryReport {
                crash_count: recovery.crash_count,
                window_start_unix_ms: recovery.window_start_unix_ms,
                budget: RECOVERY_BUDGET,
                safe_stop: recovery.safe_stop,
                last_crash_at_unix_ms: recovery.last_crash_at_unix_ms,
            }),
            observed_at_unix_ms: unix_ms()?,
            evidence: if self.evidence.is_empty() {
                vec![default_evidence]
            } else {
                self.evidence.clone()
            },
        })
    }
}

impl ManagedProcess {
    fn spawn(spec: LaunchSpec) -> Result<(Self, Receiver<String>), ManagedRuntimeError> {
        let mut command = Command::new(&spec.executable);
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &spec.environment {
            command.env(key, value);
        }
        configure_process_group(&mut command);

        let mut child = command
            .spawn()
            .map_err(|_| ManagedRuntimeError::SpawnUnavailable)?;
        let tree = match ProcessTree::attach(&child) {
            Ok(tree) => tree,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ManagedRuntimeError::ProcessTreeUnavailable);
            }
        };

        let (sender, receiver) = mpsc::sync_channel(4);
        if let Some(stdout) = child.stdout.take() {
            spawn_output_reader(stdout, sender.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_output_reader(stderr, sender);
        }
        Ok((Self { child, tree }, receiver))
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    fn stop(mut self) -> io::Result<StopDisposition> {
        if self.child.try_wait()?.is_some() {
            return Ok(StopDisposition::Graceful);
        }
        let disposition = self.tree.stop(&mut self.child)?;
        let _ = self.child.wait();
        Ok(disposition)
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.tree.force_stop(&mut self.child);
            let _ = self.child.wait();
        }
    }
}

fn spawn_output_reader<R>(mut reader: R, sender: SyncSender<String>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut chunk = [0_u8; 512];
        let mut line = Vec::with_capacity(256);
        let mut overflow = false;
        loop {
            let read = match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            for byte in &chunk[..read] {
                if *byte == b'\n' {
                    if !overflow {
                        maybe_send_candidate(&line, &sender);
                    }
                    line.clear();
                    overflow = false;
                } else if *byte != b'\r' {
                    if line.len() < MAX_OUTPUT_LINE_BYTES {
                        line.push(*byte);
                    } else {
                        overflow = true;
                    }
                }
            }
        }
        if !line.is_empty() && !overflow {
            maybe_send_candidate(&line, &sender);
        }
    });
}

fn maybe_send_candidate(line: &[u8], sender: &SyncSender<String>) {
    let Ok(line) = std::str::from_utf8(line) else {
        return;
    };
    let Some(candidate) = line.trim().strip_prefix(OUTPUT_MARKER) else {
        return;
    };
    let _ = sender.try_send(candidate.trim().to_string());
}

#[cfg(windows)]
fn configure_process_group(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
    };
    // CREATE_NEW_PROCESS_GROUP keeps the child in its own process group;
    // CREATE_NO_WINDOW stops the console subsystem child (node) from popping
    // up its own console window next to the Shell.
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
struct ProcessTree {
    job: windows_sys::Win32::Foundation::HANDLE,
}

// SAFETY: `job` is an owned kernel handle. All access is mediated by the
// Supervisor mutex, and ProcessTree closes the handle exactly once in Drop.
#[cfg(windows)]
unsafe impl Send for ProcessTree {}

#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &Child) -> io::Result<Self> {
        use std::mem::{size_of, zeroed};
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const information).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        let assigned = configured != 0
            && unsafe { AssignProcessToJobObject(job, child.as_raw_handle().cast()) } != 0;
        if !assigned {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(io::Error::last_os_error());
        }
        Ok(Self { job })
    }

    fn stop(&self, child: &mut Child) -> io::Result<StopDisposition> {
        self.force_stop(child)?;
        wait_for_exit(child, STOP_TIMEOUT)?;
        Ok(StopDisposition::Forced)
    }

    fn force_stop(&self, _child: &mut Child) -> io::Result<()> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        if unsafe { TerminateJobObject(self.job, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.job) };
    }
}

#[cfg(unix)]
struct ProcessTree {
    process_group: i32,
}

#[cfg(unix)]
impl ProcessTree {
    fn attach(child: &Child) -> io::Result<Self> {
        let process_group = i32::try_from(child.id())
            .map_err(|_| io::Error::other("child id exceeds process-group range"))?;
        Ok(Self { process_group })
    }

    fn stop(&self, child: &mut Child) -> io::Result<StopDisposition> {
        if self.signal(libc::SIGTERM).is_ok() && wait_for_exit(child, STOP_TIMEOUT).is_ok() {
            return Ok(StopDisposition::Graceful);
        }
        self.force_stop(child)?;
        wait_for_exit(child, STOP_TIMEOUT)?;
        Ok(StopDisposition::Forced)
    }

    fn force_stop(&self, _child: &mut Child) -> io::Result<()> {
        self.signal(libc::SIGKILL)
    }

    fn signal(&self, signal: i32) -> io::Result<()> {
        if unsafe { libc::kill(-self.process_group, signal) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process stop timed out",
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    use super::*;

    pub(crate) fn environment(
        ownership: &str,
        mode: &str,
        port: serde_json::Value,
    ) -> ManagedEnvironment {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed DSH",
            "harness": { "mode": mode, "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": port },
            "ownership": ownership
        }))
        .expect("environment fixture")
    }

    fn other_environment() -> ManagedEnvironment {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-other",
            "label": "Other Managed DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "managed"
        }))
        .expect("environment fixture")
    }

    fn configure(state: &ManagedRuntimeState, spec: LaunchSpec, auto_restart: bool) {
        let mut supervisor = lock_supervisor(state).expect("supervisor lock");
        supervisor.retained_spec = Some(spec);
        supervisor.auto_restart_on_crash = auto_restart;
        supervisor.recovery = None;
    }

    #[test]
    fn launch_spec_forces_structured_loopback_no_open_arguments() {
        let spec = build_launch_spec(&environment(
            "managed",
            "executable",
            serde_json::json!("auto"),
        ))
        .expect("launch spec");
        let args: Vec<_> = spec
            .args
            .iter()
            .map(|value| value.to_string_lossy())
            .collect();
        assert_eq!(
            args,
            ["web", "--host", "127.0.0.1", "--port", "0", "--no-open"]
        );
        assert_eq!(spec.expected_port, None);
    }

    struct TestRepo(PathBuf);

    impl TestRepo {
        fn new() -> Self {
            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "dsh-managed-repo-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(path.join("apps/cli/src")).expect("repo entry dirs");
            fs::create_dir_all(path.join("scripts")).expect("repo scripts dir");
            fs::write(path.join("apps/cli/src/bin.ts"), b"console.log('dsh')
")
                .expect("entry stub");
            fs::write(path.join("scripts/register-tsx-esm.mjs"), b"export {};
")
                .expect("loader stub");
            Self(path)
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn repository_directory_recipe_runs_entry_through_ts_loader() {
        let repo = TestRepo::new();
        let node = std::env::current_exe().expect("test executable");
        let repo_root = repo.0.to_string_lossy().into_owned();
        let value: ManagedEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed source DSH",
            "harness": {
                "mode": "repository",
                "path": repo_root,
                "cwd": repo_root
            },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "work",
            "nodePath": node,
            "endpoint": { "host": "127.0.0.1", "port": 4317 },
            "ownership": "managed"
        }))
        .expect("repository environment");
        let spec = build_launch_spec(&value).expect("repository launch spec");
        assert_eq!(PathBuf::from(&spec.executable), node);
        assert_eq!(spec.cwd.as_deref(), Some(repo.0.as_path()));
        let loader = repo.0.join("scripts/register-tsx-esm.mjs");
        let entry = repo.0.join("apps/cli/src/bin.ts");
        let loader_url = Url::from_file_path(&loader).expect("loader file url");
        let displays: Vec<_> = spec
            .args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            displays,
            vec![
                "--import".to_string(),
                loader_url.to_string(),
                entry.to_string_lossy().into_owned(),
                "--profile".to_string(),
                "work".to_string(),
                "--host".to_string(),
                "127.0.0.1".to_string(),
                "--port".to_string(),
                "4317".to_string(),
                "--no-open".to_string()
            ]
        );
    }

    #[test]
    fn repository_directory_without_entry_or_loader_is_rejected() {
        let repo = TestRepo::new();
        fs::remove_file(repo.0.join("apps/cli/src/bin.ts")).expect("remove entry");
        let node = std::env::current_exe().expect("test executable");
        let value: ManagedEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed source DSH",
            "harness": {
                "mode": "repository",
                "path": repo.0,
                "cwd": repo.0
            },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "nodePath": node,
            "endpoint": { "host": "127.0.0.1", "port": 4317 },
            "ownership": "managed"
        }))
        .expect("repository environment");
        assert_eq!(
            build_launch_spec(&value).expect_err("missing entry reject"),
            ManagedRuntimeError::UnsupportedSource
        );
    }

    #[test]
    fn repository_node_recipe_rejects_missing_node_executable() {
        let executable = std::env::current_exe().expect("test executable");
        let value: ManagedEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed source DSH",
            "harness": {
                "mode": "repository",
                "path": executable,
                "cwd": executable.parent().expect("executable parent")
            },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "nodePath": if cfg!(windows) {
                "C:/does-not-exist/node.exe"
            } else {
                "/does-not-exist/node"
            },
            "endpoint": { "host": "127.0.0.1", "port": 4317 },
            "ownership": "managed"
        }))
        .expect("repository environment");
        assert_eq!(
            build_launch_spec(&value).expect_err("missing Node reject"),
            ManagedRuntimeError::NodeOverrideUnsupported
        );
    }

    #[test]
    fn attached_and_unprepared_repository_sources_are_rejected() {
        assert_eq!(
            build_launch_spec(&environment(
                "attached",
                "executable",
                serde_json::json!(4317)
            ))
            .expect_err("attached reject"),
            ManagedRuntimeError::NotManaged
        );
        assert_eq!(
            build_launch_spec(&environment(
                "managed",
                "repository",
                serde_json::json!(4317)
            ))
            .expect_err("repository reject"),
            ManagedRuntimeError::UnsupportedSource
        );
    }

    #[test]
    fn candidate_parser_requires_exact_root_loopback_and_expected_port() {
        assert_eq!(
            parse_candidate("http://127.0.0.1:4317", Some(4317))
                .expect("candidate")
                .endpoint
                .port,
            4317
        );
        let token = "A234567890123456789012345678901234567890123";
        let authenticated =
            parse_candidate(&format!("http://127.0.0.1:4317/?token={token}"), Some(4317))
                .expect("authenticated candidate");
        assert_eq!(authenticated.endpoint.port, 4317);
        assert_eq!(
            authenticated.bootstrap_url.expect("marker bootstrap url").as_str(),
            format!("http://127.0.0.1:4317/?token={token}")
        );
        for invalid in [
            "http://localhost:4317/",
            "http://127.0.0.1:4318/",
            "http://user@127.0.0.1:4317/",
            "https://127.0.0.1:4317/",
            "http://127.0.0.1:4317/path",
            "http://127.1:4317/",
            "http://127.0.0.1:4317/?token=short",
            "http://127.0.0.1:4317/?token=A234567890123456789012345678901234567890123&token=again",
            "http://127.0.0.1:4317/?token=A234567890123456789012345678901234567890123&debug=1",
            "http://127.0.0.1:4317/?debug=A234567890123456789012345678901234567890123",
            "http://127.0.0.1:4317/?token=A234567890123456789012345678901234567890123#fragment",
        ] {
            assert!(parse_candidate(invalid, Some(4317)).is_err(), "{invalid}");
        }
    }

    #[test]
    fn authenticated_candidate_is_absent_from_public_runtime_report() {
        let token = "A234567890123456789012345678901234567890123";
        let candidate =
            parse_candidate(&format!("http://127.0.0.1:4317/?token={token}"), Some(4317))
                .expect("authenticated candidate");
        let supervisor = Supervisor {
            environment_id: Some("managed-local".into()),
            state: Some(ManagedState::Healthy),
            generation: 7,
            instance_id: Some("managed-7-1787792400000".into()),
            endpoint: Some(candidate.endpoint),
            bootstrap_url: candidate.bootstrap_url.clone(),
            stop_disposition: Some(StopDisposition::NotRequested),
            evidence: Vec::new(),
            process: None,
            ..Default::default()
        };
        let serialized =
            serde_json::to_string(&supervisor.report("managed-local").expect("runtime report"))
                .expect("serialize report");
        assert!(!serialized.contains(token));
        assert!(!serialized.contains("token="));
        assert!(!serialized.contains("?"));
    }

    #[test]
    fn authenticated_binding_carries_private_bootstrap_url_and_public_report_redacts() {
        let state = ManagedRuntimeState::default();
        let report = start_with_spec(
            &state,
            "managed-local",
            fake_spec("authenticated-server"),
            Duration::from_secs(8),
        )
        .expect("managed start");
        assert_eq!(report.state, ManagedState::Healthy);
        let token = "A234567890123456789012345678901234567890123";
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let binding = verified_surface_binding(&state, &managed, report.generation)
            .expect("verified authenticated binding");
        let expected_url = format!("http://127.0.0.1:{}/?token={token}", binding.port());
        assert_eq!(binding.url().as_str(), expected_url);
        let serialized = serde_json::to_string(&report).expect("serialize report");
        assert!(!serialized.contains(token));
        assert!(!serialized.contains("token="));
        assert!(!serialized.contains("?"));
        stop_managed_environment(&state, &managed, report.generation).expect("stop");
    }

    #[test]
    fn owned_tree_reaches_ready_rejects_stale_stop_and_releases_endpoint() {
        let state = ManagedRuntimeState::default();
        let report = start_with_spec(
            &state,
            "managed-local",
            fake_spec("tree-parent"),
            Duration::from_secs(8),
        )
        .expect("managed start");
        assert_eq!(report.state, ManagedState::Healthy);
        assert_eq!(report.process_ownership, ProcessOwnership::Owned);
        assert_eq!(report.readiness, Readiness::Verified);
        let endpoint = report.endpoint.expect("published endpoint");

        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let binding = verified_surface_binding(&state, &managed, report.generation)
            .expect("verified Surface binding");
        assert_eq!(binding.generation(), report.generation);
        assert_eq!(binding.port(), endpoint.port);
        assert_eq!(
            binding.url().as_str(),
            format!("http://127.0.0.1:{}/", endpoint.port)
        );
        assert_eq!(
            verified_surface_binding(&state, &managed, report.generation + 1)
                .expect_err("stale Surface binding"),
            ManagedRuntimeError::StaleGeneration
        );
        assert_eq!(
            stop_managed_environment(&state, &managed, report.generation + 1)
                .expect_err("stale stop"),
            ManagedRuntimeError::StaleGeneration
        );
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port)).is_ok());

        let stopped =
            stop_managed_environment(&state, &managed, report.generation).expect("owned stop");
        assert_eq!(stopped.state, ManagedState::Stopped);
        assert_eq!(stopped.process_ownership, ProcessOwnership::None);
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port)).is_err());
        assert_eq!(
            verified_surface_binding(&state, &managed, report.generation)
                .expect_err("stopped Surface binding"),
            ManagedRuntimeError::SurfaceBindingUnavailable
        );

        let second = start_with_spec(
            &state,
            "managed-local",
            fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("second start");
        assert_eq!(second.generation, report.generation + 1);
        stop_managed_environment(&state, &managed, second.generation).expect("second stop");
    }

    #[test]
    fn invalid_owned_output_fails_closed_and_cleans_process() {
        let state = ManagedRuntimeState::default();
        let result = start_with_spec(
            &state,
            "managed-local",
            fake_spec("invalid-candidate"),
            Duration::from_secs(8),
        );
        assert_eq!(
            result.expect_err("invalid candidate"),
            ManagedRuntimeError::CandidateInvalid
        );
        let report = lock_supervisor(&state)
            .expect("state")
            .report("managed-local")
            .expect("report");
        assert_eq!(report.state, ManagedState::Crashed);
        assert_eq!(report.process_ownership, ProcessOwnership::None);
        assert!(report.endpoint.is_none());
    }

    pub(crate) fn fake_spec(mode: &str) -> LaunchSpec {
        LaunchSpec {
            executable: std::env::current_exe()
                .expect("test executable")
                .into_os_string(),
            args: [
                "--exact",
                "supervisor::tests::fake_managed_child",
                "--nocapture",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: None,
            environment: vec![(OsString::from("DSH_FAKE_MANAGED"), OsString::from(mode))],
            expected_port: None,
        }
    }

    #[test]
    fn fake_managed_child() {
        let Ok(mode) = std::env::var("DSH_FAKE_MANAGED") else {
            return;
        };
        match mode.as_str() {
            "server" => fake_server(),
            "authenticated-server" => fake_authenticated_server(),
            "invalid-candidate" => {
                println!("dsh web: http://localhost:4317/");
                io::stdout().flush().expect("flush invalid candidate");
                loop {
                    thread::sleep(Duration::from_secs(1));
                }
            }
            "crash-loop" => {
                std::process::exit(1);
            }
            "tcp-only" => {
                fake_tcp_only_server();
            }
            "crash-after-ready" => {
                fake_server_with_exit(Duration::from_millis(800));
            }
            "tree-parent" => {
                thread::sleep(Duration::from_millis(250));
                let mut child = Command::new(std::env::current_exe().expect("test executable"))
                    .args([
                        "--exact",
                        "supervisor::tests::fake_managed_child",
                        "--nocapture",
                    ])
                    .env("DSH_FAKE_MANAGED", "server")
                    .stdout(Stdio::piped())
                    .spawn()
                    .expect("spawn tree child");
                let stdout = child.stdout.take().expect("tree child stdout");
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if line.contains(OUTPUT_MARKER) {
                        println!("{line}");
                        io::stdout().flush().expect("flush relayed candidate");
                        break;
                    }
                }
                let _ = child.wait();
            }
            _ => panic!("unexpected fake mode"),
        }
    }

    fn fake_server() -> ! {
        fake_server_with_exit(Duration::from_secs(3600));
    }

    fn fake_server_with_exit(exit_after: Duration) -> ! {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind fake server");
        listener
            .set_nonblocking(true)
            .expect("non-blocking fake server");
        let port = listener.local_addr().expect("fake address").port();
        println!("dsh web: http://127.0.0.1:{port}/");
        io::stdout().flush().expect("flush candidate");
        let deadline = Instant::now() + exit_after;
        while Instant::now() < deadline {
            if let Ok((mut stream, _)) = listener.accept() {
                // Answer the HTTP-level readiness probe (FM-1) like a
                // minimal HTTP server so the publication gate passes.
                let mut buf = [0u8; 256];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
            thread::sleep(Duration::from_millis(25));
        }
        std::process::exit(1);
    }

    /// Accepts TCP but never answers HTTP: must fail the readiness gate.
    fn fake_tcp_only_server() -> ! {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind fake server");
        listener
            .set_nonblocking(true)
            .expect("non-blocking fake server");
        let port = listener.local_addr().expect("fake address").port();
        println!("dsh web: http://127.0.0.1:{port}/");
        io::stdout().flush().expect("flush candidate");
        loop {
            if listener.accept().is_ok() {
                // Accept and never respond: TCP reachability alone must not
                // pass the HTTP-level readiness gate.
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn fake_authenticated_server() -> ! {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind fake server");
        listener
            .set_nonblocking(true)
            .expect("non-blocking fake server");
        let port = listener.local_addr().expect("fake address").port();
        let token = "A234567890123456789012345678901234567890123";
        println!("dsh web: http://127.0.0.1:{port}/?token={token}");
        io::stdout().flush().expect("flush candidate");
        loop {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 256];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    b"HTTP/1.1 303 See Other\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn crash_without_auto_restart_stays_crashed_with_recovery_history() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("crash-loop"), false);
        let start = start_with_spec(
            &state,
            "managed-local",
            fake_spec("crash-loop"),
            Duration::from_secs(8),
        );
        assert_eq!(
            start.expect_err("crash"),
            ManagedRuntimeError::ProcessExited
        );
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let report = get_managed_runtime_status(&state, &managed).expect("crashed status report");
        assert_eq!(report.state, ManagedState::Crashed);
        assert_eq!(report.process_ownership, ProcessOwnership::None);
        let recovery = report.recovery.expect("recovery history");
        assert_eq!(recovery.crash_count, 1);
        assert!(!recovery.safe_stop);
        assert_eq!(recovery.budget, RECOVERY_BUDGET);
    }

    #[test]
    fn crash_loop_auto_restart_exhausts_budget_into_safe_stop() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("crash-loop"), true);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let first = start_with_spec(
            &state,
            "managed-local",
            fake_spec("crash-loop"),
            Duration::from_secs(8),
        );
        assert_eq!(
            first.expect_err("first crash"),
            ManagedRuntimeError::ProcessExited
        );

        // Each status poll attempts a bounded auto-restart; crashes 2 and 3 exhaust the budget.
        for _ in 0..RECOVERY_BUDGET {
            let _ = get_managed_runtime_status(&state, &managed);
        }
        let report = get_managed_runtime_status(&state, &managed).expect("safe stop report");
        assert_eq!(report.state, ManagedState::SafeStop);
        assert_eq!(report.generation, RECOVERY_BUDGET as u64);
        assert_eq!(report.process_ownership, ProcessOwnership::None);
        assert!(report.endpoint.is_none());
        assert!(report.instance_id.is_none());
        let recovery = report.recovery.expect("exhausted recovery");
        assert_eq!(recovery.crash_count, RECOVERY_BUDGET);
        assert!(recovery.safe_stop);
        assert!(
            report
                .evidence
                .iter()
                .any(|e| e.code == "MANAGED_SAFE_STOP")
        );

        // A further status poll must not auto-restart from Safe Stop.
        let again = get_managed_runtime_status(&state, &managed).expect("still safe stop");
        assert_eq!(again.state, ManagedState::SafeStop);
        assert_eq!(again.generation, RECOVERY_BUDGET as u64);

        // An explicit start with a healthy recipe resets recovery and starts a new generation.
        configure(&state, fake_spec("server"), false);
        let healthy = start_with_spec(
            &state,
            "managed-local",
            fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("recovered start");
        assert_eq!(healthy.state, ManagedState::Healthy);
        assert!(healthy.recovery.is_none());
        stop_managed_environment(&state, &managed, healthy.generation).expect("stop");
    }

    #[test]
    fn crash_after_ready_auto_restarts_new_generation_and_binding() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("crash-after-ready"), true);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let first = start_with_spec(
            &state,
            "managed-local",
            fake_spec("crash-after-ready"),
            Duration::from_secs(8),
        )
        .expect("first ready");
        assert_eq!(first.state, ManagedState::Healthy);
        let first_generation = first.generation;
        assert_eq!(first.recovery, None);

        thread::sleep(Duration::from_millis(1300));
        let recovered = get_managed_runtime_status(&state, &managed).expect("auto restart");
        assert_eq!(recovered.state, ManagedState::Healthy);
        assert!(recovered.generation > first_generation);
        let recovery = recovered.recovery.expect("recovery after crash");
        assert_eq!(recovery.crash_count, 1);
        assert!(!recovery.safe_stop);

        assert_eq!(
            verified_surface_binding(&state, &managed, first_generation)
                .expect_err("old generation stale"),
            ManagedRuntimeError::StaleGeneration
        );
        let binding =
            verified_surface_binding(&state, &managed, recovered.generation).expect("new binding");
        assert_eq!(binding.port(), recovered.endpoint.expect("endpoint").port);
        stop_managed_environment(&state, &managed, recovered.generation).expect("stop");
    }

    #[test]
    fn readiness_requires_http_response_not_just_tcp() {
        // FM-1 regression: a loopback service that accepts TCP but never
        // answers HTTP must not pass the publication gate.
        let state = ManagedRuntimeState::default();
        assert_eq!(
            start_with_spec(
                &state,
                "managed-local",
                fake_spec("tcp-only"),
                Duration::from_millis(900),
            )
            .expect_err("tcp-only reject"),
            ManagedRuntimeError::ReadinessTimeout
        );
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let report = get_managed_runtime_status(&state, &managed).expect("failed start report");
        assert_eq!(report.state, ManagedState::Crashed);
        assert!(report.endpoint.is_none());
    }

    #[test]
    fn status_for_foreign_environment_conflicts() {
        // FM-4 regression: the Supervisor owns exactly one environment;
        // querying another environment must conflict instead of mislabeling
        // state or auto-restarting the wrong recipe.
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("server"), false);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        start_with_spec(
            &state,
            "managed-local",
            fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("first ready");
        assert_eq!(
            get_managed_runtime_status(&state, &other_environment()).expect_err("foreign status"),
            ManagedRuntimeError::Conflict
        );
        assert_eq!(
            verified_surface_binding(&state, &other_environment(), 1).expect_err("foreign binding"),
            ManagedRuntimeError::Conflict
        );
        stop_managed_environment(&state, &managed, 1).expect("stop");
    }

    #[test]
    fn restart_updates_generation_and_surface_binding() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("server"), false);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let first = start_with_spec(
            &state,
            "managed-local",
            fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("first ready");
        assert_eq!(first.state, ManagedState::Healthy);
        let old_generation = first.generation;

        let request = ManagedRuntimeRestartRequest {
            schema_version: 1,
            environment_id: "managed-local".into(),
            expected_generation: old_generation,
        };
        let restarted = restart_managed_environment(&state, &managed, request).expect("restart");
        assert_eq!(restarted.state, ManagedState::Healthy);
        assert!(restarted.generation > old_generation);
        assert_ne!(restarted.instance_id, first.instance_id);
        assert!(restarted.recovery.is_none());

        assert_eq!(
            verified_surface_binding(&state, &managed, old_generation)
                .expect_err("old generation stale"),
            ManagedRuntimeError::StaleGeneration
        );
        let binding = verified_surface_binding(&state, &managed, restarted.generation)
            .expect("new generation binding");
        assert_eq!(binding.port(), restarted.endpoint.expect("endpoint").port);
        stop_managed_environment(&state, &managed, restarted.generation).expect("stop");
    }

    #[test]
    fn restart_rejects_stale_generation_and_foreign_environment() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("server"), false);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let first = start_with_spec(
            &state,
            "managed-local",
            fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("first ready");

        let stale = ManagedRuntimeRestartRequest {
            schema_version: 1,
            environment_id: "managed-local".into(),
            expected_generation: first.generation + 1,
        };
        assert_eq!(
            restart_managed_environment(&state, &managed, stale).expect_err("stale restart"),
            ManagedRuntimeError::StaleGeneration
        );
        let foreign = ManagedRuntimeRestartRequest {
            schema_version: 1,
            environment_id: "managed-other".into(),
            expected_generation: first.generation,
        };
        assert_eq!(
            restart_managed_environment(&state, &other_environment(), foreign)
                .expect_err("foreign restart"),
            ManagedRuntimeError::Conflict
        );
        let still = get_managed_runtime_status(&state, &managed).expect("still healthy");
        assert_eq!(still.state, ManagedState::Healthy);
        assert_eq!(still.generation, first.generation);
        stop_managed_environment(&state, &managed, first.generation).expect("stop");
    }

    #[test]
    fn restart_when_stopped_starts_a_new_generation() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("server"), false);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let first = start_with_spec(
            &state,
            "managed-local",
            fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("first ready");
        let stopped = stop_managed_environment(&state, &managed, first.generation).expect("stop");
        assert_eq!(stopped.state, ManagedState::Stopped);

        let request = ManagedRuntimeRestartRequest {
            schema_version: 1,
            environment_id: "managed-local".into(),
            expected_generation: first.generation,
        };
        let restarted = restart_managed_environment(&state, &managed, request).expect("restart");
        assert_eq!(restarted.state, ManagedState::Healthy);
        assert!(restarted.generation > first.generation);
        stop_managed_environment(&state, &managed, restarted.generation).expect("stop");
    }

    #[test]
    fn concurrent_same_environment_starts_are_idempotent() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("server"), false);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let state = Arc::new(state);
        let state_b = state.clone();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let barrier_b = barrier.clone();
        let handle = std::thread::spawn(move || {
            barrier_b.wait();
            start_with_spec(
                &state_b,
                "managed-local",
                fake_spec("server"),
                Duration::from_secs(8),
            )
        });
        barrier.wait();
        let first = start_with_spec(
            &state,
            "managed-local",
            fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("first start");
        let second = handle.join().expect("second thread").expect("second start");
        // One caller spawns the generation; the concurrent caller observes the same
        // generation (possibly still Starting) instead of spawning a second tree.
        assert_eq!(first.generation, second.generation);
        assert_eq!(first.instance_id, second.instance_id);
        assert!(
            matches!(first.state, ManagedState::Healthy | ManagedState::Starting)
                && matches!(second.state, ManagedState::Healthy | ManagedState::Starting)
        );
        let healthy = get_managed_runtime_status(&state, &managed).expect("settled status");
        assert_eq!(healthy.state, ManagedState::Healthy);
        assert_eq!(healthy.generation, first.generation);
        stop_managed_environment(&state, &managed, healthy.generation).expect("stop");
    }

    #[test]
    fn concurrent_foreign_environment_start_conflicts() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("server"), false);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        let first = start_with_spec(
            &state,
            "managed-local",
            fake_spec("server"),
            Duration::from_secs(8),
        )
        .expect("first ready");
        assert_eq!(
            start_with_spec(
                &state,
                "managed-other",
                fake_spec("server"),
                Duration::from_secs(8)
            )
            .expect_err("foreign conflict"),
            ManagedRuntimeError::Conflict
        );
        stop_managed_environment(&state, &managed, first.generation).expect("stop");
    }

    #[test]
    fn restart_resets_recovery_budget_after_crashes() {
        let state = ManagedRuntimeState::default();
        configure(&state, fake_spec("crash-loop"), true);
        let managed = environment("managed", "executable", serde_json::json!("auto"));
        assert_eq!(
            start_with_spec(
                &state,
                "managed-local",
                fake_spec("crash-loop"),
                Duration::from_secs(8),
            )
            .expect_err("first crash"),
            ManagedRuntimeError::ProcessExited
        );
        // One status poll triggers a bounded auto-restart that also crashes.
        let _ = get_managed_runtime_status(&state, &managed);
        let generation = lock_supervisor(&state).expect("supervisor lock").generation;
        assert_eq!(generation, 2, "auto-restart created generation two");

        // Explicit restart resets the budget and attempts a fresh generation.
        let request = ManagedRuntimeRestartRequest {
            schema_version: 1,
            environment_id: "managed-local".into(),
            expected_generation: generation,
        };
        assert_eq!(
            restart_managed_environment(&state, &managed, request)
                .expect_err("restart still crash-loops"),
            ManagedRuntimeError::ProcessExited
        );
        {
            let supervisor = lock_supervisor(&state).expect("supervisor lock");
            assert_eq!(supervisor.state, Some(ManagedState::Crashed));
            let recovery = supervisor.recovery.as_ref().expect("reset recovery");
            assert_eq!(recovery.crash_count, 1, "restart reset the recovery budget");
            assert!(!recovery.safe_stop);
        }
        // The bounded auto-restart policy continues from the reset budget: two more
        // status polls exhaust it into Safe Stop.
        for _ in 0..2 {
            let _ = get_managed_runtime_status(&state, &managed);
        }
        let final_report = get_managed_runtime_status(&state, &managed).expect("safe stop report");
        assert_eq!(final_report.state, ManagedState::SafeStop);
        assert_eq!(
            final_report.recovery.expect("final recovery").crash_count,
            RECOVERY_BUDGET
        );
    }
}
