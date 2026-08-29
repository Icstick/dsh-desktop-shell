use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::attached_health::{
    self, AttachedHealthError, AttachedHealthReport, AttachedHealthRequest,
};
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
    self, ManagedRuntimeError, ManagedRuntimeReport, ManagedRuntimeStartRequest,
    ManagedRuntimeState, ManagedRuntimeStatusRequest, ManagedRuntimeStopRequest,
};

const RESERVED_ARGUMENTS: [&str; 4] = ["--host", "--port", "--no-open", "--trusted-host"];
const CATALOG_FILE_NAME: &str = "environment-catalog-v1.json";
static ERROR_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellSnapshot {
    phase: &'static str,
    runtime_state: &'static str,
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
struct EnvironmentPolicy {
    auto_restart_on_crash: Option<bool>,
    allow_native_adapter: Option<bool>,
}

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
    message: &'static str,
    retryable: bool,
    correlation_id: String,
    issues: Vec<ValidationIssue>,
}

impl DshEnvironment {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn is_attached(&self) -> bool {
        matches!(self.ownership, Ownership::Attached)
    }

    pub(crate) fn is_managed(&self) -> bool {
        matches!(self.ownership, Ownership::Managed)
    }

    pub(crate) fn harness_mode(&self) -> HarnessMode {
        self.harness.mode
    }

    pub(crate) fn harness_path(&self) -> &str {
        &self.harness.path
    }

    pub(crate) fn harness_cwd(&self) -> Option<&str> {
        self.harness.cwd.as_deref()
    }

    pub(crate) fn harness_args(&self) -> &[String] {
        &self.harness.args
    }

    pub(crate) fn dsh_home(&self) -> &str {
        &self.dsh_home
    }

    pub(crate) fn profile(&self) -> &str {
        &self.profile
    }

    pub(crate) fn node_path(&self) -> Option<&str> {
        self.node_path.as_deref()
    }

    pub(crate) fn managed_expected_port(&self) -> Option<u16> {
        match &self.endpoint.port {
            EndpointPort::Fixed(port) => Some(*port),
            EndpointPort::Named(_) => None,
        }
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
            message: "Environment validation failed.",
            retryable: false,
            correlation_id: next_correlation_id(),
            issues,
        }
    }

    fn malformed_discovery() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Harness discovery request is invalid.",
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_dsh_surface_policy_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "DSH Surface policy request is invalid.",
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_dsh_surface_navigation_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "DSH Surface navigation request is invalid.",
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
            message: "Attached health request is invalid.",
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_managed_runtime_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "Managed runtime request is invalid.",
            retryable: false,
            correlation_id: next_correlation_id(),
            issues: Vec::new(),
        }
    }

    fn malformed_dsh_surface_lifecycle_request() -> Self {
        Self {
            code: "MALFORMED_MESSAGE",
            message: "DSH Surface lifecycle request is invalid.",
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
                message: "The DSH Surface request targets a stale generation.",
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
                message: "Managed lifecycle requires a Managed environment.",
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            ManagedRuntimeError::InvalidEnvironment => Self::invalid_environment(Vec::new()),
            ManagedRuntimeError::UnsupportedSource => Self::unavailable(
                "Managed start requires an existing executable or a prebuilt source recipe.",
                false,
            ),
            ManagedRuntimeError::NodeOverrideUnsupported => Self::unavailable(
                "Managed source start requires an absolute existing Node executable.",
                false,
            ),
            ManagedRuntimeError::Conflict => Self {
                code: "CONFLICT",
                message: "Another Managed environment or lifecycle transition is active.",
                retryable: true,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            ManagedRuntimeError::StaleGeneration => Self {
                code: "STALE_GENERATION",
                message: "The Managed lifecycle request targets a stale generation.",
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
            ManagedRuntimeError::SpawnUnavailable
            | ManagedRuntimeError::ProcessTreeUnavailable
            | ManagedRuntimeError::StopFailed
            | ManagedRuntimeError::StateUnavailable
            | ManagedRuntimeError::ClockUnavailable => {
                Self::unavailable("Managed runtime is unavailable.", true)
            }
        }
    }

    fn from_attached_health(error: AttachedHealthError) -> Self {
        match error {
            AttachedHealthError::NotAttached => Self {
                code: "NOT_PROCESS_OWNER",
                message: "Health probe requires an Attached environment.",
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            AttachedHealthError::FixedPortRequired => Self {
                code: "UNAVAILABLE",
                message: "Attached health requires a fixed loopback port.",
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            AttachedHealthError::ClockUnavailable => {
                Self::unavailable("Attached health timestamp is unavailable.", true)
            }
        }
    }

    fn unavailable(message: &'static str, retryable: bool) -> Self {
        Self {
            code: "UNAVAILABLE",
            message,
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
                message: "Environment catalog contains an invalid environment.",
                retryable: false,
                correlation_id: next_correlation_id(),
                issues: Vec::new(),
            },
            StoreError::Capacity => Self {
                code: "CONFLICT",
                message: "Environment catalog capacity has been reached.",
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

fn next_correlation_id() -> String {
    format!(
        "desktop-{}-{}",
        std::process::id(),
        ERROR_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn catalog_path(app: &AppHandle) -> Result<PathBuf, CommandError> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(CATALOG_FILE_NAME))
        .map_err(|_| CommandError::unavailable("Application data directory is unavailable.", true))
}

#[tauri::command]
pub fn get_shell_snapshot(
    app: AppHandle,
    managed_state: State<'_, ManagedRuntimeState>,
) -> Result<ShellSnapshot, CommandError> {
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let active = catalog.active_environment();

    let (runtime_state, generation) = match active {
        Some(environment) if environment.is_managed() => {
            let report = managed_runtime::get_managed_runtime_status(&managed_state, environment)
                .map_err(CommandError::from_managed_runtime)?;
            (report.runtime_state(), report.generation())
        }
        Some(environment) => (environment.runtime_state(), 0),
        None => ("unconfigured", 0),
    };

    Ok(ShellSnapshot {
        phase: "shell-mvp",
        runtime_state,
        environment_id: active.map(|environment| environment.id().to_string()),
        generation,
    })
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

#[tauri::command]
pub fn discover_harnesses(
    request: HarnessDiscoveryRequest,
) -> Result<HarnessDiscoveryReport, CommandError> {
    discovery::discover_harnesses(request).map_err(|error| match error {
        DiscoveryError::MalformedRequest => CommandError::malformed_discovery(),
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
    managed_state: State<'_, ManagedRuntimeState>,
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
    let state = managed_state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        managed_runtime::start_managed_environment(&state, &environment)
    })
    .await
    .map_err(|_| CommandError::unavailable("Managed start task is unavailable.", true))?
    .map_err(CommandError::from_managed_runtime)
}

#[tauri::command]
pub async fn mount_dsh_surface(
    app: AppHandle,
    managed_state: State<'_, ManagedRuntimeState>,
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
    let binding = managed_runtime::verified_surface_binding(
        &managed_state,
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
    managed_state: State<'_, ManagedRuntimeState>,
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
    let binding = managed_runtime::verified_surface_binding(
        &managed_state,
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
    managed_state: State<'_, ManagedRuntimeState>,
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
    let binding = managed_runtime::verified_surface_binding(
        &managed_state,
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
    managed_state: State<'_, ManagedRuntimeState>,
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
    let binding = managed_runtime::verified_surface_binding(
        &managed_state,
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
pub fn get_managed_runtime_status(
    app: AppHandle,
    managed_state: State<'_, ManagedRuntimeState>,
    request: ManagedRuntimeStatusRequest,
) -> Result<ManagedRuntimeReport, CommandError> {
    if request.schema_version() != 1 || !is_valid_id(request.environment_id()) {
        return Err(CommandError::malformed_managed_runtime_request());
    }
    let catalog =
        environment_store::load_catalog(&catalog_path(&app)?).map_err(CommandError::from_store)?;
    let environment = catalog
        .environment(request.environment_id())
        .ok_or_else(|| CommandError::unavailable("Managed environment is unavailable.", false))?;
    managed_runtime::get_managed_runtime_status(&managed_state, environment)
        .map_err(CommandError::from_managed_runtime)
}

#[tauri::command]
pub async fn stop_managed_environment(
    app: AppHandle,
    managed_state: State<'_, ManagedRuntimeState>,
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
    let state = managed_state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        managed_runtime::stop_managed_environment(&state, &environment, expected_generation)
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

    let _policy_is_present = environment
        .policy
        .as_ref()
        .map(|policy| (policy.auto_restart_on_crash, policy.allow_native_adapter));
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

    #[test]
    fn node_path_is_limited_to_managed_repository_recipe() {
        let mut repository = environment();
        repository.harness.mode = HarnessMode::Repository;
        repository.harness.path = "C:/src/deepseek-harness/apps/cli/lib/bin.js".into();
        repository.node_path = Some("C:/Program Files/nodejs/node.exe".into());
        let result = validate_environment_value(repository.clone());
        assert!(result.valid);
        let preview = result.launch_preview.expect("preview");
        assert_eq!(preview.executable, "C:/Program Files/nodejs/node.exe");
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
