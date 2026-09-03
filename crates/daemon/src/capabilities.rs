//! Capability surface of the daemon (ADR-0019 decision 5).
//!
//! The catalog is the daemon-side *policy*: every coordinate in
//! [`all()`] is grantable and is dispatched to a handler here. The
//! authorization decision itself is delegated to the broker chain
//! (`crates/supervisor`): `Hello` → `broker_grant_from_negotiation`,
//! `Invocation` → `Broker::enforce_dispatch` (see [`crate::server`]).
//! This replaces the static `GrantPolicy` of the M5-B2 reference loop
//! (external-api-example) with the broker-driven upgrade required by
//! ADR-0019 decision 5 / M5-E1.
//!
//! Resource-backed methods: `terminal.*` is **real** since M6-C1 (the
//! daemon owns the PTY registry, [`crate::terminal`]); `browser.*` is
//! **real** since M6-C3 (the daemon owns the browser session registry,
//! [`crate::browser`]); `runtime.*` is **real** since M6-C2 (the
//! daemon owns the Managed DSH process tree, [`crate::runtime`]);
//! `scheduler.*` (IF-SCHEDULE-WAKE, ADR-0019 decision 6) is M6-D.
//!
//! The terminal envelope methods use the namespaced form
//! `terminal.create` / `terminal.write` / `terminal.resize` /
//! `terminal.close` / `terminal.status` (the envelope method pattern
//! `^[a-z][a-z0-9._-]+$`; specs/terminal wire contract).

use std::sync::{Arc, Mutex};

use dsh_supervisor::{Broker, SystemClock};

use crate::browser::{self, BrowserHost};
use crate::envelope::{ErrorCode, ProtocolCoordinate};
use crate::events::EventRouter;
use crate::runtime::{self, ManagedRuntimeHost};
use crate::scheduler::{
    SCHEDULER_API_VERSION, SCHEDULER_CANCEL_METHOD, SCHEDULER_KIND, SCHEDULER_WAKE_METHOD,
    Scheduler, SchedulerError, SchedulerStats,
};
use crate::terminal::{self, TerminalHost};

/// `system` capability (method `ping`) — liveness probe.
pub const SYSTEM_API_VERSION: &str = "system.dsh-desktop.local/v1alpha1";
pub const SYSTEM_KIND: &str = "System";
pub const SYSTEM_PING_METHOD: &str = "ping";

/// `daemon` capability (method `status`) — daemon identity and counters.
pub const DAEMON_API_VERSION: &str = "daemon.dsh-desktop.local/v1alpha1";
pub const DAEMON_KIND: &str = "Daemon";
pub const DAEMON_STATUS_METHOD: &str = "status";

/// `browser` capability — session state authority, **real** since
/// M6-C3 (the daemon owns the browser `SessionRegistry`,
/// [`crate::browser`]). Envelope methods are the namespaced
/// `browser.create` / `browser.list` / `browser.status` /
/// `browser.close` form; the M6-B1 placeholder method `list_browsers`
/// is superseded.
pub const BROWSER_API_VERSION: &str = "browser.dsh-desktop.local/v1alpha1";
pub const BROWSER_KIND: &str = "Browser";
pub const BROWSER_CREATE_METHOD: &str = "browser.create";
pub const BROWSER_LIST_METHOD: &str = "browser.list";
pub const BROWSER_STATUS_METHOD: &str = "browser.status";
pub const BROWSER_CLOSE_METHOD: &str = "browser.close";

/// `terminal` capability — real since M6-C1 (the daemon holds the PTY
/// registry). Envelope methods are the namespaced `terminal.*` form.
pub const TERMINAL_API_VERSION: &str = "terminal.dsh-desktop.local/v1alpha1";
pub const TERMINAL_KIND: &str = "Terminal";
pub const TERMINAL_CREATE_METHOD: &str = "terminal.create";
pub const TERMINAL_WRITE_METHOD: &str = "terminal.write";
pub const TERMINAL_RESIZE_METHOD: &str = "terminal.resize";
pub const TERMINAL_CLOSE_METHOD: &str = "terminal.close";
pub const TERMINAL_STATUS_METHOD: &str = "terminal.status";

/// `runtime` capability — managed DSH runtimes; **real** since M6-C2
/// (the daemon owns the DSH process tree, [`crate::runtime`]). Envelope
/// methods are the namespaced `runtime.start` / `runtime.status` /
/// `runtime.stop` / `runtime.restart` form; payload shapes mirror
/// specs/runtime/*.schema.json.
pub const RUNTIME_API_VERSION: &str = "runtime.dsh-desktop.local/v1alpha1";
pub const RUNTIME_KIND: &str = "Runtime";
pub const RUNTIME_START_METHOD: &str = "runtime.start";
pub const RUNTIME_STATUS_METHOD: &str = "runtime.status";
pub const RUNTIME_STOP_METHOD: &str = "runtime.stop";
pub const RUNTIME_RESTART_METHOD: &str = "runtime.restart";
/// `runtime.binding` - the verified Surface binding of the exact
/// generation, carrying its private bootstrap URL (never part of the
/// public status report; see the redaction tests in managed-runtime).
pub const RUNTIME_BINDING_METHOD: &str = "runtime.binding";

/// `scheduler` capability (methods `wake`/`cancel`) — IF-SCHEDULE-WAKE
/// TimerHost (ADR-0019 decision 6, M6-D); constants live in
/// `crate::scheduler`.
///
/// Every capability the daemon implements (the grantable set).
pub fn all() -> Vec<ProtocolCoordinate> {
    vec![
        ProtocolCoordinate {
            api_version: SYSTEM_API_VERSION.into(),
            kind: SYSTEM_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: DAEMON_API_VERSION.into(),
            kind: DAEMON_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: BROWSER_API_VERSION.into(),
            kind: BROWSER_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: TERMINAL_API_VERSION.into(),
            kind: TERMINAL_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: RUNTIME_API_VERSION.into(),
            kind: RUNTIME_KIND.into(),
        },
        ProtocolCoordinate {
            api_version: SCHEDULER_API_VERSION.into(),
            kind: SCHEDULER_KIND.into(),
        },
    ]
}

/// Whether the daemon implements `coordinate`.
pub fn supports(coordinate: &ProtocolCoordinate) -> bool {
    all().contains(coordinate)
}

/// Snapshot of daemon runtime facts for `daemon.status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatusSnapshot {
    pub version: &'static str,
    pub pid: u32,
    pub started_at: String,
    pub uptime_seconds: u64,
    pub claim_port: u16,
    pub port: u16,
    pub connections: usize,
    pub credentials_issued: u64,
    pub activations: usize,
    pub scheduler: SchedulerStats,
    /// Live terminal sessions (real since M6-C1).
    pub terminals: usize,
    /// Live browser sessions (real since M6-C3).
    pub browsers: usize,
    /// Environments held by the Managed runtime supervisor (0 or 1;
    /// real since M6-C2).
    pub managed_runtimes: usize,
}

/// Everything a capability handler may need beyond the request payload,
/// built by the server per invocation ([`crate::server`]).
pub struct CapabilityContext {
    pub snapshot: DaemonStatusSnapshot,
    /// The daemon-owned PTY host (M6-C1).
    pub terminal: Arc<TerminalHost>,
    /// The daemon-owned browser session host (M6-C3).
    pub browser: Arc<BrowserHost>,
    /// The daemon-owned Managed runtime host (M6-C2: DSH process tree).
    pub runtime: Arc<ManagedRuntimeHost>,
    /// The daemon event router (event subscriptions by session id).
    pub events: Arc<EventRouter>,
    /// The shared broker (agent authorization gate).
    pub broker: Arc<Mutex<Broker<SystemClock>>>,
    /// The scheduler TimerHost (M6-D).
    pub scheduler: Arc<Scheduler>,
    /// Connection key of the invoking connection (event subscription /
    /// session ownership).
    pub connection_id: u64,
}

/// Why a dispatched method could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonMethodError {
    /// The capability is implemented but the method is not.
    MethodNotFound {
        capability: ProtocolCoordinate,
        method: String,
    },
    /// The method is implemented but the payload violates its contract
    /// (mapped to MALFORMED_MESSAGE).
    InvalidPayload {
        capability: ProtocolCoordinate,
        method: String,
        message: String,
    },
    /// The operation conflicts with current state, e.g. a duplicate
    /// wakeId (mapped to CONFLICT).
    Conflict {
        capability: ProtocolCoordinate,
        method: String,
        message: String,
    },
    /// The method is implemented but failed; carries the envelope error
    /// contract (code/message/retryable) verbatim (resource methods,
    /// M6-C1).
    MethodFailed {
        code: ErrorCode,
        message: String,
        retryable: bool,
    },
}

/// Execute one capability method (called after the broker dispatch gate
/// passed; see `server.rs`). All resource capabilities are real since
/// M6-C2/C3.
pub fn dispatch(
    context: &CapabilityContext,
    capability: &ProtocolCoordinate,
    method: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    match (
        capability.api_version.as_str(),
        capability.kind.as_str(),
        method,
    ) {
        (SYSTEM_API_VERSION, SYSTEM_KIND, SYSTEM_PING_METHOD) => Ok(serde_json::json!({
            "pong": true,
            "echo": payload.clone(),
        })),
        (DAEMON_API_VERSION, DAEMON_KIND, DAEMON_STATUS_METHOD) => Ok(serde_json::json!({
            "daemonVersion": context.snapshot.version,
            "pid": context.snapshot.pid,
            "startedAt": context.snapshot.started_at,
            "uptimeSeconds": context.snapshot.uptime_seconds,
            "claimPort": context.snapshot.claim_port,
            "port": context.snapshot.port,
            "connections": context.snapshot.connections,
            "credentialsIssued": context.snapshot.credentials_issued,
            "activations": context.snapshot.activations,
            // terminals real since M6-C1, browsers real since M6-C3,
            // managedRuntimes real since M6-C2.
            "resources": {
                "browsers": context.snapshot.browsers,
                "terminals": context.snapshot.terminals,
                "managedRuntimes": context.snapshot.managed_runtimes,
            },
            "scheduler": serde_json::to_value(&context.snapshot.scheduler)
                .expect("scheduler stats serialize"),
        })),
        (BROWSER_API_VERSION, BROWSER_KIND, method) => match method {
            BROWSER_CREATE_METHOD => browser::handle_create(context, payload),
            BROWSER_LIST_METHOD => browser::handle_list(context),
            BROWSER_STATUS_METHOD => browser::handle_status(context),
            BROWSER_CLOSE_METHOD => browser::handle_close(context, payload),
            _ => Err(DaemonMethodError::MethodNotFound {
                capability: capability.clone(),
                method: method.to_string(),
            }),
        },
        (TERMINAL_API_VERSION, TERMINAL_KIND, method) => match method {
            TERMINAL_CREATE_METHOD => terminal::handle_create(context, payload),
            TERMINAL_WRITE_METHOD => terminal::handle_write(context, payload),
            TERMINAL_RESIZE_METHOD => terminal::handle_resize(context, payload),
            TERMINAL_CLOSE_METHOD => terminal::handle_close(context, payload),
            TERMINAL_STATUS_METHOD => terminal::handle_status(context),
            _ => Err(DaemonMethodError::MethodNotFound {
                capability: capability.clone(),
                method: method.to_string(),
            }),
        },
        (RUNTIME_API_VERSION, RUNTIME_KIND, method) => match method {
            RUNTIME_START_METHOD => runtime::handle_start(context, payload),
            RUNTIME_STATUS_METHOD => runtime::handle_status(context, payload),
            RUNTIME_STOP_METHOD => runtime::handle_stop(context, payload),
            RUNTIME_RESTART_METHOD => runtime::handle_restart(context, payload),
            RUNTIME_BINDING_METHOD => runtime::handle_binding(context, payload),
            _ => Err(DaemonMethodError::MethodNotFound {
                capability: capability.clone(),
                method: method.to_string(),
            }),
        },
        (SCHEDULER_API_VERSION, SCHEDULER_KIND, SCHEDULER_WAKE_METHOD) => context
            .scheduler
            .register(payload)
            .map_err(|error| scheduler_error(capability, method, error)),
        (SCHEDULER_API_VERSION, SCHEDULER_KIND, SCHEDULER_CANCEL_METHOD) => context
            .scheduler
            .cancel(payload)
            .map_err(|error| scheduler_error(capability, method, error)),
        _ => Err(DaemonMethodError::MethodNotFound {
            capability: capability.clone(),
            method: method.to_string(),
        }),
    }
}

/// Map scheduler failures onto capability dispatch errors.
fn scheduler_error(
    capability: &ProtocolCoordinate,
    method: &str,
    error: SchedulerError,
) -> DaemonMethodError {
    match error {
        SchedulerError::InvalidPayload(message) => DaemonMethodError::InvalidPayload {
            capability: capability.clone(),
            method: method.to_string(),
            message,
        },
        SchedulerError::Conflict(message) => DaemonMethodError::Conflict {
            capability: capability.clone(),
            method: method.to_string(),
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> DaemonStatusSnapshot {
        DaemonStatusSnapshot {
            version: "0.1.0",
            pid: 4242,
            started_at: "2026-08-31T09:30:00.000Z".to_string(),
            uptime_seconds: 7,
            claim_port: 37_771,
            port: 50_001,
            connections: 1,
            credentials_issued: 2,
            activations: 1,
            scheduler: SchedulerStats::default(),
            terminals: 0,
            browsers: 0,
            managed_runtimes: 0,
        }
    }

    fn context() -> CapabilityContext {
        CapabilityContext {
            snapshot: snapshot(),
            terminal: Arc::new(TerminalHost::new()),
            browser: Arc::new(BrowserHost::new()),
            runtime: Arc::new(ManagedRuntimeHost::new(
                std::env::temp_dir().join("dsh-daemon-capabilities-nonexistent-catalog.json"),
            )),
            events: EventRouter::spawn(),
            broker: Arc::new(Mutex::new(Broker::new())),
            scheduler: Arc::new(Scheduler::new()),
            connection_id: 1,
        }
    }

    fn coordinate(api_version: &str, kind: &str) -> ProtocolCoordinate {
        ProtocolCoordinate {
            api_version: api_version.into(),
            kind: kind.into(),
        }
    }

    #[test]
    fn catalog_contains_all_coordinates() {
        let coordinates = all();
        assert_eq!(coordinates.len(), 6);
        assert!(supports(&coordinates[0]));
        assert!(supports(&coordinate(SCHEDULER_API_VERSION, SCHEDULER_KIND)));
        assert!(!supports(&coordinate(
            "unknown.dsh-desktop.local/v1alpha1",
            "Unknown"
        )));
    }

    #[test]
    fn ping_echoes_payload() {
        let payload = serde_json::json!({ "message": "hi" });
        let result = dispatch(
            &context(),
            &coordinate(SYSTEM_API_VERSION, SYSTEM_KIND),
            SYSTEM_PING_METHOD,
            &payload,
        )
        .expect("ping is implemented");
        assert_eq!(result["pong"], true);
        assert_eq!(result["echo"], payload);
    }

    #[test]
    fn daemon_status_reports_snapshot() {
        let result = dispatch(
            &context(),
            &coordinate(DAEMON_API_VERSION, DAEMON_KIND),
            DAEMON_STATUS_METHOD,
            &serde_json::json!({}),
        )
        .expect("status is implemented");
        assert_eq!(result["daemonVersion"], "0.1.0");
        assert_eq!(result["pid"], 4242);
        assert_eq!(result["resources"]["browsers"], 0);
        assert_eq!(result["resources"]["terminals"], 0);
        assert_eq!(result["resources"]["managedRuntimes"], 0);
        assert_eq!(result["scheduler"]["registered"], 0);
        assert_eq!(result["scheduler"]["pending"], 0);
    }

    #[test]
    fn daemon_status_reports_real_terminal_count() {
        let ctx = context();
        // terminals comes from the snapshot, which the server fills from
        // the live host (M6-C1).
        let mut with_sessions = ctx.snapshot;
        with_sessions.terminals = 3;
        let result = dispatch(
            &CapabilityContext {
                snapshot: with_sessions,
                ..ctx
            },
            &coordinate(DAEMON_API_VERSION, DAEMON_KIND),
            DAEMON_STATUS_METHOD,
            &serde_json::json!({}),
        )
        .expect("status is implemented");
        assert_eq!(result["resources"]["terminals"], 3);
    }

    #[test]
    fn daemon_status_reports_real_managed_runtime_count() {
        let ctx = context();
        // managed_runtimes comes from the snapshot, which the server fills
        // from the live host (M6-C2).
        let mut with_runtime = ctx.snapshot;
        with_runtime.managed_runtimes = 1;
        let result = dispatch(
            &CapabilityContext {
                snapshot: with_runtime,
                ..ctx
            },
            &coordinate(DAEMON_API_VERSION, DAEMON_KIND),
            DAEMON_STATUS_METHOD,
            &serde_json::json!({}),
        )
        .expect("status is implemented");
        assert_eq!(result["resources"]["managedRuntimes"], 1);
    }

    #[test]
    fn browser_create_list_close_dispatch_roundtrip() {
        let ctx = context();
        let coordinate = coordinate(BROWSER_API_VERSION, BROWSER_KIND);

        // Register the context connection's subscriber *before* the
        // dispatch: the first register() key is 1, which is exactly
        // ctx.connection_id, so the created event routes here.
        let subscriber = ctx.events.register();
        assert_eq!(subscriber.key(), ctx.connection_id);

        let report = dispatch(
            &ctx,
            &coordinate,
            BROWSER_CREATE_METHOD,
            &serde_json::json!({ "schemaVersion": 1, "mode": "human_surface" }),
        )
        .expect("browser.create is implemented");
        let session_id = report["sessionId"].as_str().expect("sessionId");
        assert!(session_id.starts_with("brw-"));
        assert_eq!(report["state"], "created");
        assert_eq!(report["mode"], "human_surface");

        // The creating connection owns the session and is subscribed; the
        // created event is queued for it.
        let created = subscriber
            .recv_timeout(std::time::Duration::from_millis(200))
            .expect("created event routed to the owner");
        assert_eq!(created.session_id(), session_id);

        let listed = dispatch(
            &ctx,
            &coordinate,
            BROWSER_LIST_METHOD,
            &serde_json::json!({}),
        )
        .expect("browser.list is implemented");
        assert_eq!(listed["browsers"].as_array().map(Vec::len), Some(1));
        assert_eq!(listed["browsers"][0]["sessionId"], session_id);

        dispatch(
            &ctx,
            &coordinate,
            BROWSER_CLOSE_METHOD,
            &serde_json::json!({ "schemaVersion": 1, "sessionId": session_id }),
        )
        .expect("browser.close is implemented");

        let listed = dispatch(
            &ctx,
            &coordinate,
            BROWSER_LIST_METHOD,
            &serde_json::json!({}),
        )
        .expect("browser.list is implemented");
        assert_eq!(listed["browsers"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn placeholder_capabilities_return_static_shapes() {
        // browser.list is real since M6-C3: an empty host returns an empty
        // list (same shape as the M6-B1 placeholder).
        let browser = dispatch(
            &context(),
            &coordinate(BROWSER_API_VERSION, BROWSER_KIND),
            BROWSER_LIST_METHOD,
            &serde_json::json!({}),
        )
        .expect("browser.list is implemented");
        assert_eq!(browser["browsers"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn runtime_status_requires_environment_id() {
        // runtime.status is real since M6-C2: it needs the schema
        // environmentId payload; the M6-B1 placeholder empty payload is
        // malformed now.
        let error = dispatch(
            &context(),
            &coordinate(RUNTIME_API_VERSION, RUNTIME_KIND),
            RUNTIME_STATUS_METHOD,
            &serde_json::json!({}),
        )
        .expect_err("empty payload is not a runtime.status request");
        assert!(matches!(
            error,
            DaemonMethodError::MethodFailed {
                code: ErrorCode::MalformedMessage,
                ..
            }
        ));
    }

    #[test]
    fn runtime_status_unknown_environment_is_unavailable() {
        // The test host points at a nonexistent catalog: a well-formed
        // request for an environment that is not in it fails with
        // UNAVAILABLE (fail-closed, not a silent empty report).
        let error = dispatch(
            &context(),
            &coordinate(RUNTIME_API_VERSION, RUNTIME_KIND),
            RUNTIME_STATUS_METHOD,
            &serde_json::json!({
                "schemaVersion": 1,
                "environmentId": "managed-local",
            }),
        )
        .expect_err("environment not in the catalog");
        assert!(matches!(
            error,
            DaemonMethodError::MethodFailed {
                code: ErrorCode::Unavailable,
                ..
            }
        ));
    }

    #[test]
    fn terminal_status_on_empty_host_is_empty_list() {
        let terminal = dispatch(
            &context(),
            &coordinate(TERMINAL_API_VERSION, TERMINAL_KIND),
            TERMINAL_STATUS_METHOD,
            &serde_json::json!({}),
        )
        .expect("terminal.status is implemented");
        assert_eq!(terminal["count"], 0);
        assert_eq!(terminal["sessions"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn terminal_create_bad_payload_is_malformed() {
        let error = dispatch(
            &context(),
            &coordinate(TERMINAL_API_VERSION, TERMINAL_KIND),
            TERMINAL_CREATE_METHOD,
            &serde_json::json!({}),
        )
        .expect_err("empty payload is not a create request");
        assert!(matches!(
            error,
            DaemonMethodError::MethodFailed {
                code: ErrorCode::MalformedMessage,
                ..
            }
        ));
    }

    #[test]
    fn scheduler_wake_dispatches_to_timer_host() {
        let ctx = context();
        let coordinate = coordinate(SCHEDULER_API_VERSION, SCHEDULER_KIND);
        let result = dispatch(
            &ctx,
            &coordinate,
            SCHEDULER_WAKE_METHOD,
            &serde_json::json!({
                "wakeId": "w-dispatch-01",
                "requestedAt": "2026-08-31T00:00:00.000Z",
                "reason": "scheduled_due",
            }),
        )
        .expect("wake is implemented");
        assert_eq!(result["wakeId"], "w-dispatch-01");
        assert_eq!(result["pending"], 1);

        let error = dispatch(
            &ctx,
            &coordinate,
            SCHEDULER_WAKE_METHOD,
            &serde_json::json!({ "wakeId": "short" }),
        )
        .expect_err("invalid wake payload");
        assert!(matches!(
            error,
            DaemonMethodError::InvalidPayload { method, .. } if method == SCHEDULER_WAKE_METHOD
        ));
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let error = dispatch(
            &context(),
            &coordinate(SYSTEM_API_VERSION, SYSTEM_KIND),
            "shutdown",
            &serde_json::json!({}),
        )
        .expect_err("shutdown is not implemented");
        assert!(matches!(
            error,
            DaemonMethodError::MethodNotFound { method, .. } if method == "shutdown"
        ));
        // Namespaced terminal methods are the catalog; anything else in
        // the terminal capability is not implemented either.
        let error = dispatch(
            &context(),
            &coordinate(TERMINAL_API_VERSION, TERMINAL_KIND),
            "terminal.shutdown",
            &serde_json::json!({}),
        )
        .expect_err("terminal.shutdown is not implemented");
        assert!(matches!(
            error,
            DaemonMethodError::MethodNotFound { method, .. } if method == "terminal.shutdown"
        ));
    }
}
