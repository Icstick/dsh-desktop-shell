use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::mpsc;
#[cfg(windows)]
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Url};

#[cfg(windows)]
use tauri::WebviewUrl;
#[cfg(windows)]
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
#[cfg(windows)]
use webview2_com::{
    Microsoft::Web::WebView2::Win32::*, NavigationCompletedEventHandler,
    PermissionRequestedEventHandler,
};
#[cfg(windows)]
use windows_core::Interface;

use crate::managed_runtime::VerifiedSurfaceBinding;

const SCHEMA_VERSION: u8 = 1;
pub(crate) const SURFACE_LABEL: &str = "dsh-surface";
const SHELL_WINDOW_LABEL: &str = "shell";
#[cfg(windows)]
const NATIVE_HOOK_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshSurfaceMountRequest {
    schema_version: u8,
    environment_id: String,
    expected_generation: u64,
    bounds: DshSurfaceBounds,
    visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshSurfaceStatusRequest {
    schema_version: u8,
    environment_id: String,
    expected_generation: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshSurfaceLayoutRequest {
    schema_version: u8,
    environment_id: String,
    expected_generation: u64,
    bounds: DshSurfaceBounds,
    visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshSurfaceReloadRequest {
    schema_version: u8,
    environment_id: String,
    expected_generation: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshSurfaceUnmountRequest {
    schema_version: u8,
    environment_id: String,
    expected_generation: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshSurfaceBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl DshSurfaceMountRequest {
    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }

    pub(crate) fn expected_generation(&self) -> u64 {
        self.expected_generation
    }

    pub(crate) fn is_valid(&self) -> bool {
        valid_common(
            self.schema_version,
            &self.environment_id,
            self.expected_generation,
        ) && self.bounds.is_valid()
    }
}

impl DshSurfaceStatusRequest {
    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }

    pub(crate) fn expected_generation(&self) -> u64 {
        self.expected_generation
    }

    pub(crate) fn is_valid(&self) -> bool {
        valid_common(
            self.schema_version,
            &self.environment_id,
            self.expected_generation,
        )
    }
}

impl DshSurfaceLayoutRequest {
    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }

    pub(crate) fn expected_generation(&self) -> u64 {
        self.expected_generation
    }

    pub(crate) fn is_valid(&self) -> bool {
        valid_common(
            self.schema_version,
            &self.environment_id,
            self.expected_generation,
        ) && self.bounds.is_valid()
    }
}

impl DshSurfaceReloadRequest {
    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }

    pub(crate) fn expected_generation(&self) -> u64 {
        self.expected_generation
    }

    pub(crate) fn is_valid(&self) -> bool {
        valid_common(
            self.schema_version,
            &self.environment_id,
            self.expected_generation,
        )
    }
}

impl DshSurfaceUnmountRequest {
    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }

    pub(crate) fn expected_generation(&self) -> u64 {
        self.expected_generation
    }

    pub(crate) fn is_valid(&self) -> bool {
        valid_common(
            self.schema_version,
            &self.environment_id,
            self.expected_generation,
        )
    }
}

impl DshSurfaceBounds {
    fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f64::is_finite)
            && (0.0..=32768.0).contains(&self.x)
            && (0.0..=32768.0).contains(&self.y)
            && (320.0..=16384.0).contains(&self.width)
            && (240.0..=16384.0).contains(&self.height)
    }
}

fn valid_common(schema_version: u8, environment_id: &str, expected_generation: u64) -> bool {
    schema_version == SCHEMA_VERSION
        && crate::commands::is_valid_id(environment_id)
        && expected_generation > 0
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshSurfaceStatus {
    schema_version: u8,
    environment_id: String,
    generation: u64,
    surface_label: &'static str,
    state: SurfaceLifecycleState,
    platform: SurfacePlatform,
    verified_origin: VerifiedOrigin,
    bounds: Option<DshSurfaceBounds>,
    visible: bool,
    policy: SurfacePolicy,
    error: Option<SurfaceErrorReport>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SurfaceLifecycleState {
    Unmounted,
    Mounting,
    Loading,
    Ready,
    Hidden,
    Error,
    Stale,
    UnsupportedPlatform,
}

impl SurfaceLifecycleState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unmounted => "unmounted",
            Self::Mounting => "mounting",
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Hidden => "hidden",
            Self::Error => "error",
            Self::Stale => "stale",
            Self::UnsupportedPlatform => "unsupported_platform",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SurfacePlatform {
    Windows,
    Macos,
    Linux,
    Other,
}

impl SurfacePlatform {
    fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

impl SurfacePlatform {
    fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
struct VerifiedOrigin {
    scheme: &'static str,
    host: &'static str,
    port: u16,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct SurfacePolicy {
    same_origin_navigation: &'static str,
    cross_origin_navigation: &'static str,
    new_window: &'static str,
    downloads: &'static str,
    page_permissions: &'static str,
    privileged_ipc: &'static str,
    dom_injection: &'static str,
    automatic_external_open: bool,
}

impl Default for SurfacePolicy {
    fn default() -> Self {
        Self {
            same_origin_navigation: "allow",
            cross_origin_navigation: "deny",
            new_window: "deny",
            downloads: "deny",
            page_permissions: "deny",
            privileged_ipc: "denied",
            dom_injection: "denied",
            automatic_external_open: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct SurfaceErrorReport {
    code: &'static str,
    reason: &'static str,
    message: &'static str,
}

#[derive(Debug, Clone)]
struct SurfaceRecord {
    environment_id: String,
    generation: u64,
    port: u16,
    state: SurfaceLifecycleState,
    bounds: Option<DshSurfaceBounds>,
    visible: bool,
    error: Option<SurfaceErrorReport>,
}

impl SurfaceRecord {
    fn report(&self) -> DshSurfaceStatus {
        DshSurfaceStatus {
            schema_version: SCHEMA_VERSION,
            environment_id: self.environment_id.clone(),
            generation: self.generation,
            surface_label: SURFACE_LABEL,
            state: self.state,
            platform: SurfacePlatform::current(),
            verified_origin: VerifiedOrigin {
                scheme: "http",
                host: "127.0.0.1",
                port: self.port,
            },
            bounds: self.bounds,
            visible: self.visible,
            policy: SurfacePolicy::default(),
            error: self.error.clone(),
        }
    }
}

#[derive(Clone, Default)]
pub struct DshSurfaceState {
    inner: Arc<Mutex<Option<SurfaceRecord>>>,
}

impl DshSurfaceState {
    fn lock(&self) -> Result<MutexGuard<'_, Option<SurfaceRecord>>, DshSurfaceError> {
        self.inner
            .lock()
            .map_err(|_| DshSurfaceError::StateUnavailable)
    }

    fn replace(&self, record: SurfaceRecord) -> Result<DshSurfaceStatus, DshSurfaceError> {
        let report = record.report();
        *self.lock()? = Some(record);
        Ok(report)
    }

    fn transition(
        &self,
        environment_id: &str,
        generation: u64,
        state: SurfaceLifecycleState,
        error: Option<SurfaceErrorReport>,
    ) {
        if let Ok(mut current) = self.inner.lock()
            && let Some(record) = current.as_mut()
            && record.environment_id == environment_id
            && record.generation == generation
        {
            record.state = if state == SurfaceLifecycleState::Ready && !record.visible {
                SurfaceLifecycleState::Hidden
            } else {
                state
            };
            record.error = error;
        }
    }

    fn current(&self) -> Result<Option<SurfaceRecord>, DshSurfaceError> {
        Ok(self.lock()?.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DshSurfaceError {
    MalformedRequest,
    StaleGeneration,
    SurfaceUnavailable,
    StateUnavailable,
}

/// Construct the fail-closed record returned on targets where the native
/// Surface is not enabled. Shared by the #[cfg(not(windows))] branch of
/// mount_surface and unit tests so the platform gate and its serialized
/// report stay single-source.
#[cfg_attr(not(test), allow(dead_code))]
fn unsupported_platform_record(environment_id: &str, generation: u64, port: u16) -> SurfaceRecord {
    SurfaceRecord {
        environment_id: environment_id.to_string(),
        generation,
        port,
        state: SurfaceLifecycleState::UnsupportedPlatform,
        bounds: None,
        visible: false,
        error: Some(SurfaceErrorReport {
            code: "UNAVAILABLE",
            reason: "unsupported_platform",
            message: "Native DSH Surface is not enabled on this platform.",
        }),
    }
}

pub(crate) fn mount_surface(
    app: &AppHandle,
    state: &DshSurfaceState,
    binding: VerifiedSurfaceBinding,
    request: &DshSurfaceMountRequest,
) -> Result<DshSurfaceStatus, DshSurfaceError> {
    if !request.is_valid() || request.expected_generation != binding.generation() {
        return Err(DshSurfaceError::MalformedRequest);
    }

    if let Some(current) = state.current()? {
        if current.environment_id == request.environment_id
            && current.generation == request.expected_generation
            && current.port == binding.port()
            && app.get_webview(SURFACE_LABEL).is_some()
        {
            return update_existing_layout(app, state, request.bounds, request.visible);
        }
        close_existing(app)?;
    } else if app.get_webview(SURFACE_LABEL).is_some() {
        close_existing(app)?;
    }

    let initial = SurfaceRecord {
        environment_id: request.environment_id.clone(),
        generation: request.expected_generation,
        port: binding.port(),
        state: SurfaceLifecycleState::Mounting,
        bounds: Some(request.bounds),
        visible: request.visible,
        error: None,
    };
    state.replace(initial)?;

    #[cfg(not(windows))]
    {
        let _ = app;
        return state.replace(unsupported_platform_record(
            &request.environment_id,
            request.expected_generation,
            binding.port(),
        ));
    }

    #[cfg(windows)]
    {
        mount_windows_surface(app, state, binding, request)
    }
}

#[cfg(windows)]
fn mount_windows_surface(
    app: &AppHandle,
    state: &DshSurfaceState,
    binding: VerifiedSurfaceBinding,
    request: &DshSurfaceMountRequest,
) -> Result<DshSurfaceStatus, DshSurfaceError> {
    let shell = app
        .get_window(SHELL_WINDOW_LABEL)
        .ok_or(DshSurfaceError::SurfaceUnavailable)?;
    let port = binding.port();
    let environment_id = request.environment_id.clone();
    let page_state = state.clone();
    let page_environment_id = environment_id.clone();
    let generation = request.expected_generation;
    let blocked_navigation = Arc::new(AtomicBool::new(false));
    let navigation_gate = blocked_navigation.clone();

    let builder = WebviewBuilder::new(
        SURFACE_LABEL,
        WebviewUrl::External(Url::parse("about:blank").expect("valid bootstrap URL")),
    )
    .focused(false)
    .zoom_hotkeys_enabled(false)
    .browser_extensions_enabled(false)
    .general_autofill_enabled(false)
    .devtools(false)
    .on_navigation(move |url| {
        let allowed = is_bootstrap_url(url) || is_exact_surface_origin(url, port);
        navigation_gate.store(!allowed, Ordering::SeqCst);
        allowed
    })
    .on_new_window(|_, _| NewWindowResponse::Deny)
    .on_download(|_, _| false)
    .on_page_load(move |_, payload| {
        if !is_exact_surface_origin(payload.url(), port) {
            return;
        }
        let next = match payload.event() {
            PageLoadEvent::Started => SurfaceLifecycleState::Loading,
            PageLoadEvent::Finished => SurfaceLifecycleState::Ready,
        };
        page_state.transition(&page_environment_id, generation, next, None);
    });

    let webview = shell
        .add_child(
            builder,
            LogicalPosition::new(request.bounds.x, request.bounds.y),
            LogicalSize::new(request.bounds.width, request.bounds.height),
        )
        .map_err(|_| {
            surface_operation_failed(state, &environment_id, generation, "surface_create_failed")
        })?;

    if !request.visible {
        webview.hide().map_err(|_| {
            // Hide failure must not leave an orphaned child WebView behind;
            // close it so a later mount starts from a clean surface.
            let _ = webview.close();
            surface_operation_failed(
                state,
                &environment_id,
                generation,
                "surface_operation_failed",
            )
        })?;
    }
    install_windows_deny_hooks(
        &webview,
        state.clone(),
        environment_id.clone(),
        generation,
        blocked_navigation,
    )
    .map_err(|_| {
        let _ = webview.close();
        surface_operation_failed(state, &environment_id, generation, "surface_create_failed")
    })?;

    state.transition(
        &environment_id,
        generation,
        SurfaceLifecycleState::Loading,
        None,
    );
    webview.navigate(binding.url()).map_err(|_| {
        let _ = webview.close();
        surface_operation_failed(
            state,
            &environment_id,
            generation,
            "surface_operation_failed",
        )
    })?;

    state
        .current()?
        .map(|record| record.report())
        .ok_or(DshSurfaceError::StateUnavailable)
}

#[cfg(windows)]
// Deny-order invariant (ADR-0011/0012, AC-WEB-006): all WebView2 deny
// hooks (permission, autofill, password save, navigation, popup, download)
// are installed BEFORE any remote document is loaded; the child starts at
// about:blank and only navigates to the backend-owned bootstrap URL.
fn install_windows_deny_hooks(
    webview: &tauri::Webview,
    state: DshSurfaceState,
    environment_id: String,
    generation: u64,
    blocked_navigation: Arc<AtomicBool>,
) -> Result<(), DshSurfaceError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    webview
        .with_webview(move |platform| {
            let result = (|| -> Result<(), ()> {
                let controller = platform.controller();
                let native = unsafe { controller.CoreWebView2().map_err(|_| ())? };
                let native13: ICoreWebView2_13 = native.cast().map_err(|_| ())?;
                let profile = unsafe { native13.Profile().map_err(|_| ())? };
                let profile6: ICoreWebView2Profile6 = profile.cast().map_err(|_| ())?;
                unsafe {
                    profile6
                        .SetIsPasswordAutosaveEnabled(false)
                        .map_err(|_| ())?;
                    profile6
                        .SetIsGeneralAutofillEnabled(false)
                        .map_err(|_| ())?;
                }

                let mut token = 0;
                let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
                    if let Some(args) = args {
                        unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)? };
                    }
                    Ok(())
                }));
                unsafe {
                    native
                        .add_PermissionRequested(&handler, &mut token)
                        .map_err(|_| ())?;
                }

                let navigation_handler =
                    NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                        if let Some(args) = args {
                            let mut succeeded = windows_core::BOOL(0);
                            unsafe { args.IsSuccess(&mut succeeded)? };
                            let policy_denied = blocked_navigation.swap(false, Ordering::SeqCst);
                            if !succeeded.as_bool() && !policy_denied {
                                state.transition(
                                    &environment_id,
                                    generation,
                                    SurfaceLifecycleState::Error,
                                    Some(SurfaceErrorReport {
                                        code: "UNAVAILABLE",
                                        reason: "surface_operation_failed",
                                        message: "Native DSH Surface navigation failed.",
                                    }),
                                );
                            }
                        }
                        Ok(())
                    }));
                let mut navigation_token = 0;
                unsafe {
                    native
                        .add_NavigationCompleted(&navigation_handler, &mut navigation_token)
                        .map_err(|_| ())?;
                }
                Ok(())
            })();
            let _ = sender.send(result);
        })
        .map_err(|_| DshSurfaceError::SurfaceUnavailable)?;

    receiver
        .recv_timeout(NATIVE_HOOK_TIMEOUT)
        .map_err(|_| DshSurfaceError::SurfaceUnavailable)?
        .map_err(|_| DshSurfaceError::SurfaceUnavailable)
}

fn surface_operation_failed(
    state: &DshSurfaceState,
    environment_id: &str,
    generation: u64,
    reason: &'static str,
) -> DshSurfaceError {
    state.transition(
        environment_id,
        generation,
        SurfaceLifecycleState::Error,
        Some(SurfaceErrorReport {
            code: "UNAVAILABLE",
            reason,
            message: "Native DSH Surface operation failed.",
        }),
    );
    DshSurfaceError::SurfaceUnavailable
}

pub(crate) fn get_surface_status(
    state: &DshSurfaceState,
    binding: VerifiedSurfaceBinding,
    request: &DshSurfaceStatusRequest,
) -> Result<DshSurfaceStatus, DshSurfaceError> {
    if !request.is_valid() || request.expected_generation != binding.generation() {
        return Err(DshSurfaceError::MalformedRequest);
    }
    match state.current()? {
        Some(record)
            if record.environment_id == request.environment_id
                && record.generation == request.expected_generation
                && record.port == binding.port() =>
        {
            Ok(record.report())
        }
        Some(_) => Ok(SurfaceRecord {
            environment_id: request.environment_id.clone(),
            generation: request.expected_generation,
            port: binding.port(),
            state: SurfaceLifecycleState::Stale,
            bounds: None,
            visible: false,
            error: Some(SurfaceErrorReport {
                code: "STALE_GENERATION",
                reason: "stale_generation",
                message: "The previous DSH Surface belongs to another generation.",
            }),
        }
        .report()),
        None => Ok(SurfaceRecord {
            environment_id: request.environment_id.clone(),
            generation: request.expected_generation,
            port: binding.port(),
            state: SurfaceLifecycleState::Unmounted,
            bounds: None,
            visible: false,
            error: None,
        }
        .report()),
    }
}

pub(crate) fn update_surface_layout(
    app: &AppHandle,
    state: &DshSurfaceState,
    binding: VerifiedSurfaceBinding,
    request: &DshSurfaceLayoutRequest,
) -> Result<DshSurfaceStatus, DshSurfaceError> {
    if !request.is_valid() || request.expected_generation != binding.generation() {
        return Err(DshSurfaceError::MalformedRequest);
    }
    let current = matching_record(
        state,
        &request.environment_id,
        request.expected_generation,
        binding.port(),
    )?;
    if current.state == SurfaceLifecycleState::UnsupportedPlatform {
        return Ok(current.report());
    }
    update_existing_layout(app, state, request.bounds, request.visible)
}

fn update_existing_layout(
    app: &AppHandle,
    state: &DshSurfaceState,
    bounds: DshSurfaceBounds,
    visible: bool,
) -> Result<DshSurfaceStatus, DshSurfaceError> {
    let webview = app
        .get_webview(SURFACE_LABEL)
        .ok_or(DshSurfaceError::SurfaceUnavailable)?;
    webview
        .set_position(LogicalPosition::new(bounds.x, bounds.y))
        .and_then(|_| webview.set_size(LogicalSize::new(bounds.width, bounds.height)))
        .and_then(|_| {
            if visible {
                webview.show()
            } else {
                webview.hide()
            }
        })
        .map_err(|_| DshSurfaceError::SurfaceUnavailable)?;

    let mut locked = state.lock()?;
    let record = locked.as_mut().ok_or(DshSurfaceError::SurfaceUnavailable)?;
    record.bounds = Some(bounds);
    record.visible = visible;
    record.state = match (visible, record.state) {
        (false, SurfaceLifecycleState::Ready | SurfaceLifecycleState::Hidden) => {
            SurfaceLifecycleState::Hidden
        }
        (true, SurfaceLifecycleState::Hidden) => SurfaceLifecycleState::Ready,
        (_, other) => other,
    };
    Ok(record.report())
}

pub(crate) fn reload_surface(
    app: &AppHandle,
    state: &DshSurfaceState,
    binding: VerifiedSurfaceBinding,
    request: &DshSurfaceReloadRequest,
) -> Result<DshSurfaceStatus, DshSurfaceError> {
    if !request.is_valid() || request.expected_generation != binding.generation() {
        return Err(DshSurfaceError::MalformedRequest);
    }
    matching_record(
        state,
        &request.environment_id,
        request.expected_generation,
        binding.port(),
    )?;
    let webview = app
        .get_webview(SURFACE_LABEL)
        .ok_or(DshSurfaceError::SurfaceUnavailable)?;
    state.transition(
        &request.environment_id,
        request.expected_generation,
        SurfaceLifecycleState::Loading,
        None,
    );
    webview.reload().map_err(|_| {
        surface_operation_failed(
            state,
            &request.environment_id,
            request.expected_generation,
            "surface_operation_failed",
        )
    })?;
    state
        .current()?
        .map(|record| record.report())
        .ok_or(DshSurfaceError::StateUnavailable)
}

pub(crate) fn unmount_surface(
    app: &AppHandle,
    state: &DshSurfaceState,
    request: &DshSurfaceUnmountRequest,
) -> Result<DshSurfaceStatus, DshSurfaceError> {
    if !request.is_valid() {
        return Err(DshSurfaceError::MalformedRequest);
    }
    let current = state
        .current()?
        .ok_or(DshSurfaceError::SurfaceUnavailable)?;
    if current.environment_id != request.environment_id
        || current.generation != request.expected_generation()
    {
        return Err(DshSurfaceError::StaleGeneration);
    }
    force_unmount(
        app,
        state,
        &request.environment_id,
        request.expected_generation(),
    )?;
    state
        .current()?
        .map(|record| record.report())
        .ok_or(DshSurfaceError::StateUnavailable)
}

pub(crate) fn force_unmount(
    app: &AppHandle,
    state: &DshSurfaceState,
    environment_id: &str,
    generation: u64,
) -> Result<(), DshSurfaceError> {
    if let Some(webview) = app.get_webview(SURFACE_LABEL) {
        webview
            .close()
            .map_err(|_| DshSurfaceError::SurfaceUnavailable)?;
    }
    let mut locked = state.lock()?;
    if let Some(record) = locked.as_mut()
        && record.environment_id == environment_id
        && record.generation == generation
    {
        record.state = SurfaceLifecycleState::Unmounted;
        record.bounds = None;
        record.visible = false;
        record.error = None;
    }
    Ok(())
}

fn matching_record(
    state: &DshSurfaceState,
    environment_id: &str,
    generation: u64,
    port: u16,
) -> Result<SurfaceRecord, DshSurfaceError> {
    match state.current()? {
        Some(record)
            if record.environment_id == environment_id
                && record.generation == generation
                && record.port == port =>
        {
            Ok(record)
        }
        Some(_) => Err(DshSurfaceError::StaleGeneration),
        None => Err(DshSurfaceError::SurfaceUnavailable),
    }
}

fn close_existing(app: &AppHandle) -> Result<(), DshSurfaceError> {
    if let Some(webview) = app.get_webview(SURFACE_LABEL) {
        webview
            .close()
            .map_err(|_| DshSurfaceError::SurfaceUnavailable)?;
    }
    Ok(())
}

fn is_bootstrap_url(url: &Url) -> bool {
    url.scheme() == "about" && url.as_str() == "about:blank"
}

fn is_exact_surface_origin(url: &Url, port: u16) -> bool {
    url.scheme() == "http"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str() == Some("127.0.0.1")
        && url.port_or_known_default() == Some(port)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfaceDiagnostics {
    pub(crate) state: &'static str,
    pub(crate) platform: &'static str,
    pub(crate) generation: u64,
    pub(crate) visible: bool,
    pub(crate) error: Option<SurfaceDiagnosticsError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SurfaceDiagnosticsError {
    pub(crate) code: &'static str,
    pub(crate) reason: &'static str,
    pub(crate) message: &'static str,
}

/// Read-only credential-free view of the Surface record for diagnostics.
/// The record never stores a URL, token, or cookie; only lifecycle state,
/// platform, generation, visibility, and a bounded static error report.
pub(crate) fn surface_diagnostics(
    state: &DshSurfaceState,
) -> Result<SurfaceDiagnostics, DshSurfaceError> {
    let record = state.current()?;
    Ok(match record {
        Some(record) => SurfaceDiagnostics {
            state: record.state.as_str(),
            platform: SurfacePlatform::current().as_str(),
            generation: record.generation,
            visible: record.visible,
            error: record.error.as_ref().map(|error| SurfaceDiagnosticsError {
                code: error.code,
                reason: error.reason,
                message: error.message,
            }),
        },
        None => SurfaceDiagnostics {
            state: SurfaceLifecycleState::Unmounted.as_str(),
            platform: SurfacePlatform::current().as_str(),
            generation: 0,
            visible: false,
            error: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_request_rejects_caller_endpoint_and_invalid_bounds() {
        let with_endpoint = serde_json::json!({
            "schemaVersion": 1,
            "environmentId": "managed-local",
            "expectedGeneration": 7,
            "endpoint": "http://127.0.0.1:13579/",
            "bounds": { "x": 0, "y": 0, "width": 800, "height": 600 },
            "visible": true
        });
        assert!(serde_json::from_value::<DshSurfaceMountRequest>(with_endpoint).is_err());

        let invalid: DshSurfaceMountRequest = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "environmentId": "managed-local",
            "expectedGeneration": 7,
            "bounds": { "x": 0, "y": 0, "width": 319, "height": 600 },
            "visible": true
        }))
        .expect("request shape");
        assert!(!invalid.is_valid());
    }

    #[test]
    fn native_navigation_only_allows_bootstrap_or_exact_origin() {
        assert!(is_bootstrap_url(&Url::parse("about:blank").unwrap()));
        assert!(is_exact_surface_origin(
            &Url::parse("http://127.0.0.1:13579/path?q=1#fragment").unwrap(),
            13579
        ));
        for candidate in [
            "https://127.0.0.1:13579/",
            "http://localhost:13579/",
            "http://127.0.0.1:13580/",
            "http://user@127.0.0.1:13579/",
            "https://example.com/",
            "file:///tmp/value",
        ] {
            assert!(
                !is_exact_surface_origin(&Url::parse(candidate).unwrap(), 13579),
                "{candidate}"
            );
        }
    }

    #[test]
    fn unsupported_status_is_explicit_and_unmounted() {
        let record = unsupported_platform_record("managed-local", 7, 13579);
        assert_eq!(record.state, SurfaceLifecycleState::UnsupportedPlatform);
        assert!(record.bounds.is_none());
        assert!(!record.visible);
        let error = record.error.as_ref().expect("error");
        assert_eq!(error.code, "UNAVAILABLE");
        assert_eq!(error.reason, "unsupported_platform");
        assert!(!error.message.is_empty());

        let report = record.report();
        assert_eq!(report.state, SurfaceLifecycleState::UnsupportedPlatform);
        assert_eq!(report.platform, SurfacePlatform::current());
        assert_eq!(report.verified_origin.port, 13579);
        assert!(report.bounds.is_none());
        assert!(!report.visible);
        assert_eq!(report.error.expect("error").reason, "unsupported_platform");
        assert_eq!(report.surface_label, SURFACE_LABEL);
        assert_eq!(report.generation, 7);
        assert_eq!(report.environment_id, "managed-local");
    }

    #[test]
    fn surface_diagnostics_is_credential_free_and_unmounted_by_default() {
        let view = surface_diagnostics(&DshSurfaceState::default()).expect("surface diagnostics");
        assert_eq!(view.state, "unmounted");
        assert_eq!(view.platform, SurfacePlatform::current().as_str());
        assert_eq!(view.generation, 0);
        assert!(!view.visible);
        assert!(view.error.is_none());
        let serialized = serde_json::to_string(&view).expect("serialize diagnostics view");
        assert!(!serialized.contains("http"));
        assert!(!serialized.contains("url"));
        assert!(!serialized.contains("token"));
    }

    #[test]
    fn surface_diagnostics_condenses_an_unsupported_platform_record() {
        let state = DshSurfaceState::default();
        state
            .replace(unsupported_platform_record("managed-local", 7, 13579))
            .expect("replace record");
        let view = surface_diagnostics(&state).expect("surface diagnostics");
        assert_eq!(view.state, "unsupported_platform");
        assert_eq!(view.generation, 7);
        assert!(!view.visible);
        let error = view.error.as_ref().expect("error");
        assert_eq!(error.code, "UNAVAILABLE");
        assert_eq!(error.reason, "unsupported_platform");
        let serialized = serde_json::to_string(&view).expect("serialize diagnostics view");
        assert!(!serialized.contains("13579"));
        assert!(!serialized.contains("http"));
        assert!(!serialized.contains("token"));
    }
}
