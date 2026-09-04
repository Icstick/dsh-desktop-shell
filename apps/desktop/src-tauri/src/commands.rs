use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::attached_health::{
    self, AttachedHealthError, AttachedHealthReport, AttachedHealthRequest,
};
use crate::diagnostics::{self, DiagnosticsError, DiagnosticsReport, DiagnosticsRequest};
use crate::discovery::{self, DiscoveryError, HarnessDiscoveryReport, HarnessDiscoveryRequest};
use crate::dsh_surface::{
    self, DshSurfaceError, DshSurfaceLayoutRequest, DshSurfaceMountRequest,
    DshSurfaceReloadRequest, DshSurfaceState, DshSurfaceStatus, DshSurfaceStatusRequest,
    DshSurfaceUnmountRequest,
};
use crate::dsh_surface_policy::{
    self, DshSurfaceNavigationDecision, DshSurfaceNavigationRequest, DshSurfacePolicy,
    DshSurfacePolicyError, DshSurfacePolicyRequest,
};
use crate::environment_store::{self, EnvironmentCatalog, StoreError};
use crate::managed_runtime::{
    self, ManagedRuntimeError, ManagedRuntimeReport, ManagedRuntimeRestartRequest,
    ManagedRuntimeStartRequest, ManagedRuntimeStatusRequest, ManagedRuntimeStopRequest,
};
use crate::notification::{self, NotificationError};
use crate::usage::{self, UsageError};

const RESERVED_ARGUMENTS: [&str; 4] = ["--host", "--port", "--no-open", "--trusted-host"];
const CATALOG_FILE_NAME: &str = "environment-catalog-v1.json";
static ERROR_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellSnapshot {
    phase: &'static str,
    runtime_state: String,
    environment_id: Option<String>,
    generation: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshEnvironment {
    schema_version: u8,
    id: String,
    label: String,
    harness: HarnessSource,
    dsh_home: String,
    profile: String,
    node_path: Option<String>,
    endpoint: Endpoint,
    ownership: Ownership,
    policy: Option<EnvironmentPolicy>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HarnessSource {
    mode: HarnessMode,
    path: String,
    cwd: Option<String>,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HarnessMode {
    Repository,
    Executable,
    Command,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    host: String,
    port: EndpointPort,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum EndpointPort {
    Named(String),
    Fixed(u16),
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Ownership {
    Managed,
    Attached,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EnvironmentPolicy {
    auto_restart_on_crash: Option<bool>,
    allow_native_adapter: Option<bool>,
}

// The policy flags are Shell-side persistence; the Managed runtime
// consumes auto_restart_on_crash through the dsh-managed-runtime crate
// (the wrapper converts DshEnvironment before launching).

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentValidation {
    valid: bool,
    issues: Vec<ValidationIssue>,
    launch_preview: Option<LaunchPreview>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationIssue {
    field: &'static str,
    code: &'static str,
    message: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchPreview {
    source: &'static str,
    executable: String,
    cwd: Option<String>,
    ownership: Ownership,
    endpoint: String,
    arguments: Vec<LaunchArgumentPreview>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchArgumentPreview {
    category: &'static str,
    display: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: &'static str,
    /// Dynamic-capable message (daemon RPC reasons are surfaced verbatim so
    /// managed launch failures stay diagnosable from the UI).
    message: String,
    retryable: bool,
    correlation_id: String,
    issues: Vec<ValidationIssue>,
}

impl DshEnvironment {
    pub(crate) fn dsh_home(&self) -> &str {
        &self.dsh_home
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn is_attached(&self) -> bool {
        matches!(self.ownership, Ownership::Attached)
    }

    pub(crate) fn is_managed(&self) -> bool {
        matches!(self.ownership, Ownership::Managed)
    }

    pub(crate) fn fixed_loopback_port(&self) -> Option<u16> {
        if self.endpoint.host != "127.0.0.1" {
            return None;
        }
        match &self.endpoint.port {
            EndpointPort::Fixed(port) => Some(*port),
            EndpointPort::Named(_) => None,
        }
    }

    fn runtime_state(&self) -> &'static str {
        match self.ownership {
            Ownership::Managed => "stopped",
            Ownership::Attached => "unavailable",
        }
    }
}

impl EnvironmentValidation {
    pub(crate) fn is_valid(&self) -> bool {
        self.valid
    }
}

impl CommandError {
    fn invalid_environment(issues: Vec<ValidationIssue>) -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Environment validation failed.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues,
        }
    }

    fn malformed_setup_assist() -> Self {
        Self::unavailable("Setup assistance request is malformed.", false)
    }

    fn setup_assist_io() -> Self {
        Self::unavailable("Setup assistance could not inspect the environment.", false)
    }

    fn malformed_discovery() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Harness discovery request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_dsh_surface_policy_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "DSH Surface policy request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_dsh_surface_navigation_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "DSH Surface navigation request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn from_dsh_surface_policy(error: DshSurfacePolicyError) -> Self {
        match error {
            DshSurfacePolicyError::FixedEndpointRequired => Self::unavailable(
                "DSH Surface policy requires a fixed loopback endpoint.",
                false,
            ),
        }
    }

    fn malformed_attached_health_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Attached health request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_diagnostics_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Diagnostics request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_managed_runtime_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Managed runtime request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_notification_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Notification request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn from_notification(error: NotificationError) -> Self {
        match error {
            NotificationError::MalformedRequest => Self::malformed_notification_request(),
            NotificationError::AuditUnavailable | NotificationError::ClockUnavailable => {
                Self::unavailable("Notification registry is unavailable.", true)
            }
        }
    }

    fn malformed_usage_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Usage snapshot request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn from_usage(error: UsageError) -> Self {
        match error {
            UsageError::MalformedRequest => Self::malformed_usage_request(),
            UsageError::StoreUnavailable | UsageError::ClockUnavailable => {
                Self::unavailable("Usage ledger is unavailable.", true)
            }
        }
    }

    fn malformed_dsh_surface_lifecycle_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "DSH Surface lifecycle request is invalid.".into(),
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn from_dsh_surface(error: DshSurfaceError) -> Self {
        match error {
            DshSurfaceError::MalformedRequest => Self::malformed_dsh_surface_lifecycle_request(),
            DshSurfaceError::StaleGeneration => Self {
                code: "STALE_GENERATION",
                message: "The DSH Surface request targets a stale generation.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            DshSurfaceError::SurfaceUnavailable | DshSurfaceError::StateUnavailable => {
                Self::unavailable("Native DSH Surface is unavailable.", true)
            }
        }
    }

    fn from_managed_runtime(error: ManagedRuntimeError) -> Self {
        match error {
            ManagedRuntimeError::NotManaged => Self {
                code: "NOT_PROCESS_OWNER",
                message: "Managed lifecycle requires a Managed environment.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            ManagedRuntimeError::InvalidEnvironment => Self::invalid_environment(Vec::new()),
            ManagedRuntimeError::UnsupportedSource => Self::unavailable(
                "Managed start source is missing or is not a deepseek-harness checkout (entry or TS loader not found).",
                false,
            ),
            ManagedRuntimeError::NodeOverrideUnsupported => Self::unavailable(
                "Managed start needs an absolute existing Node executable (set nodePath or add node to PATH).",
                false,
            ),
            ManagedRuntimeError::Conflict => Self {
                code: "CONFLICT",
                message: "Another Managed environment or lifecycle transition is active.".into(),
                retryable: true,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            ManagedRuntimeError::StaleGeneration => Self {
                code: "STALE_GENERATION",
                message: "The Managed lifecycle request targets a stale generation.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            ManagedRuntimeError::CandidateInvalid | ManagedRuntimeError::CandidatePortMismatch => {
                Self::unavailable(
                    "Managed endpoint candidate failed publication policy.",
                    false,
                )
            }
            ManagedRuntimeError::ProcessExited => {
                Self::unavailable("Managed process exited before readiness.", true)
            }
            ManagedRuntimeError::ReadinessTimeout => {
                Self::unavailable("Managed readiness timed out.", true)
            }
            ManagedRuntimeError::EndpointStillReachable => Self::unavailable(
                "Managed process stopped, but the previous endpoint remains reachable.",
                true,
            ),
            ManagedRuntimeError::SurfaceBindingUnavailable => Self::unavailable(
                "Managed endpoint is not a verified current-generation Surface binding.",
                true,
            ),
            ManagedRuntimeError::SpawnFailed(reason) => Self {
                code: "UNAVAILABLE",
                message: format!(
                    "Managed process could not be started: {}",
                    truncate_error(&reason, 400)
                ),
                retryable: true,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            ManagedRuntimeError::ProcessTreeFailed(reason) => Self {
                code: "UNAVAILABLE",
                message: format!(
                    "Managed process tree could not be attached: {}",
                    truncate_error(&reason, 400)
                ),
                retryable: true,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            ManagedRuntimeError::RuntimeUnavailable(reason) => Self {
                code: "UNAVAILABLE",
                message: truncate_error(&reason, 400),
                retryable: true,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            ManagedRuntimeError::StopFailed
            | ManagedRuntimeError::StateUnavailable
            | ManagedRuntimeError::ClockUnavailable => {
                Self::unavailable("Managed runtime is unavailable.", true)
            }
        }
    }

    fn from_diagnostics(error: DiagnosticsError) -> Self {
        match error {
            DiagnosticsError::MalformedRequest => Self::malformed_diagnostics_request(),
            DiagnosticsError::EnvironmentNotFound => {
                Self::unavailable("Diagnostics environment is unavailable.", false)
            }
            DiagnosticsError::NotManaged => Self {
                code: "NOT_PROCESS_OWNER",
                message: "Diagnostics requires a Managed environment.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            DiagnosticsError::CatalogUnavailable
            | DiagnosticsError::StateUnavailable
            | DiagnosticsError::ClockUnavailable => {
                Self::unavailable("Diagnostics are unavailable.", true)
            }
        }
    }

    fn from_attached_health(error: AttachedHealthError) -> Self {
        match error {
            AttachedHealthError::NotAttached => Self {
                code: "NOT_PROCESS_OWNER",
                message: "Health probe requires an Attached environment.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            AttachedHealthError::FixedPortRequired => Self {
                code: "UNAVAILABLE",
                message: "Attached health requires a fixed loopback port.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            AttachedHealthError::ClockUnavailable => {
                Self::unavailable("Attached health timestamp is unavailable.", true)
            }
        }
    }

    fn unavailable(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: "UNAVAILABLE",
            message: message.into(),
            retryable,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn from_store(error: StoreError) -> Self {
        match error {
            StoreError::Corrupt => {
                Self::unavailable("Environment catalog failed integrity validation.", false)
            }
            StoreError::InvalidEnvironment => Self {
                code: "MALFORMED_MESSAGE",
                message: "Environment catalog contains an invalid environment.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            StoreError::Capacity => Self {
                code: "CONFLICT",
                message: "Environment catalog capacity has been reached.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            StoreError::NotFound => Self {
                code: "NOT_FOUND",
                message: "Environment is not in the catalog.".into(),
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            StoreError::Unavailable => {
                Self::unavailable("Environment catalog is unavailable.", true)
            }
        }
    }
}

/// Bound a backend error string before it travels into a CommandError
/// message shown in the UI (spawn/attach errors may carry long os text).
fn truncate_error(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(max_chars).collect();
    truncated.push_str("…");
    truncated
}

fn next_correlation_id() -> String {
    format!(
        "desktop-{}-{}",
        std::process::id(),
        ERROR_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn catalog_path(app: &AppHandle) -> Result<PathBuf, CommandError> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(CATALOG_FILE_NAME))
        .map_err(|_| CommandError::unavailable("Application data directory is unavailable.", true))
}

#[tauri::command]
pub async fn get_shell_snapshot(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
) -> Result<ShellSnapshot, CommandError> {
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let active = catalog.active_environment().cloned();
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;

    // Snapshot consults the daemon supervisor, which may run a bounded
    // auto-restart (see get_managed_runtime_status); keep it off the main
    // thread.
    tauri::async_runtime::spawn_blocking(move || {
        let (runtime_state, generation) = match &active {
            Some(environment) if environment.is_managed() => {
                let report =
                    managed_runtime::get_managed_runtime_status(connector.as_ref(), environment)
                        .map_err(CommandError::from_managed_runtime)?;
                (report.runtime_state().to_string(), report.generation())
            }
            Some(environment) => (environment.runtime_state().to_string(), 0),
            None => ("unconfigured".to_string(), 0),
        };
        Ok(ShellSnapshot {
            phase: "shell-mvp",
            runtime_state,
            environment_id: active
                .as_ref()
                .map(|environment| environment.id().to_string()),
            generation,
        })
    })
    .await
    .map_err(|_| CommandError::unavailable("Shell snapshot task is unavailable.", true))?
}

#[tauri::command]
pub fn get_environment_catalog(app: AppHandle) -> Result<EnvironmentCatalog, CommandError> {
    environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)
}

#[tauri::command]
pub fn get_dsh_surface_policy(
    app: AppHandle,
    request: DshSurfacePolicyRequest,
) -> Result<DshSurfacePolicy, CommandError> {
    if request.schema_version() != 1 || !is_valid_id(request.environment_id()) {
        return Err(CommandError::malformed_dsh_surface_policy_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .ok_or_else(|| CommandError::unavailable("DSH Surface policy is unavailable.", false))?;

    dsh_surface_policy::derive_policy(environment).map_err(CommandError::from_dsh_surface_policy)
}

#[tauri::command]
pub fn evaluate_dsh_surface_navigation(
    app: AppHandle,
    request: DshSurfaceNavigationRequest,
) -> Result<DshSurfaceNavigationDecision, CommandError> {
    if request.schema_version() != 1 || !is_valid_id(request.environment_id()) {
        return Err(CommandError::malformed_dsh_surface_navigation_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .ok_or_else(|| CommandError::unavailable("DSH Surface policy is unavailable.", false))?;

    Ok(dsh_surface_policy::evaluate_navigation(
        environment,
        &request,
    ))
}

#[tauri::command]
pub fn save_environment(
    app: AppHandle,
    environment: DshEnvironment,
) -> Result<EnvironmentCatalog, CommandError> {
    let validation = validate_environment_value(environment.clone());
    if !validation.is_valid() {
        return Err(CommandError::invalid_environment(validation.issues));
    }

    environment_store::save_environment(&catalog_path(&app)?, environment)
        .map_err(CommandError::from_store)
}

/// Open a native folder picker for the wizard browse buttons. Returns
/// null when the user cancels. Pure UI affordance: no filesystem access
/// happens in the Shell (the picked path is only stored into the draft).
/// Runs on a blocking worker so the native dialog never stalls the main
/// thread (repository convention: blocking commands are async + spawn_blocking).
#[tauri::command]
pub async fn pick_directory(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        app.dialog()
            .file()
            .blocking_pick_folder()
            .map(|folder| folder.to_string())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub fn discover_harnesses(
    request: HarnessDiscoveryRequest,
) -> Result<HarnessDiscoveryReport, CommandError> {
    discovery::discover_harnesses(request).map_err(|error| match error {
        DiscoveryError::MalformedRequest => CommandError::malformed_discovery(),
    })
}

/// Switch the active environment in the catalog (B1 multi-profile).
#[tauri::command]
pub fn set_active_environment(
    app: AppHandle,
    request: SetActiveEnvironmentRequest,
) -> Result<EnvironmentCatalog, CommandError> {
    if request.schema_version != 1 || !is_valid_id(&request.environment_id) {
        return Err(CommandError::unavailable(
            "Environment activation request is malformed.",
            false,
        ));
    }
    environment_store::set_active_environment(&catalog_path(&app)?, &request.environment_id)
        .map_err(CommandError::from_store)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetActiveEnvironmentRequest {
    pub schema_version: u8,
    pub environment_id: String,
}

/// Remove an environment from the catalog (env quick-edit card action).
/// Removing the active environment clears the active selection; the Shell
/// returns to the empty surface state. Running managed process trees are
/// the caller's responsibility (the Shell stops them before removing).
#[tauri::command]
pub fn remove_environment(
    app: AppHandle,
    environment_id: String,
) -> Result<EnvironmentCatalog, CommandError> {
    if !is_valid_id(&environment_id) {
        return Err(CommandError::unavailable(
            "Environment removal request is malformed.",
            false,
        ));
    }
    environment_store::remove_environment(&catalog_path(&app)?, &environment_id)
        .map_err(CommandError::from_store)
}

#[tauri::command]
pub fn discover_profiles(
    request: crate::setup_assist::DiscoverProfilesRequest,
) -> Result<crate::setup_assist::DiscoverProfilesReport, CommandError> {
    crate::setup_assist::discover_profiles(&request).map_err(|error| match error {
        crate::setup_assist::SetupAssistError::MalformedRequest => {
            CommandError::malformed_setup_assist()
        }
        crate::setup_assist::SetupAssistError::HomeMissing
        | crate::setup_assist::SetupAssistError::HomeNotDirectory
        | crate::setup_assist::SetupAssistError::Io(_) => CommandError::setup_assist_io(),
    })
}

/// Probe whether a loopback port is in use (M7-A setup wizard).
#[tauri::command]
pub fn probe_port(
    request: crate::setup_assist::ProbePortRequest,
) -> Result<crate::setup_assist::ProbePortReport, CommandError> {
    crate::setup_assist::probe_port(&request).map_err(|error| match error {
        crate::setup_assist::SetupAssistError::MalformedRequest => {
            CommandError::malformed_setup_assist()
        }
        _ => CommandError::setup_assist_io(),
    })
}

#[tauri::command]
pub async fn probe_attached_environment(
    app: AppHandle,
    request: AttachedHealthRequest,
) -> Result<AttachedHealthReport, CommandError> {
    if request.schema_version() != 1 || !is_valid_id(request.environment_id()) {
        return Err(CommandError::malformed_attached_health_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .cloned()
        .ok_or_else(|| CommandError::unavailable("Attached environment is unavailable.", false))?;

    tauri::async_runtime::spawn_blocking(move || {
        attached_health::probe_attached_environment(&environment)
    })
    .await
    .map_err(|_| CommandError::unavailable("Attached health probe is unavailable.", true))?
    .map_err(CommandError::from_attached_health)
}

#[tauri::command]
pub async fn start_managed_environment(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    request: ManagedRuntimeStartRequest,
) -> Result<ManagedRuntimeReport, CommandError> {
    if request.schema_version() != 1 || !is_valid_id(request.environment_id()) {
        return Err(CommandError::malformed_managed_runtime_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .cloned()
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;

    tauri::async_runtime::spawn_blocking(move || {
        managed_runtime::start_managed_environment(connector.as_ref(), &environment)
    })
    .await
    .map_err(|_| CommandError::unavailable("Managed start task is unavailable.", true))?
    .map_err(CommandError::from_managed_runtime)
}

#[tauri::command]
pub async fn restart_managed_environment(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    surface_state: State<'_, DshSurfaceState>,
    request: ManagedRuntimeRestartRequest,
) -> Result<ManagedRuntimeReport, CommandError> {
    if request.schema_version() != 1 || !is_valid_id(request.environment_id()) {
        return Err(CommandError::malformed_managed_runtime_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .cloned()
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;
    let surface_state = surface_state.inner().clone();
    let environment_id = environment.id().to_string();

    let report = tauri::async_runtime::spawn_blocking(move || {
        managed_runtime::restart_managed_environment(connector.as_ref(), &environment, request)
    })
    .await
    .map_err(|_| CommandError::unavailable("Managed restart task is unavailable.", true))?
    .map_err(CommandError::from_managed_runtime)?;

    // The old generation's Surface binding died with the restart; close any
    // stale child WebView so no retired generation keeps a live window.
    let _ = dsh_surface::force_unmount(
        &app,
        &surface_state,
        &environment_id,
        report.generation().saturating_sub(1),
    );
    Ok(report)
}

#[tauri::command]
pub async fn mount_dsh_surface(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    surface_state: State<'_, DshSurfaceState>,
    request: DshSurfaceMountRequest,
) -> Result<DshSurfaceStatus, CommandError> {
    if !request.is_valid() {
        return Err(CommandError::malformed_dsh_surface_lifecycle_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;
    let binding = managed_runtime::verified_surface_binding(
        connector.as_ref(),
        environment,
        request.expected_generation(),
    )
    .map_err(CommandError::from_managed_runtime)?;
    dsh_surface::mount_surface(&app, &surface_state, binding, &request)
        .map_err(CommandError::from_dsh_surface)
}

#[tauri::command]
pub async fn get_dsh_surface_status(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    surface_state: State<'_, DshSurfaceState>,
    request: DshSurfaceStatusRequest,
) -> Result<DshSurfaceStatus, CommandError> {
    if !request.is_valid() {
        return Err(CommandError::malformed_dsh_surface_lifecycle_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;
    let binding = managed_runtime::verified_surface_binding(
        connector.as_ref(),
        environment,
        request.expected_generation(),
    )
    .map_err(CommandError::from_managed_runtime)?;
    dsh_surface::get_surface_status(&surface_state, binding, &request)
        .map_err(CommandError::from_dsh_surface)
}

#[tauri::command]
pub async fn update_dsh_surface_layout(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    surface_state: State<'_, DshSurfaceState>,
    request: DshSurfaceLayoutRequest,
) -> Result<DshSurfaceStatus, CommandError> {
    if !request.is_valid() {
        return Err(CommandError::malformed_dsh_surface_lifecycle_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;
    let binding = managed_runtime::verified_surface_binding(
        connector.as_ref(),
        environment,
        request.expected_generation(),
    )
    .map_err(CommandError::from_managed_runtime)?;
    dsh_surface::update_surface_layout(&app, &surface_state, binding, &request)
        .map_err(CommandError::from_dsh_surface)
}

#[tauri::command]
pub async fn reload_dsh_surface(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    surface_state: State<'_, DshSurfaceState>,
    request: DshSurfaceReloadRequest,
) -> Result<DshSurfaceStatus, CommandError> {
    if !request.is_valid() {
        return Err(CommandError::malformed_dsh_surface_lifecycle_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;
    let binding = managed_runtime::verified_surface_binding(
        connector.as_ref(),
        environment,
        request.expected_generation(),
    )
    .map_err(CommandError::from_managed_runtime)?;
    dsh_surface::reload_surface(&app, &surface_state, binding, &request)
        .map_err(CommandError::from_dsh_surface)
}

#[tauri::command]
pub async fn unmount_dsh_surface(
    app: AppHandle,
    surface_state: State<'_, DshSurfaceState>,
    request: DshSurfaceUnmountRequest,
) -> Result<DshSurfaceStatus, CommandError> {
    if !request.is_valid() {
        return Err(CommandError::malformed_dsh_surface_lifecycle_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    if !environment.is_managed() {
        return Err(CommandError::from_managed_runtime(
            ManagedRuntimeError::NotManaged,
        ));
    }
    dsh_surface::unmount_surface(&app, &surface_state, &request)
        .map_err(CommandError::from_dsh_surface)
}

#[tauri::command]
pub async fn get_managed_runtime_status(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    notification_state: State<'_, crate::notification::NotificationService>,
    usage_state: State<'_, crate::usage::UsageService>,
    request: ManagedRuntimeStatusRequest,
) -> Result<ManagedRuntimeReport, CommandError> {
    if request.schema_version() != 1 || !is_valid_id(request.environment_id()) {
        return Err(CommandError::malformed_managed_runtime_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .cloned()
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;

    // The status path may run a bounded auto-restart daemon-side (backoff
    // sleep plus a readiness poll up to START_TIMEOUT); run it off the main
    // thread so a crash-looping backend never freezes the Shell UI.
    let report = tauri::async_runtime::spawn_blocking(move || {
        managed_runtime::get_managed_runtime_status(connector.as_ref(), &environment)
    })
    .await
    .map_err(|_| CommandError::unavailable("Managed status task is unavailable.", true))?
    .map_err(CommandError::from_managed_runtime)?;

    // M3-A notification wiring (IF-NOTIFICATION, AC-NOT-002): emit a
    // runtime_changed notification exactly once per observed transition into
    // healthy/crashed/safe_stop/stopped and audit it. The Supervisor state
    // machine is untouched; a failing notification never fails the status read.
    if let Ok(path) = notification::audit_path(&app) {
        let _ = notification::maybe_emit_runtime_change(
            &app,
            &notification_state,
            &path,
            request.environment_id(),
            report.runtime_state(),
        );
    }
    // M3-C usage wiring (IF-USAGE, AC-USG-001/002): the status read path
    // also feeds the local usage ledger — healthy opens a runtime session
    // timer, stopped/crashed/safe_stop closes it and records the elapsed
    // period as an estimate. The Supervisor state machine is untouched; a
    // failing usage write never fails the status read.
    if let Ok(path) = usage::records_path(&app) {
        let _ = usage::observe_runtime_state(
            &usage_state,
            &path,
            request.environment_id(),
            report.runtime_state(),
        );
    }
    Ok(report)
}

#[tauri::command]
pub async fn get_diagnostics(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    surface_state: State<'_, DshSurfaceState>,
    request: DiagnosticsRequest,
) -> Result<DiagnosticsReport, CommandError> {
    if !request.is_valid() {
        return Err(CommandError::malformed_diagnostics_request());
    }
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;
    let surface = surface_state.inner().clone();
    let environment_id = request.environment_id().to_string();

    // Diagnostics reads the daemon supervisor (which may auto-restart) and
    // the Surface record; keep the read off the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        diagnostics::collect(&app, connector.as_ref(), &surface, &environment_id)
    })
    .await
    .map_err(|_| CommandError::unavailable("Diagnostics task is unavailable.", true))?
    .map_err(CommandError::from_diagnostics)
}

#[tauri::command]
pub async fn stop_managed_environment(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    surface_state: State<'_, DshSurfaceState>,
    request: ManagedRuntimeStopRequest,
) -> Result<ManagedRuntimeReport, CommandError> {
    if request.schema_version() != 1
        || !is_valid_id(request.environment_id())
        || request.expected_generation() == 0
    {
        return Err(CommandError::malformed_managed_runtime_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .cloned()
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    let expected_generation = request.expected_generation();
    dsh_surface::force_unmount(
        &app,
        &surface_state,
        request.environment_id(),
        expected_generation,
    )
    .map_err(CommandError::from_dsh_surface)?;
    let connector = daemon
        .connector()
        .ok_or_else(|| CommandError::unavailable("The daemon is not connected.", true))?;

    tauri::async_runtime::spawn_blocking(move || {
        managed_runtime::stop_managed_environment(
            connector.as_ref(),
            &environment,
            expected_generation,
        )
    })
    .await
    .map_err(|_| CommandError::unavailable("Managed stop task is unavailable.", true))?
    .map_err(CommandError::from_managed_runtime)
}

#[tauri::command]
pub fn validate_environment(environment: DshEnvironment) -> EnvironmentValidation {
    validate_environment_value(environment)
}

pub(crate) fn validate_environment_value(environment: DshEnvironment) -> EnvironmentValidation {
    let mut issues = Vec::new();

    if environment.schema_version != 1 {
        issues.push(issue(
            "schemaVersion",
            "UNSUPPORTED_VERSION",
            "Only schemaVersion 1 is supported.",
        ));
    }
    if !is_valid_id(&environment.id) {
        issues.push(issue(
            "id",
            "MALFORMED_VALUE",
            "Use 2-64 lowercase letters, digits, or hyphens.",
        ));
    }
    if environment.label.trim().is_empty() || environment.label.chars().count() > 128 {
        issues.push(issue(
            "label",
            "MALFORMED_VALUE",
            "Label must contain 1-128 characters.",
        ));
    }
    if environment.harness.path.trim().is_empty() {
        issues.push(issue(
            "harness.path",
            "UNAVAILABLE",
            "Select an existing DSH launch source.",
        ));
    }
    if environment.dsh_home.trim().is_empty() {
        issues.push(issue("dshHome", "MALFORMED_VALUE", "DSH_HOME is required."));
    }
    if environment.profile.trim().is_empty() {
        issues.push(issue("profile", "MALFORMED_VALUE", "Profile is required."));
    }
    if environment.endpoint.host != "127.0.0.1" {
        issues.push(issue(
            "endpoint.host",
            "UNAUTHORIZED",
            "Only the loopback host 127.0.0.1 is allowed.",
        ));
    }
    if !is_valid_port(&environment.endpoint.port) {
        issues.push(issue(
            "endpoint.port",
            "MALFORMED_VALUE",
            "Port must be auto or an integer from 1024 to 65535.",
        ));
    }
    if environment.harness.args.len() > 64 {
        issues.push(issue(
            "harness.args",
            "MALFORMED_VALUE",
            "At most 64 extra arguments are allowed.",
        ));
    }
    if environment
        .harness
        .args
        .iter()
        .any(|argument| is_reserved_argument(argument))
    {
        issues.push(issue(
            "harness.args",
            "UNAUTHORIZED",
            "Host, port, trusted-host, and browser-open policy are Supervisor-owned.",
        ));
    }
    if let Some(node_path) = environment.node_path.as_deref() {
        if node_path.trim().is_empty() || !PathBuf::from(node_path).is_absolute() {
            issues.push(issue(
                "nodePath",
                "MALFORMED_VALUE",
                "Node path must be an absolute executable path.",
            ));
        } else if environment.harness.mode != HarnessMode::Repository
            || environment.ownership != Ownership::Managed
        {
            issues.push(issue(
                "nodePath",
                "UNAUTHORIZED",
                "Node path is only allowed for a Managed prebuilt source checkout.",
            ));
        }
    }

    let launch_preview = issues
        .is_empty()
        .then(|| build_launch_preview(&environment));
    EnvironmentValidation {
        valid: issues.is_empty(),
        issues,
        launch_preview,
    }
}

fn issue(field: &'static str, code: &'static str, message: &'static str) -> ValidationIssue {
    ValidationIssue {
        field,
        code,
        message,
    }
}

pub(crate) fn is_valid_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (2..=64).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn is_valid_port(port: &EndpointPort) -> bool {
    match port {
        EndpointPort::Named(value) => value == "auto",
        EndpointPort::Fixed(value) => *value >= 1024,
    }
}

fn is_reserved_argument(argument: &str) -> bool {
    RESERVED_ARGUMENTS
        .iter()
        .any(|reserved| argument == *reserved || argument.starts_with(&format!("{reserved}=")))
}

fn build_launch_preview(environment: &DshEnvironment) -> LaunchPreview {
    let source = match environment.harness.mode {
        HarnessMode::Repository => "repository",
        HarnessMode::Executable => "executable",
        HarnessMode::Command => "command",
    };
    let port = match &environment.endpoint.port {
        EndpointPort::Named(_) => "0".to_string(),
        EndpointPort::Fixed(value) => value.to_string(),
    };
    let mut arguments = Vec::new();
    if environment.node_path.is_some() {
        arguments.push(argument("repository-entry", "[prebuilt-entry]"));
    }

    if matches!(environment.ownership, Ownership::Managed) {
        if environment.profile != "default" {
            arguments.extend([
                argument("profile", "--profile"),
                argument("profile", environment.profile.clone()),
            ]);
        }
        arguments.push(argument("command", "web"));
        arguments.extend([
            argument("endpoint-policy", "--host"),
            argument("endpoint-policy", "127.0.0.1"),
            argument("endpoint-policy", "--port"),
            argument("endpoint-policy", port.clone()),
            argument("browser-policy", "--no-open"),
        ]);
        arguments.extend(
            environment
                .harness
                .args
                .iter()
                .map(|_| argument("user-extra", "[redacted]")),
        );
    }

    LaunchPreview {
        source,
        executable: environment
            .node_path
            .clone()
            .unwrap_or_else(|| environment.harness.path.clone()),
        cwd: environment.harness.cwd.clone(),
        ownership: environment.ownership,
        endpoint: format!("http://127.0.0.1:{port}"),
        arguments,
    }
}

fn argument(category: &'static str, display: impl Into<String>) -> LaunchArgumentPreview {
    LaunchArgumentPreview {
        category,
        display: display.into(),
    }
}

// ------------------------- Terminal commands (M3-B, ADR-0015; M6-C4 daemon proxy) -------------------------

#[tauri::command]
pub fn create_terminal(
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    usage_state: State<'_, crate::usage::UsageService>,
    request: crate::terminal::TerminalCreateRequest,
) -> Result<crate::terminal::TerminalReport, crate::terminal::TerminalCommandError> {
    let connector = daemon
        .connector()
        .ok_or_else(crate::terminal::TerminalCommandError::from_daemon_unavailable)?;
    let report = crate::terminal::create_terminal(connector.as_ref(), &request)?;
    // M3-C usage wiring (IF-USAGE, AC-USG-001/002): remember the session
    // start in memory; the ledger entry is written on close. Usage never
    // receives terminal output (AC-USG-001).
    usage::mark_terminal_session_start(&usage_state, report.session_id());
    Ok(report)
}

#[tauri::command]
pub fn write_terminal(
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    request: crate::terminal::TerminalWriteRequest,
) -> Result<(), crate::terminal::TerminalCommandError> {
    let connector = daemon
        .connector()
        .ok_or_else(crate::terminal::TerminalCommandError::from_daemon_unavailable)?;
    crate::terminal::write_terminal(connector.as_ref(), &request)
}

#[tauri::command]
pub fn resize_terminal(
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    request: crate::terminal::TerminalResizeRequest,
) -> Result<crate::terminal::TerminalReport, crate::terminal::TerminalCommandError> {
    let connector = daemon
        .connector()
        .ok_or_else(crate::terminal::TerminalCommandError::from_daemon_unavailable)?;
    crate::terminal::resize_terminal(connector.as_ref(), &request)
}

#[tauri::command]
pub fn close_terminal(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    usage_state: State<'_, crate::usage::UsageService>,
    request: crate::terminal::TerminalSessionRequest,
) -> Result<(), crate::terminal::TerminalCommandError> {
    let connector = daemon
        .connector()
        .ok_or_else(crate::terminal::TerminalCommandError::from_daemon_unavailable)?;
    crate::terminal::close_terminal(connector.as_ref(), &request)?;
    // M3-C usage wiring (IF-USAGE, AC-USG-001/002): record the session
    // duration as an estimate after the PTY is closed. A failing usage
    // write never fails the close.
    if let Ok(path) = usage::records_path(&app) {
        let _ = usage::mark_terminal_session_end(&usage_state, &path, request.session_id());
    }
    Ok(())
}

#[tauri::command]
pub fn status_terminal(
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    request: crate::terminal::TerminalSessionRequest,
) -> Result<crate::terminal::TerminalReport, crate::terminal::TerminalCommandError> {
    let connector = daemon
        .connector()
        .ok_or_else(crate::terminal::TerminalCommandError::from_daemon_unavailable)?;
    crate::terminal::status_terminal(connector.as_ref(), &request)
}

#[tauri::command]
pub fn list_terminals(
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
) -> Vec<crate::terminal::TerminalReport> {
    match daemon.connector() {
        Some(connector) => crate::terminal::list_terminals(connector.as_ref()),
        // Fail closed (empty list): the command contract cannot carry an
        // error and the daemon is the PTY authority since M6-C1.
        None => Vec::new(),
    }
}

// ------------------------- Browser commands (M4-C, ADR-0017; M6-C4 daemon proxy for create/list/close) -------------------------

#[tauri::command]
pub async fn create_browser(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    browser_state: State<'_, crate::browser::BrowserState>,
    request: crate::browser::BrowserCreateRequest,
) -> Result<crate::browser::BrowserReport, crate::browser::BrowserCommandError> {
    let connector = daemon
        .connector()
        .ok_or_else(crate::browser::BrowserCommandError::daemon_unavailable)?;
    crate::browser::create_browser(&app, connector.as_ref(), &browser_state, &request).await
}

#[tauri::command]
pub async fn navigate_browser(
    app: AppHandle,
    browser_state: State<'_, crate::browser::BrowserState>,
    request: crate::browser::BrowserNavigateRequest,
) -> Result<crate::browser::BrowserReport, crate::browser::BrowserCommandError> {
    crate::browser::navigate_browser(&app, &browser_state, &request).await
}

#[tauri::command]
pub async fn snapshot_browser(
    app: AppHandle,
    browser_state: State<'_, crate::browser::BrowserState>,
    request: crate::browser::BrowserSnapshotRequest,
) -> Result<crate::browser::BrowserSnapshotReport, crate::browser::BrowserCommandError> {
    crate::browser::snapshot_browser(&app, &browser_state, &request).await
}

#[tauri::command]
pub async fn close_browser(
    app: AppHandle,
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
    browser_state: State<'_, crate::browser::BrowserState>,
    request: crate::browser::BrowserCloseRequest,
) -> Result<crate::browser::BrowserReport, crate::browser::BrowserCommandError> {
    let connector = daemon
        .connector()
        .ok_or_else(crate::browser::BrowserCommandError::daemon_unavailable)?;
    crate::browser::close_browser(&app, connector.as_ref(), &browser_state, &request).await
}

#[tauri::command]
pub async fn list_browsers(
    daemon: State<'_, crate::daemon_client::DaemonClientState>,
) -> Result<Vec<crate::browser::BrowserReport>, crate::browser::BrowserCommandError> {
    let connector = daemon
        .connector()
        .ok_or_else(crate::browser::BrowserCommandError::daemon_unavailable)?;
    crate::browser::list_browsers(connector.as_ref()).await
}

#[tauri::command]
pub async fn interact_browser(
    app: AppHandle,
    browser_state: State<'_, crate::browser::BrowserState>,
    request: crate::browser::BrowserInteractRequest,
) -> Result<crate::browser::BrowserReport, crate::browser::BrowserCommandError> {
    crate::browser::interact_browser(&app, &browser_state, &request).await
}

#[tauri::command]
pub async fn take_over_browser(
    _app: AppHandle,
    browser_state: State<'_, crate::browser::BrowserState>,
    request: crate::browser::BrowserTakeoverRequest,
) -> Result<crate::browser::BrowserReport, crate::browser::BrowserCommandError> {
    crate::browser::take_over_browser(&browser_state, &request)
}

// ------------------------- Notification commands (M3-A, ADR-0016) -------------------------

#[tauri::command]
pub fn notify_application(
    app: AppHandle,
    notification_state: State<'_, crate::notification::NotificationService>,
    usage_state: State<'_, crate::usage::UsageService>,
    request: crate::notification::NotificationRequest,
) -> Result<crate::notification::NotificationReport, CommandError> {
    if request.schema_version() != 1 {
        return Err(CommandError::malformed_notification_request());
    }
    let path = notification::audit_path(&app).map_err(CommandError::from_notification)?;
    let report = notification::notify(
        &notification_state,
        &path,
        request,
        crate::notification::SOURCE_APP,
    )
    .map_err(CommandError::from_notification)?;

    // M3-C usage wiring (IF-USAGE, ADR-0016): a notification that was
    // actually audited also contributes a local usage record; folded
    // (deduplicated) deliveries never re-audit, so they never re-record.
    // The usage record carries no notification content (AC-USG-001) and is
    // never sent anywhere (AC-USG-002); a failing usage write never fails
    // the notification delivery.
    if !report.deduplicated()
        && let Ok(usage_path) = usage::records_path(&app)
    {
        let _ = usage::record_notification(&usage_state, &usage_path);
    }
    Ok(report)
}

#[tauri::command]
pub fn list_notifications(
    app: AppHandle,
    notification_state: State<'_, crate::notification::NotificationService>,
) -> Result<Vec<crate::notification::NotificationReport>, CommandError> {
    let path = notification::audit_path(&app).map_err(CommandError::from_notification)?;
    notification::list(&notification_state, &path).map_err(CommandError::from_notification)
}

#[tauri::command]
pub fn dismiss_notification(
    notification_state: State<'_, crate::notification::NotificationService>,
    request: crate::notification::NotificationDismissRequest,
) -> Result<(), CommandError> {
    if request.schema_version() != 1 {
        return Err(CommandError::malformed_notification_request());
    }
    notification::dismiss(&notification_state, request.notification_id())
        .map_err(CommandError::from_notification)
}

// ------------------------- Usage commands (M3-C, ADR-0016) -------------------------

#[tauri::command]
pub fn get_usage_snapshot(
    app: AppHandle,
    request: crate::usage::UsageSnapshotRequest,
) -> Result<crate::usage::UsageSnapshot, CommandError> {
    if request.schema_version() != 1 {
        return Err(CommandError::malformed_usage_request());
    }
    let path = usage::records_path(&app).map_err(CommandError::from_usage)?;
    let dsh_home = active_environment_dsh_home(&app);
    usage::snapshot_with_dsh(&path, dsh_home.as_deref(), request.since_unix_ms())
        .map_err(CommandError::from_usage)
}

/// dshHome of the currently active environment, if any. The cost-meter
/// ledger import follows the active environment (its dialogue usage is what
/// the Shell shows).
fn active_environment_dsh_home(app: &AppHandle) -> Option<std::path::PathBuf> {
    let catalog = environment_store::load_catalog(&catalog_path(app).ok()?).ok()?;
    Some(std::path::PathBuf::from(
        catalog.active_environment()?.dsh_home(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment() -> DshEnvironment {
        DshEnvironment {
            schema_version: 1,
            id: "local-dev".into(),
            label: "Local DSH".into(),
            harness: HarnessSource {
                mode: HarnessMode::Executable,
                path: "dsh".into(),
                cwd: None,
                args: vec![],
            },
            dsh_home: "C:/Users/example/.dsh".into(),
            profile: "default".into(),
            node_path: None,
            endpoint: Endpoint {
                host: "127.0.0.1".into(),
                port: EndpointPort::Named("auto".into()),
            },
            ownership: Ownership::Managed,
            policy: None,
        }
    }

    #[test]
    fn managed_preview_owns_loopback_and_browser_policy() {
        let result = validate_environment_value(environment());
        assert!(result.valid);
        let preview = result.launch_preview.expect("preview");
        let displays: Vec<_> = preview
            .arguments
            .iter()
            .map(|arg| arg.display.as_str())
            .collect();
        assert!(displays.windows(2).any(|pair| pair == ["--port", "0"]));
        assert!(displays.contains(&"--no-open"));
    }

    #[test]
    fn reserved_arguments_are_rejected() {
        let mut value = environment();
        value.harness.args = vec!["--host=0.0.0.0".into()];
        let result = validate_environment_value(value);
        assert!(!result.valid);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.code == "UNAUTHORIZED")
        );
    }

    #[test]
    fn attached_preview_has_no_lifecycle_arguments() {
        let mut value = environment();
        value.ownership = Ownership::Attached;
        let result = validate_environment_value(value);
        assert!(result.valid);
        assert!(result.launch_preview.expect("preview").arguments.is_empty());
    }

    fn absolute_path() -> String {
        if cfg!(windows) {
            "C:/Program Files/nodejs/node.exe".to_string()
        } else {
            "/usr/bin/node".to_string()
        }
    }

    #[test]
    fn node_path_is_limited_to_managed_repository_recipe() {
        let mut repository = environment();
        repository.harness.mode = HarnessMode::Repository;
        repository.harness.path = format!("{}/apps/cli/lib/bin.js", absolute_path());
        repository.node_path = Some(absolute_path());
        let result = validate_environment_value(repository.clone());
        assert!(result.valid);
        let preview = result.launch_preview.expect("preview");
        assert_eq!(preview.executable, absolute_path());
        assert_eq!(preview.arguments[0].display, "[prebuilt-entry]");

        repository.harness.mode = HarnessMode::Executable;
        let result = validate_environment_value(repository.clone());
        assert!(!result.valid);
        assert!(result.issues.iter().any(|issue| issue.field == "nodePath"));

        repository.harness.mode = HarnessMode::Repository;
        repository.ownership = Ownership::Attached;
        let result = validate_environment_value(repository);
        assert!(!result.valid);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.code == "UNAUTHORIZED")
        );
    }
}
