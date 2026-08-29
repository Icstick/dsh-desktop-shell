use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use tauri::Url;

use crate::commands::DshEnvironment;

const POLICY_SCHEMA_VERSION: u8 = 1;
const MAX_CANDIDATE_URL_LENGTH: usize = 2048;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshSurfacePolicyRequest {
    schema_version: u8,
    environment_id: String,
}

impl DshSurfacePolicyRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshSurfacePolicy {
    schema_version: u8,
    environment_id: String,
    surface_label: &'static str,
    allowed_origin: AllowedOrigin,
    same_origin_main_frame: &'static str,
    external_http_navigation: &'static str,
    new_window: &'static str,
    downloads: &'static str,
    permissions: &'static str,
    privileged_ipc: &'static str,
    dom_injection: &'static str,
    renderer_patch: &'static str,
    automatic_external_open: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AllowedOrigin {
    scheme: &'static str,
    host: &'static str,
    port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DshSurfaceNavigationRequest {
    schema_version: u8,
    environment_id: String,
    candidate_url: String,
    navigation_kind: NavigationKind,
    user_gesture: bool,
}

impl DshSurfaceNavigationRequest {
    pub(crate) fn schema_version(&self) -> u8 {
        self.schema_version
    }

    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum NavigationKind {
    MainFrame,
    NewWindow,
    Download,
    Permission,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshSurfaceNavigationDecision {
    schema_version: u8,
    environment_id: String,
    navigation_kind: NavigationKind,
    candidate_origin: Option<String>,
    disposition: NavigationDisposition,
    reason: NavigationReason,
    user_gesture_required: bool,
    user_confirmation_required: bool,
    privileged_ipc: &'static str,
    open_automatically: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum NavigationDisposition {
    AllowInSurface,
    DelegateExternal,
    Deny,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum NavigationReason {
    SameOrigin,
    ExternalHttpUserAction,
    ExternalNavigationNoUserGesture,
    InvalidUrl,
    CredentialsForbidden,
    SchemeForbidden,
    LoopbackOriginMismatch,
    NewWindowDenied,
    DownloadDenied,
    PermissionDenied,
    PolicyUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DshSurfacePolicyError {
    FixedEndpointRequired,
}

pub(crate) fn derive_policy(
    environment: &DshEnvironment,
) -> Result<DshSurfacePolicy, DshSurfacePolicyError> {
    let port = environment
        .fixed_loopback_port()
        .ok_or(DshSurfacePolicyError::FixedEndpointRequired)?;

    Ok(DshSurfacePolicy {
        schema_version: POLICY_SCHEMA_VERSION,
        environment_id: environment.id().to_string(),
        surface_label: "dsh-surface",
        allowed_origin: AllowedOrigin {
            scheme: "http",
            host: "127.0.0.1",
            port,
        },
        same_origin_main_frame: "allow",
        external_http_navigation: "delegate_with_user_action",
        new_window: "deny",
        downloads: "deny",
        permissions: "deny",
        privileged_ipc: "denied",
        dom_injection: "denied",
        renderer_patch: "denied",
        automatic_external_open: false,
    })
}

pub(crate) fn evaluate_navigation(
    environment: &DshEnvironment,
    request: &DshSurfaceNavigationRequest,
) -> DshSurfaceNavigationDecision {
    if request.schema_version != POLICY_SCHEMA_VERSION || request.environment_id != environment.id()
    {
        return deny(request, None, NavigationReason::PolicyUnavailable);
    }

    let policy = match derive_policy(environment) {
        Ok(policy) => policy,
        Err(_) => return deny(request, None, NavigationReason::PolicyUnavailable),
    };

    match request.navigation_kind {
        NavigationKind::NewWindow => {
            return deny(
                request,
                safe_origin(&request.candidate_url),
                NavigationReason::NewWindowDenied,
            );
        }
        NavigationKind::Download => {
            return deny(
                request,
                safe_origin(&request.candidate_url),
                NavigationReason::DownloadDenied,
            );
        }
        NavigationKind::Permission => {
            return deny(
                request,
                safe_origin(&request.candidate_url),
                NavigationReason::PermissionDenied,
            );
        }
        NavigationKind::MainFrame => {}
    }

    if request.candidate_url.is_empty() || request.candidate_url.len() > MAX_CANDIDATE_URL_LENGTH {
        return deny(request, None, NavigationReason::InvalidUrl);
    }
    let candidate = match Url::parse(&request.candidate_url) {
        Ok(candidate) => candidate,
        Err(_) => return deny(request, None, NavigationReason::InvalidUrl),
    };
    let candidate_origin = safe_parsed_origin(&candidate);

    if !candidate.username().is_empty() || candidate.password().is_some() {
        return deny(
            request,
            candidate_origin,
            NavigationReason::CredentialsForbidden,
        );
    }
    if !matches!(candidate.scheme(), "http" | "https") {
        return deny(request, candidate_origin, NavigationReason::SchemeForbidden);
    }

    let exact_authority = format!(
        "{}:{}",
        policy.allowed_origin.host, policy.allowed_origin.port
    );
    let same_origin = candidate.scheme() == policy.allowed_origin.scheme
        && candidate.host_str() == Some(policy.allowed_origin.host)
        && candidate.port_or_known_default() == Some(policy.allowed_origin.port)
        && raw_authority(&request.candidate_url) == Some(exact_authority.as_str());
    if same_origin {
        return decision(
            request,
            candidate_origin,
            NavigationDisposition::AllowInSurface,
            NavigationReason::SameOrigin,
            false,
            false,
        );
    }

    if candidate.host_str().is_some_and(is_loopback_host) {
        return deny(
            request,
            candidate_origin,
            NavigationReason::LoopbackOriginMismatch,
        );
    }
    if !request.user_gesture {
        return deny(
            request,
            candidate_origin,
            NavigationReason::ExternalNavigationNoUserGesture,
        );
    }

    decision(
        request,
        candidate_origin,
        NavigationDisposition::DelegateExternal,
        NavigationReason::ExternalHttpUserAction,
        true,
        true,
    )
}

fn deny(
    request: &DshSurfaceNavigationRequest,
    candidate_origin: Option<String>,
    reason: NavigationReason,
) -> DshSurfaceNavigationDecision {
    decision(
        request,
        candidate_origin,
        NavigationDisposition::Deny,
        reason,
        false,
        false,
    )
}

fn decision(
    request: &DshSurfaceNavigationRequest,
    candidate_origin: Option<String>,
    disposition: NavigationDisposition,
    reason: NavigationReason,
    user_gesture_required: bool,
    user_confirmation_required: bool,
) -> DshSurfaceNavigationDecision {
    DshSurfaceNavigationDecision {
        schema_version: POLICY_SCHEMA_VERSION,
        environment_id: request.environment_id.clone(),
        navigation_kind: request.navigation_kind,
        candidate_origin,
        disposition,
        reason,
        user_gesture_required,
        user_confirmation_required,
        privileged_ipc: "denied",
        open_automatically: false,
    }
}

fn safe_origin(candidate_url: &str) -> Option<String> {
    if candidate_url.len() > MAX_CANDIDATE_URL_LENGTH {
        return None;
    }
    Url::parse(candidate_url)
        .ok()
        .and_then(|candidate| safe_parsed_origin(&candidate))
}

fn safe_parsed_origin(candidate: &Url) -> Option<String> {
    if !matches!(candidate.scheme(), "http" | "https") {
        return None;
    }
    let origin = candidate.origin().ascii_serialization();
    (origin != "null" && origin.len() <= 256).then_some(origin)
}

fn raw_authority(candidate_url: &str) -> Option<&str> {
    let (_, remainder) = candidate_url.split_once("://")?;
    let end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    Some(&remainder[..end])
}

fn is_loopback_host(host: &str) -> bool {
    let normalized = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    if normalized.eq_ignore_ascii_case("localhost")
        || normalized.to_ascii_lowercase().ends_with(".localhost")
    {
        return true;
    }
    if normalized
        .parse::<Ipv4Addr>()
        .is_ok_and(|address| address.is_loopback())
    {
        return true;
    }
    normalized
        .parse::<Ipv6Addr>()
        .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(port: serde_json::Value) -> DshEnvironment {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "attached-local",
            "label": "Attached DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": port },
            "ownership": "attached"
        }))
        .expect("environment fixture")
    }

    fn request(
        candidate_url: &str,
        kind: NavigationKind,
        user_gesture: bool,
    ) -> DshSurfaceNavigationRequest {
        DshSurfaceNavigationRequest {
            schema_version: 1,
            environment_id: "attached-local".into(),
            candidate_url: candidate_url.into(),
            navigation_kind: kind,
            user_gesture,
        }
    }

    #[test]
    fn policy_is_derived_only_from_fixed_loopback_environment() {
        let policy = derive_policy(&environment(serde_json::json!(4317))).expect("policy");
        assert_eq!(policy.allowed_origin.host, "127.0.0.1");
        assert_eq!(policy.allowed_origin.port, 4317);
        assert_eq!(policy.privileged_ipc, "denied");
        assert!(!policy.automatic_external_open);
        assert!(matches!(
            derive_policy(&environment(serde_json::json!("auto"))),
            Err(DshSurfacePolicyError::FixedEndpointRequired)
        ));
    }

    #[test]
    fn exact_origin_allows_paths_queries_and_fragments() {
        let decision = evaluate_navigation(
            &environment(serde_json::json!(4317)),
            &request(
                "http://127.0.0.1:4317/chat?token=not-echoed#latest",
                NavigationKind::MainFrame,
                false,
            ),
        );
        assert_eq!(decision.disposition, NavigationDisposition::AllowInSurface);
        assert_eq!(decision.reason, NavigationReason::SameOrigin);
        assert_eq!(
            decision.candidate_origin.as_deref(),
            Some("http://127.0.0.1:4317")
        );
    }

    #[test]
    fn external_http_requires_user_gesture_and_confirmation() {
        let environment = environment(serde_json::json!(4317));
        let denied = evaluate_navigation(
            &environment,
            &request("https://example.com/path", NavigationKind::MainFrame, false),
        );
        assert_eq!(denied.disposition, NavigationDisposition::Deny);
        assert_eq!(
            denied.reason,
            NavigationReason::ExternalNavigationNoUserGesture
        );

        let delegated = evaluate_navigation(
            &environment,
            &request("https://example.com/path", NavigationKind::MainFrame, true),
        );
        assert_eq!(
            delegated.disposition,
            NavigationDisposition::DelegateExternal
        );
        assert!(delegated.user_gesture_required);
        assert!(delegated.user_confirmation_required);
        assert!(!delegated.open_automatically);
    }

    #[test]
    fn alternate_loopback_origins_never_delegate() {
        let environment = environment(serde_json::json!(4317));
        for candidate in [
            "http://127.0.0.1:4318",
            "http://localhost:4317",
            "http://127.1:4317",
            "http://[::1]:4317",
        ] {
            let decision = evaluate_navigation(
                &environment,
                &request(candidate, NavigationKind::MainFrame, true),
            );
            assert_eq!(
                decision.disposition,
                NavigationDisposition::Deny,
                "{candidate}"
            );
            assert_eq!(
                decision.reason,
                NavigationReason::LoopbackOriginMismatch,
                "{candidate}"
            );
        }
    }

    #[test]
    fn credentials_and_non_http_schemes_are_denied_without_secret_echo() {
        let environment = environment(serde_json::json!(4317));
        let credentialed = evaluate_navigation(
            &environment,
            &request(
                "http://user:secret@127.0.0.1:4317/chat",
                NavigationKind::MainFrame,
                true,
            ),
        );
        assert_eq!(credentialed.reason, NavigationReason::CredentialsForbidden);
        assert_eq!(
            credentialed.candidate_origin.as_deref(),
            Some("http://127.0.0.1:4317")
        );

        for candidate in ["file:///tmp/a", "data:text/html,x", "javascript:alert(1)"] {
            let decision = evaluate_navigation(
                &environment,
                &request(candidate, NavigationKind::MainFrame, true),
            );
            assert_eq!(decision.reason, NavigationReason::SchemeForbidden);
            assert!(decision.candidate_origin.is_none());
        }
    }

    #[test]
    fn popup_download_and_permission_are_denied() {
        let environment = environment(serde_json::json!(4317));
        for (kind, reason) in [
            (NavigationKind::NewWindow, NavigationReason::NewWindowDenied),
            (NavigationKind::Download, NavigationReason::DownloadDenied),
            (
                NavigationKind::Permission,
                NavigationReason::PermissionDenied,
            ),
        ] {
            let decision =
                evaluate_navigation(&environment, &request("https://example.com", kind, true));
            assert_eq!(decision.disposition, NavigationDisposition::Deny);
            assert_eq!(decision.reason, reason);
        }
    }
}
