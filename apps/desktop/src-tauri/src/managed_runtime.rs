//! Thin Shell wrapper over the daemon runtime capability (M6-C4 proxy).
//!
//! Since M6-C2 the daemon owns the Managed DSH process tree (ADR-0019
//! decision 3 - the P0 Supervisor state machine lives daemon-side in
//! crates/managed-runtime). This module is the Shell-side envelope proxy:
//! start/stop/restart/status/verified-surface-binding are runtime.*
//! invocations through crate::daemon_client::DaemonConnector; the Shell
//! keeps no supervisor state (the daemon is the authority, runtime.* is
//! daemon-wide and not connection-owner-scoped, M6-C2). The tauri
//! commands keep their request/response types - the frontend contract is
//! unchanged.
//!
//! [ManagedRuntimeReport] is the Shell-side wire mirror of the crate's
//! report (the crate type serializes but cannot deserialize: &'static str
//! fields); the JSON shape is byte-identical to what the frontend already
//! consumed, and runtime_state()/generation() keep the crate accessors.

use serde::{Deserialize, Serialize};

use dsh_daemon::capabilities::{
    RUNTIME_API_VERSION, RUNTIME_BINDING_METHOD, RUNTIME_KIND, RUNTIME_RESTART_METHOD,
    RUNTIME_START_METHOD, RUNTIME_STATUS_METHOD, RUNTIME_STOP_METHOD,
};
use dsh_daemon::envelope::{ErrorCode, ProtocolCoordinate};

pub use dsh_managed_runtime::{
    ManagedRuntimeError, ManagedRuntimeRestartRequest, ManagedRuntimeStartRequest,
    ManagedRuntimeStatusRequest, ManagedRuntimeStopRequest, VerifiedSurfaceBinding,
};

use crate::commands::DshEnvironment;
use crate::daemon_client::{DaemonCommandError, DaemonConnector};

/// Runtime capability coordinate (runtime.dsh-desktop.local/v1alpha1 +
/// Runtime). Every proxied invocation addresses exactly this coordinate.
fn runtime_coordinate() -> ProtocolCoordinate {
    ProtocolCoordinate {
        api_version: RUNTIME_API_VERSION.into(),
        kind: RUNTIME_KIND.into(),
    }
}

/// Wire mirror of the public ManagedRuntimeReport (see module docs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntimeReport {
    pub schema_version: u8,
    pub environment_id: String,
    pub ownership: String,
    pub state: String,
    pub generation: u64,
    pub instance_id: Option<String>,
    pub process_ownership: String,
    pub lifecycle_mutation: String,
    pub readiness: String,
    pub endpoint: Option<ManagedEndpointView>,
    pub stop_disposition: String,
    pub recovery: Option<RecoveryReportView>,
    pub observed_at_unix_ms: u64,
    pub evidence: Vec<EvidenceView>,
}

impl ManagedRuntimeReport {
    pub fn runtime_state(&self) -> &str {
        &self.state
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

/// Endpoint of the report (the daemon never leaks the bootstrap token;
/// the host + port are what the Surface binding needs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedEndpointView {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub source: String,
    pub verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReportView {
    pub crash_count: u64,
    pub window_start_unix_ms: u64,
    pub budget: u64,
    pub safe_stop: bool,
    pub last_crash_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceView {
    pub code: String,
    pub severity: String,
    pub message: String,
}

/// Start a new Managed generation (daemon-side; idempotent for the same
/// environment - a running generation reports instead of spawning).
pub fn start_managed_environment(
    connector: &dyn DaemonConnector,
    environment: &DshEnvironment,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "environmentId": environment.id(),
    });
    invoke_report(connector, RUNTIME_START_METHOD, payload)
}

/// Current runtime report (the daemon may run a bounded auto-restart for
/// a crashed generation with auto-restart policy - M1 status semantics
/// preserved daemon-side).
pub fn get_managed_runtime_status(
    connector: &dyn DaemonConnector,
    environment: &DshEnvironment,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "environmentId": environment.id(),
    });
    invoke_report(connector, RUNTIME_STATUS_METHOD, payload)
}

/// Verified surface binding (ADR-0012/0013 invariant preserved): the
/// daemon is the supervisor authority, so the Shell re-derives the
/// binding from the verified runtime.status report - generation match,
/// healthy state and a published endpoint. Only the daemon may verify;
/// the Shell only re-materializes the verified report (M6-C4).
pub fn verified_surface_binding(
    connector: &dyn DaemonConnector,
    environment: &DshEnvironment,
    expected_generation: u64,
) -> Result<VerifiedSurfaceBinding, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    if expected_generation == 0 {
        return Err(ManagedRuntimeError::StaleGeneration);
    }
    // runtime.binding (daemon-only channel): the daemon re-verifies the
    // exact generation and returns its private bootstrap URL (the
    // authenticated entry through the DSH web token exchange). The URL
    // never travels in the public status report (managed-runtime
    // redaction tests).
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "environmentId": environment.id(),
        "expectedGeneration": expected_generation,
    });
    let value = connector
        .invoke(runtime_coordinate(), RUNTIME_BINDING_METHOD, payload)
        .map_err(|error| {
            eprintln!("[managed-runtime] runtime.binding invoke failed: {error:?}");
            map_daemon_error(error)
        })?;
    let binding: SurfaceBindingReport = serde_json::from_value(value)
        .map_err(|_error| ManagedRuntimeError::StateUnavailable)?;
    if binding.schema_version != 1 {
        return Err(ManagedRuntimeError::StateUnavailable);
    }
    if binding.generation != expected_generation {
        return Err(ManagedRuntimeError::StaleGeneration);
    }
    let url = tauri::Url::parse(&binding.bootstrap_url).map_err(|_| {
        ManagedRuntimeError::StateUnavailable
    })?;
    Ok(VerifiedSurfaceBinding::new(
        binding.generation,
        binding.port,
        url,
    ))
}

/// Wire shape of the runtime.binding report (daemon-only; never reaches
/// the frontend contract).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceBindingReport {
    schema_version: u8,
    generation: u64,
    port: u16,
    bootstrap_url: String,
}

/// Stop the exact current generation (stale generations are rejected
/// daemon-side with STALE_GENERATION).
pub fn stop_managed_environment(
    connector: &dyn DaemonConnector,
    environment: &DshEnvironment,
    expected_generation: u64,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "environmentId": environment.id(),
        "expectedGeneration": expected_generation,
    });
    invoke_report(connector, RUNTIME_STOP_METHOD, payload)
}

/// Restart the exact current generation.
pub fn restart_managed_environment(
    connector: &dyn DaemonConnector,
    environment: &DshEnvironment,
    request: ManagedRuntimeRestartRequest,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    if !environment.is_managed() {
        return Err(ManagedRuntimeError::NotManaged);
    }
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "environmentId": environment.id(),
        "expectedGeneration": request.expected_generation(),
    });
    invoke_report(connector, RUNTIME_RESTART_METHOD, payload)
}

/// Invoke one runtime.* method and parse the report wire shape.
fn invoke_report(
    connector: &dyn DaemonConnector,
    method: &str,
    payload: serde_json::Value,
) -> Result<ManagedRuntimeReport, ManagedRuntimeError> {
    let value = connector
        .invoke(runtime_coordinate(), method, payload)
        .map_err(map_daemon_error)?;
    serde_json::from_value(value).map_err(|_| ManagedRuntimeError::StateUnavailable)
}

/// Reverse-map the daemon envelope error onto the Shell-side
/// ManagedRuntimeError contract (the daemon already translated the
/// supervisor failures, M6-C2 mapping - the codes survive the round
/// trip; the message text is daemon-side and surfaces in the frontend
/// through the code mapping).
fn map_daemon_error(error: DaemonCommandError) -> ManagedRuntimeError {
    match &error {
        DaemonCommandError::Remote {
            code,
            message,
            retryable: _,
        } => match code {
            ErrorCode::Conflict => ManagedRuntimeError::Conflict,
            ErrorCode::StaleGeneration => ManagedRuntimeError::StaleGeneration,
            ErrorCode::MalformedMessage => ManagedRuntimeError::InvalidEnvironment,
            ErrorCode::NotProcessOwner => ManagedRuntimeError::NotManaged,
            // Keep the daemon-side diagnostic text for unavailable codes so
            // managed launch failures stay diagnosable from the UI.
            _ => ManagedRuntimeError::RuntimeUnavailable(message.clone()),
        },
        // Connection-level failures: fail closed as an unavailable
        // runtime (retryable in the command mapping).
        _ => ManagedRuntimeError::StateUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_client::tests::MockConnector;

    fn environment() -> DshEnvironment {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "managed",
        }))
        .expect("environment fixture")
    }

    fn report_json(state: &str, generation: u64) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "environmentId": "managed-local",
            "ownership": "managed",
            "state": state,
            "generation": generation,
            "instanceId": null,
            "processOwnership": "owned",
            "lifecycleMutation": "allowed",
            "readiness": "verified",
            "endpoint": {
                "scheme": "http",
                "host": "127.0.0.1",
                "port": 41731,
                "source": "managed_process_output",
                "verification": "owned_generation_output_and_tcp",
            },
            "stopDisposition": "not_requested",
            "recovery": null,
            "observedAtUnixMs": 1_787_000_000_000u64,
            "evidence": [],
        })
    }

    #[test]
    fn start_proxies_runtime_start_and_parses_report() {
        let connector = MockConnector::ok(report_json("healthy", 3));
        let report = start_managed_environment(&connector, &environment()).expect("start");
        assert_eq!(report.runtime_state(), "healthy");
        assert_eq!(report.generation(), 3);
        let calls = connector.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Runtime");
        assert_eq!(calls[0].1, "runtime.start");
        assert_eq!(calls[0].2["environmentId"], "managed-local");
        assert_eq!(calls[0].2["schemaVersion"], 1);
    }

    #[test]
    fn status_stop_restart_proxy_with_generation() {
        let connector = MockConnector::sequential(vec![
            Ok(report_json("stopped", 2)),
            Ok(report_json("stopped", 2)),
            Ok(report_json("healthy", 3)),
        ]);
        let status = get_managed_runtime_status(&connector, &environment()).expect("status");
        assert_eq!(status.runtime_state(), "stopped");
        let stopped = stop_managed_environment(&connector, &environment(), 2).expect("stop");
        assert_eq!(stopped.generation(), 2);
        let restart_request: ManagedRuntimeRestartRequest =
            serde_json::from_value(serde_json::json!({
                "schemaVersion": 1,
                "environmentId": "managed-local",
                "expectedGeneration": 2,
            }))
            .expect("restart request");
        let restarted = restart_managed_environment(&connector, &environment(), restart_request)
            .expect("restart");
        assert_eq!(restarted.generation(), 3);
        let calls = connector.calls();
        assert_eq!(calls[0].1, "runtime.status");
        assert_eq!(calls[1].1, "runtime.stop");
        assert_eq!(calls[1].2["expectedGeneration"], 2);
        assert_eq!(calls[2].1, "runtime.restart");
        assert_eq!(calls[2].2["expectedGeneration"], 2);
    }

    #[test]
    fn non_managed_environment_is_rejected_locally() {
        // Ownership is Shell-side (the daemon catalog mirrors it); an
        // Attached environment must never reach runtime.start.
        let attached: DshEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "attached-local",
            "label": "Attached DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "attached",
        }))
        .expect("attached fixture");
        let connector = MockConnector::ok(report_json("healthy", 1));
        let error = start_managed_environment(&connector, &attached).expect_err("attached");
        assert!(matches!(error, ManagedRuntimeError::NotManaged));
        assert!(connector.calls().is_empty());
    }

    #[test]
    fn verified_surface_binding_uses_daemon_binding_channel() {
        // Healthy + matching generation -> binding with its bootstrap URL
        // (the daemon-only channel; never part of the public report).
        let connector = MockConnector::ok(binding_json(7, 41731, "http://127.0.0.1:41731/?token=abc123"));
        let binding = verified_surface_binding(&connector, &environment(), 7).expect("binding");
        assert_eq!(binding.generation(), 7);
        assert_eq!(binding.port(), 41731);
        assert_eq!(binding.url().as_str(), "http://127.0.0.1:41731/?token=abc123");
        let calls = connector.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "runtime.binding");
        assert_eq!(calls[0].2["expectedGeneration"], 7);

        // Generation mismatch -> stale (checked locally against the
        // daemon-verified report).
        let connector = MockConnector::ok(binding_json(7, 41731, "http://127.0.0.1:41731/?token=abc123"));
        let error = verified_surface_binding(&connector, &environment(), 8).expect_err("stale");
        assert!(matches!(error, ManagedRuntimeError::StaleGeneration));

        // Daemon cannot produce a binding -> the command contract maps
        // the remote Unavailable code onto RuntimeUnavailable, keeping the
        // daemon-side diagnostic text for the UI.
        let connector = MockConnector::error(DaemonCommandError::Remote {
            code: ErrorCode::Unavailable,
            message: "Managed endpoint is not a verified current-generation binding".into(),
            retryable: true,
        });
        let error =
            verified_surface_binding(&connector, &environment(), 7).expect_err("unavailable");
        assert!(matches!(
            error,
            ManagedRuntimeError::RuntimeUnavailable(message)
                if message == "Managed endpoint is not a verified current-generation binding"
        ));

        // expectedGeneration 0 -> stale, before any invocation.
        let connector = MockConnector::ok(binding_json(7, 41731, "http://127.0.0.1:41731/?token=abc123"));
        let error = verified_surface_binding(&connector, &environment(), 0).expect_err("zero");
        assert!(matches!(error, ManagedRuntimeError::StaleGeneration));
        assert!(connector.calls().is_empty());
    }

    fn binding_json(generation: u64, port: u16, bootstrap_url: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "generation": generation,
            "port": port,
            "bootstrapUrl": bootstrap_url,
        })
    }

    #[test]
    fn daemon_errors_map_onto_the_command_contract() {
        let connector = MockConnector::error(DaemonCommandError::Remote {
            code: ErrorCode::Conflict,
            message: "another environment is active".into(),
            retryable: true,
        });
        let error = start_managed_environment(&connector, &environment()).expect_err("conflict");
        assert!(matches!(error, ManagedRuntimeError::Conflict));

        let connector = MockConnector::error(DaemonCommandError::Remote {
            code: ErrorCode::StaleGeneration,
            message: "stale".into(),
            retryable: false,
        });
        let error = stop_managed_environment(&connector, &environment(), 1).expect_err("stale");
        assert!(matches!(error, ManagedRuntimeError::StaleGeneration));

        let connector = MockConnector::error(DaemonCommandError::NotConnected);
        let error = get_managed_runtime_status(&connector, &environment()).expect_err("offline");
        assert!(matches!(error, ManagedRuntimeError::StateUnavailable));
    }

    #[test]
    fn report_wire_mirror_matches_frontend_contract() {
        // The mirror serializes to the same shape the frontend consumes.
        let report: ManagedRuntimeReport =
            serde_json::from_value(report_json("healthy", 3)).expect("parses wire");
        let value = serde_json::to_value(&report).expect("serializes");
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["state"], "healthy");
        assert_eq!(value["endpoint"]["port"], 41731);
        assert_eq!(value["processOwnership"], "owned");
        assert_eq!(value["evidence"].as_array().map(Vec::len), Some(0));
    }
}
