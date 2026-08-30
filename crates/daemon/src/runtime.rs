//! Runtime capability of the daemon (M6-C2): the daemon **really owns**
//! the Managed DSH process tree (ADR-0019 decision 3 — managed_runtime
//! migrates into the daemon; cross-Shell-restart resource survival,
//! ADR-0008). The process-manager core lives in the tauri-free
//! `dsh-managed-runtime` crate (supervisor state machine, Windows Job
//! Object / unix process-group tree ownership); this module is the
//! daemon-facing host + envelope handlers.
//!
//! Environment resolution (M6-C2 decision): the Shell remains the
//! **writer** of the persisted catalog and the daemon is a **reader** —
//! every `runtime.*` invocation carries only `environmentId` and the
//! host resolves it against
//! `%APPDATA%/dev.dsh.desktop-shell/environment-catalog-v1.json`
//! ([`dsh_managed_runtime::CATALOG_FILE_NAME`]). The catalog is loaded
//! per invocation: the daemon always sees the Shell's latest persisted
//! environment edits without a restart.
//!
//! Authorization (M6-C2 decision, mirrors the terminal human path):
//! the authenticated local-transport connection + the negotiated
//! `runtime` capability grant in the Agreement **is** the
//! authorization. The runtime is a daemon-wide resource (a Shell
//! restart must be able to take over and stop the surviving tree), so
//! methods are **not** connection-owner-scoped — unlike terminal
//! sessions (per-session ownership), any authenticated runtime-granted
//! connection may start/stop/restart/status. Agent automation of the
//! Managed runtime is out of scope for M6-C2.
//!
//! Wire contract: payload shapes mirror specs/runtime/*.schema.json
//! (ManagedRuntimeStartRequest/StatusRequest/StopRequest/RestartRequest)
//! and the result is the M1 `ManagedRuntimeReport` (bootstrap token
//! never leaves the supervisor — the report shape has no URL field).

use std::path::PathBuf;

use dsh_managed_runtime::{
    CATALOG_FILE_NAME, CatalogError, ManagedEnvironment, ManagedRuntimeError, ManagedRuntimeReport,
    ManagedRuntimeRestartRequest, ManagedRuntimeStartRequest, ManagedRuntimeState,
    ManagedRuntimeStatusRequest, ManagedRuntimeStopRequest, get_managed_runtime_status,
    is_valid_id, load_catalog, restart_managed_environment, start_managed_environment,
    stop_managed_environment,
};

use crate::capabilities::{CapabilityContext, DaemonMethodError};
use crate::envelope::ErrorCode;

/// Default catalog path: the daemon data directory (same directory the
/// Shell writes the catalog into).
pub fn default_catalog_path() -> PathBuf {
    crate::credential::data_dir().join(CATALOG_FILE_NAME)
}

/// Why a runtime operation could not run against the catalog/host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeHostError {
    /// The catalog file exists but is not a valid v1 catalog.
    CatalogCorrupt,
    /// The catalog file could not be read.
    CatalogUnavailable,
    /// The requested environmentId is not in the catalog.
    EnvironmentNotFound(String),
    /// The Managed runtime itself rejected the operation.
    Managed(ManagedRuntimeError),
}

/// The daemon-owned Managed runtime host: the supervisor state machine
/// (`dsh-managed-runtime`) plus the catalog path the environments are
/// resolved from. One host per daemon, Arc-shared with the capability
/// handlers.
pub struct ManagedRuntimeHost {
    state: ManagedRuntimeState,
    catalog_path: PathBuf,
}

impl Default for ManagedRuntimeHost {
    fn default() -> Self {
        Self::new(default_catalog_path())
    }
}

impl ManagedRuntimeHost {
    pub fn new(catalog_path: PathBuf) -> Self {
        Self {
            state: ManagedRuntimeState::default(),
            catalog_path,
        }
    }

    /// The supervisor handle (envelope handlers drive it).
    pub fn state(&self) -> &ManagedRuntimeState {
        &self.state
    }

    /// Resolve one environment from the persisted catalog (fail-closed:
    /// a corrupt catalog never launches anything).
    pub fn environment(
        &self,
        environment_id: &str,
    ) -> Result<ManagedEnvironment, RuntimeHostError> {
        let catalog = load_catalog(&self.catalog_path).map_err(|error| match error {
            CatalogError::Corrupt => RuntimeHostError::CatalogCorrupt,
            CatalogError::Unavailable => RuntimeHostError::CatalogUnavailable,
        })?;
        catalog
            .environment(environment_id)
            .cloned()
            .ok_or_else(|| RuntimeHostError::EnvironmentNotFound(environment_id.to_string()))
    }

    /// Live Managed generations currently retained (0 or 1 — the
    /// Supervisor owns exactly one environment at a time, and a clean
    /// stop releases the process tree).
    pub fn managed_runtimes(&self) -> usize {
        usize::from(self.state.has_live_generation())
    }
}

/// `runtime.start`: resolve the catalog environment and start a new
/// Managed generation (idempotent for the same environment: a running
/// generation reports instead of spawning a second tree).
pub fn handle_start(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: ManagedRuntimeStartRequest = parse_request("runtime.start", payload)?;
    validate_request(&request.schema_version(), request.environment_id())?;
    let environment = ctx
        .runtime
        .environment(request.environment_id())
        .map_err(host_error)?;
    let report =
        start_managed_environment(ctx.runtime.state(), &environment).map_err(managed_error)?;
    Ok(report_value(report))
}

/// `runtime.status`: the current report of the environment (may run a
/// bounded auto-restart for a crashed generation with auto-restart
/// policy — the M1 status semantics are preserved).
pub fn handle_status(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: ManagedRuntimeStatusRequest = parse_request("runtime.status", payload)?;
    validate_request(&request.schema_version(), request.environment_id())?;
    let environment = ctx
        .runtime
        .environment(request.environment_id())
        .map_err(host_error)?;
    let report =
        get_managed_runtime_status(ctx.runtime.state(), &environment).map_err(managed_error)?;
    Ok(report_value(report))
}

/// `runtime.stop`: stop the exact current generation (stale generations
/// are rejected with STALE_GENERATION).
pub fn handle_stop(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: ManagedRuntimeStopRequest = parse_request("runtime.stop", payload)?;
    validate_request(&request.schema_version(), request.environment_id())?;
    if request.expected_generation() == 0 {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "runtime.stop requires expectedGeneration >= 1",
            false,
        ));
    }
    let environment = ctx
        .runtime
        .environment(request.environment_id())
        .map_err(host_error)?;
    let report = stop_managed_environment(
        ctx.runtime.state(),
        &environment,
        request.expected_generation(),
    )
    .map_err(managed_error)?;
    Ok(report_value(report))
}

/// `runtime.restart`: stop the exact current generation and start a new
/// one (stale generations are rejected).
pub fn handle_restart(
    ctx: &CapabilityContext,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DaemonMethodError> {
    let request: ManagedRuntimeRestartRequest = parse_request("runtime.restart", payload)?;
    validate_request(&request.schema_version(), request.environment_id())?;
    if request.expected_generation() == 0 {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "runtime.restart requires expectedGeneration >= 1",
            false,
        ));
    }
    let environment = ctx
        .runtime
        .environment(request.environment_id())
        .map_err(host_error)?;
    let report = restart_managed_environment(ctx.runtime.state(), &environment, request)
        .map_err(managed_error)?;
    Ok(report_value(report))
}

// ----------------------------------------------------------------------
// Validation / error mapping
// ----------------------------------------------------------------------

fn parse_request<T: serde::de::DeserializeOwned>(
    method: &str,
    payload: &serde_json::Value,
) -> Result<T, DaemonMethodError> {
    serde_json::from_value(payload.clone()).map_err(|error| {
        failed(
            ErrorCode::MalformedMessage,
            format!("{method} payload does not match the request shape: {error}"),
            false,
        )
    })
}

fn validate_request(schema_version: &u8, environment_id: &str) -> Result<(), DaemonMethodError> {
    if *schema_version != 1 || !is_valid_id(environment_id) {
        return Err(failed(
            ErrorCode::MalformedMessage,
            "runtime request requires schemaVersion 1 and a valid environmentId",
            false,
        ));
    }
    Ok(())
}

fn report_value(report: ManagedRuntimeReport) -> serde_json::Value {
    serde_json::to_value(report).expect("managed runtime report serializes")
}

/// Host/catalog failures -> envelope error contract.
fn host_error(error: RuntimeHostError) -> DaemonMethodError {
    match error {
        RuntimeHostError::CatalogCorrupt => failed(
            ErrorCode::Unavailable,
            "the environment catalog is corrupt; re-save an environment from the Shell",
            false,
        ),
        RuntimeHostError::CatalogUnavailable => failed(
            ErrorCode::Unavailable,
            "the environment catalog is unavailable",
            true,
        ),
        RuntimeHostError::EnvironmentNotFound(id) => failed(
            ErrorCode::Unavailable,
            format!("environment \"{id}\" is not in the catalog; save it from the Shell first"),
            false,
        ),
        RuntimeHostError::Managed(error) => managed_error(error),
    }
}

/// Managed runtime failures -> envelope error contract (mirrors the M1
/// Shell command mapping codes).
fn managed_error(error: ManagedRuntimeError) -> DaemonMethodError {
    match error {
        ManagedRuntimeError::NotManaged => failed(
            ErrorCode::Conflict,
            "the catalog environment is not Managed",
            false,
        ),
        ManagedRuntimeError::InvalidEnvironment => failed(
            ErrorCode::MalformedMessage,
            "the catalog environment failed validation",
            false,
        ),
        ManagedRuntimeError::UnsupportedSource => failed(
            ErrorCode::MalformedMessage,
            "Managed start requires an existing executable or a prebuilt source recipe",
            false,
        ),
        ManagedRuntimeError::NodeOverrideUnsupported => failed(
            ErrorCode::MalformedMessage,
            "Managed source start requires an absolute existing Node executable",
            false,
        ),
        ManagedRuntimeError::Conflict => failed(
            ErrorCode::Conflict,
            "another Managed environment or lifecycle transition is active",
            true,
        ),
        ManagedRuntimeError::StaleGeneration => failed(
            ErrorCode::StaleGeneration,
            "the Managed lifecycle request targets a stale generation",
            false,
        ),
        ManagedRuntimeError::CandidateInvalid | ManagedRuntimeError::CandidatePortMismatch => {
            failed(
                ErrorCode::Unavailable,
                "Managed endpoint candidate failed publication policy",
                false,
            )
        }
        ManagedRuntimeError::ProcessExited => failed(
            ErrorCode::Unavailable,
            "Managed process exited before readiness",
            true,
        ),
        ManagedRuntimeError::ReadinessTimeout => {
            failed(ErrorCode::Unavailable, "Managed readiness timed out", true)
        }
        ManagedRuntimeError::EndpointStillReachable => failed(
            ErrorCode::Unavailable,
            "Managed process stopped, but the previous endpoint remains reachable",
            true,
        ),
        ManagedRuntimeError::SurfaceBindingUnavailable => failed(
            ErrorCode::Unavailable,
            "Managed endpoint is not a verified current-generation binding",
            true,
        ),
        ManagedRuntimeError::SpawnUnavailable
        | ManagedRuntimeError::ProcessTreeUnavailable
        | ManagedRuntimeError::StopFailed
        | ManagedRuntimeError::StateUnavailable
        | ManagedRuntimeError::ClockUnavailable => failed(
            ErrorCode::Unavailable,
            "Managed runtime is unavailable",
            true,
        ),
    }
}

fn failed(code: ErrorCode, message: impl Into<String>, retryable: bool) -> DaemonMethodError {
    DaemonMethodError::MethodFailed {
        code,
        message: message.into(),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_mapping_preserves_wire_codes() {
        fn code_of(error: DaemonMethodError) -> ErrorCode {
            match error {
                DaemonMethodError::MethodFailed { code, .. } => code,
                other => panic!("expected MethodFailed, got {other:?}"),
            }
        }
        fn retryable_of(error: DaemonMethodError) -> bool {
            match error {
                DaemonMethodError::MethodFailed { retryable, .. } => retryable,
                other => panic!("expected MethodFailed, got {other:?}"),
            }
        }
        assert_eq!(
            code_of(managed_error(ManagedRuntimeError::StaleGeneration)),
            ErrorCode::StaleGeneration,
        );
        assert_eq!(
            code_of(managed_error(ManagedRuntimeError::Conflict)),
            ErrorCode::Conflict,
        );
        assert_eq!(
            code_of(host_error(RuntimeHostError::EnvironmentNotFound(
                "x".into()
            ))),
            ErrorCode::Unavailable,
        );
        assert_eq!(
            code_of(host_error(RuntimeHostError::CatalogCorrupt)),
            ErrorCode::Unavailable,
        );
        // Spawn/readiness failures are retryable; policy rejections are not.
        assert!(retryable_of(managed_error(
            ManagedRuntimeError::SpawnUnavailable
        )));
        assert!(!retryable_of(managed_error(
            ManagedRuntimeError::InvalidEnvironment
        )));
    }

    #[test]
    fn request_validation_matrix() {
        assert!(validate_request(&1, "managed-local").is_ok());
        assert!(matches!(
            validate_request(&2, "managed-local"),
            Err(DaemonMethodError::MethodFailed {
                code: ErrorCode::MalformedMessage,
                ..
            })
        ));
        assert!(matches!(
            validate_request(&1, "Not Valid!"),
            Err(DaemonMethodError::MethodFailed {
                code: ErrorCode::MalformedMessage,
                ..
            })
        ));
    }
}
