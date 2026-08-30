//! Credential-free diagnostics (AC-LOG-001).
//!
//! `collect` condenses Supervisor, DSH Surface, environment catalog, and
//! process state into a single read-only `DiagnosticsReport`. Every field is
//! redacted by construction: the report never carries tokens, cookies, query
//! strings, bootstrap URLs, full URLs, or PIDs. The runtime section is
//! extracted from the public `ManagedRuntimeReport` through a whitelist view,
//! and the endpoint is emitted only when its host is the fixed loopback
//! `127.0.0.1`. See docs/security/LOG_REDACTION.md and
//! specs/runtime/diagnostics-report.schema.json.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::commands;
use crate::dsh_surface::{self, DshSurfaceState, SurfaceDiagnostics};
use crate::environment_store::{self, EnvironmentCatalog};
use crate::managed_runtime::{self, ManagedRuntimeError};

const SCHEMA_VERSION: u8 = 1;
const ENDPOINT_HOST: &str = "127.0.0.1";
const MAX_EVIDENCE: usize = 8;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticsRequest {
    schema_version: u8,
    environment_id: String,
}

impl DiagnosticsRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION && commands::is_valid_id(&self.environment_id)
    }

    pub(crate) fn environment_id(&self) -> &str {
        &self.environment_id
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    schema_version: u8,
    environment_id: String,
    observed_at_unix_ms: u64,
    runtime: RuntimeDiagnostics,
    surface: SurfaceDiagnostics,
    catalog: CatalogDiagnostics,
    process: ProcessDiagnostics,
    evidence: Vec<DiagnosticsEvidence>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeDiagnostics {
    state: String,
    generation: u64,
    readiness: String,
    endpoint: Option<DiagnosticsEndpoint>,
    recovery: Option<DiagnosticsRecovery>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsEndpoint {
    host: &'static str,
    port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsRecovery {
    crash_count: u64,
    budget: u64,
    safe_stop: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogDiagnostics {
    revision: u64,
    active_environment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessDiagnostics {
    retained: bool,
    owned: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsEvidence {
    code: String,
    severity: String,
    message: String,
}

/// Whitelist view of the public `ManagedRuntimeReport`. Deserializing the
/// report JSON into this subset is the redaction boundary: every other report
/// field (instance id, stop disposition, timestamps) and any future field is
/// dropped, and the endpoint only keeps host + port.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeReportView {
    state: String,
    generation: u64,
    readiness: String,
    process_ownership: String,
    endpoint: Option<EndpointView>,
    recovery: Option<RecoveryView>,
    evidence: Vec<EvidenceView>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EndpointView {
    host: String,
    port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryView {
    crash_count: u64,
    budget: u64,
    safe_stop: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceView {
    code: String,
    severity: String,
    message: String,
}

impl From<EvidenceView> for DiagnosticsEvidence {
    fn from(view: EvidenceView) -> Self {
        Self {
            code: view.code,
            severity: view.severity,
            message: view.message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagnosticsError {
    MalformedRequest,
    EnvironmentNotFound,
    NotManaged,
    CatalogUnavailable,
    StateUnavailable,
    ClockUnavailable,
}

pub(crate) fn collect(
    app: &AppHandle,
    connector: &dyn crate::daemon_client::DaemonConnector,
    surface_state: &DshSurfaceState,
    environment_id: &str,
) -> Result<DiagnosticsReport, DiagnosticsError> {
    if !commands::is_valid_id(environment_id) {
        return Err(DiagnosticsError::MalformedRequest);
    }
    let catalog = environment_store::load_catalog(
        &commands::catalog_path(app).map_err(|_| DiagnosticsError::CatalogUnavailable)?,
    )
    .map_err(|_| DiagnosticsError::CatalogUnavailable)?;
    collect_from_catalog(catalog, connector, surface_state, environment_id)
}

fn collect_from_catalog(
    catalog: EnvironmentCatalog,
    connector: &dyn crate::daemon_client::DaemonConnector,
    surface_state: &DshSurfaceState,
    environment_id: &str,
) -> Result<DiagnosticsReport, DiagnosticsError> {
    let environment = catalog
        .environment(environment_id)
        .ok_or(DiagnosticsError::EnvironmentNotFound)?;
    if !environment.is_managed() {
        return Err(DiagnosticsError::NotManaged);
    }

    let runtime_report = managed_runtime::get_managed_runtime_status(connector, environment)
        .map_err(|error| match error {
            ManagedRuntimeError::NotManaged => DiagnosticsError::NotManaged,
            _ => DiagnosticsError::StateUnavailable,
        })?;
    let surface = dsh_surface::surface_diagnostics(surface_state)
        .map_err(|_| DiagnosticsError::StateUnavailable)?;
    let observed_at_unix_ms = unix_ms()?;

    let view: RuntimeReportView = serde_json::from_value(
        serde_json::to_value(&runtime_report).expect("public runtime report is serializable"),
    )
    .expect("public runtime report matches the whitelist view");

    let (runtime, endpoint_redacted) = runtime_section_from_view(view.clone());
    let retained = view.process_ownership == "owned";
    let process = ProcessDiagnostics {
        retained,
        owned: retained,
    };

    let mut evidence = Vec::with_capacity(MAX_EVIDENCE);
    evidence.push(evidence_item(
        "DIAGNOSTICS_COLLECTED",
        "info",
        "Diagnostics snapshot collected from Supervisor, Surface, catalog, and process state.",
    ));
    for item in view.evidence {
        if evidence.len() >= MAX_EVIDENCE {
            break;
        }
        evidence.push(item.into());
    }
    if endpoint_redacted && evidence.len() < MAX_EVIDENCE {
        evidence.push(evidence_item(
            "DIAGNOSTICS_ENDPOINT_REDACTED",
            "warning",
            "The runtime endpoint did not satisfy loopback policy and was redacted.",
        ));
    }
    if let Some(error) = &surface.error
        && evidence.len() < MAX_EVIDENCE
    {
        evidence.push(evidence_item("SURFACE_ERROR", "error", error.message));
    }
    if evidence.len() < MAX_EVIDENCE {
        evidence.push(if process.retained {
            evidence_item(
                "PROCESS_RETAINED",
                "info",
                "The Supervisor retains a Desktop-owned process tree.",
            )
        } else {
            evidence_item(
                "PROCESS_NOT_RETAINED",
                "info",
                "No Managed process tree is retained.",
            )
        });
    }
    if evidence.len() < MAX_EVIDENCE {
        evidence.push(if catalog.active_environment_id() == Some(environment_id) {
            evidence_item(
                "CATALOG_ACTIVE_ENVIRONMENT",
                "info",
                "The diagnosed environment is the active catalog selection.",
            )
        } else {
            evidence_item(
                "CATALOG_INACTIVE_ENVIRONMENT",
                "info",
                "The diagnosed environment is not the active catalog selection.",
            )
        });
    }

    Ok(DiagnosticsReport {
        schema_version: SCHEMA_VERSION,
        environment_id: environment_id.to_string(),
        observed_at_unix_ms,
        runtime,
        surface,
        catalog: CatalogDiagnostics {
            revision: catalog.revision(),
            active_environment_id: catalog.active_environment_id().map(str::to_string),
        },
        process,
        evidence,
    })
}

/// Condenses a whitelisted runtime view into the credential-free runtime
/// section. Returns `(section, endpoint_redacted)`: the endpoint is emitted
/// only when its host is the fixed loopback, and a non-loopback host is
/// dropped fail-closed instead of serialized.
fn runtime_section_from_view(view: RuntimeReportView) -> (RuntimeDiagnostics, bool) {
    let (endpoint, endpoint_redacted) = match view.endpoint {
        Some(endpoint) if endpoint.host == ENDPOINT_HOST => (
            Some(DiagnosticsEndpoint {
                host: ENDPOINT_HOST,
                port: endpoint.port,
            }),
            false,
        ),
        Some(_) => (None, true),
        None => (None, false),
    };
    (
        RuntimeDiagnostics {
            state: view.state,
            generation: view.generation,
            readiness: view.readiness,
            endpoint,
            recovery: view.recovery.map(|recovery| DiagnosticsRecovery {
                crash_count: recovery.crash_count,
                budget: recovery.budget,
                safe_stop: recovery.safe_stop,
            }),
        },
        endpoint_redacted,
    )
}

fn evidence_item(code: &str, severity: &str, message: &str) -> DiagnosticsEvidence {
    DiagnosticsEvidence {
        code: code.to_string(),
        severity: severity.to_string(),
        message: message.to_string(),
    }
}

fn unix_ms() -> Result<u64, DiagnosticsError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DiagnosticsError::ClockUnavailable)?
        .as_millis()
        .try_into()
        .map_err(|_| DiagnosticsError::ClockUnavailable)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::commands::DshEnvironment;
    use crate::daemon_client::tests::MockConnector;
    use crate::dsh_surface::DshSurfaceState;

    static CATALOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    const SECRET_COOKIE: &str = "sessionCookie=super-secret-cookie-42";

    fn managed_environment() -> DshEnvironment {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "managed"
        }))
        .expect("environment fixture")
    }

    fn catalog_with(environment: DshEnvironment) -> EnvironmentCatalog {
        let path = std::env::temp_dir().join(format!(
            "dsh-diag-catalog-{}-{}-{}",
            std::process::id(),
            environment.id(),
            CATALOG_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        let catalog =
            environment_store::save_environment(&path, environment).expect("persist catalog");
        let _ = fs::remove_file(&path);
        catalog
    }

    /// M6-C4: the daemon is the supervisor authority, so the tests feed
    /// the redaction path with canned daemon report wire shapes (the
    /// whitelist view + endpoint redaction is exactly what is under test).
    fn report_fixture(state: &str, generation: u64) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "environmentId": "managed-local",
            "ownership": "managed",
            "state": state,
            "generation": generation,
            "instanceId": null,
            "processOwnership": if state == "stopped" { "none" } else { "owned" },
            "lifecycleMutation": "allowed",
            "readiness": if state == "stopped" { "not_started" } else { "verified" },
            "endpoint": if state == "stopped" {
                serde_json::Value::Null
            } else {
                serde_json::json!({
                    "scheme": "http",
                    "host": "127.0.0.1",
                    "port": 4317,
                    "source": "managed_process_output",
                    "verification": "owned_generation_output_and_tcp",
                })
            },
            "stopDisposition": "not_requested",
            "recovery": null,
            "observedAtUnixMs": 1_787_000_000_000u64,
            "evidence": [
                { "code": "PROCESS_RETAINED", "severity": "info", "message": "Managed DSH process is retained" },
            ],
        })
    }

    fn healthy_connector() -> MockConnector {
        MockConnector::ok(report_fixture("healthy", 7))
    }

    #[test]
    fn malformed_diagnostics_request_is_rejected() {
        let with_unknown_field = serde_json::json!({
            "schemaVersion": 1,
            "environmentId": "managed-local",
            "secret": "http://127.0.0.1:4317/?token=leak"
        });
        assert!(serde_json::from_value::<DiagnosticsRequest>(with_unknown_field).is_err());

        let unsupported_version: DiagnosticsRequest = serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "environmentId": "managed-local"
        }))
        .expect("request shape");
        assert!(!unsupported_version.is_valid());

        let invalid_id: DiagnosticsRequest = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "environmentId": "Not A Valid Id!"
        }))
        .expect("request shape");
        assert!(!invalid_id.is_valid());
    }

    #[test]
    fn unknown_environment_is_rejected() {
        let catalog = catalog_with(managed_environment());
        let error = collect_from_catalog(
            catalog,
            &healthy_connector(),
            &DshSurfaceState::default(),
            "no-such-environment",
        )
        .expect_err("unknown environment");
        assert_eq!(error, DiagnosticsError::EnvironmentNotFound);
    }

    #[test]
    fn attached_environment_is_rejected_as_not_managed() {
        let attached: DshEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "attached-local",
            "label": "Attached DSH",
            "harness": { "mode": "executable", "path": "C:/tools/dsh.exe" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": 4317 },
            "ownership": "attached"
        }))
        .expect("attached fixture");
        let error = collect_from_catalog(
            catalog_with(attached),
            &healthy_connector(),
            &DshSurfaceState::default(),
            "attached-local",
        )
        .expect_err("attached environment");
        assert_eq!(error, DiagnosticsError::NotManaged);
    }

    #[test]
    fn stopped_daemon_report_condenses_to_a_credential_free_report() {
        let catalog = catalog_with(managed_environment());
        let connector = MockConnector::ok(report_fixture("stopped", 0));
        let report = collect_from_catalog(
            catalog,
            &connector,
            &DshSurfaceState::default(),
            "managed-local",
        )
        .expect("diagnostics");
        assert_eq!(report.schema_version, SCHEMA_VERSION);
        assert_eq!(report.environment_id, "managed-local");
        assert_eq!(report.runtime.state, "stopped");
        assert_eq!(report.runtime.generation, 0);
        assert_eq!(report.runtime.readiness, "not_started");
        assert!(report.runtime.endpoint.is_none());
        assert!(report.runtime.recovery.is_none());
        assert_eq!(report.surface.state, "unmounted");
        assert!(!report.surface.visible);
        assert!(report.surface.error.is_none());
        assert_eq!(report.catalog.revision, 1);
        assert_eq!(
            report.catalog.active_environment_id.as_deref(),
            Some("managed-local")
        );
        assert!(!report.process.retained);
        assert!(!report.process.owned);
        assert!(!report.evidence.is_empty());
        assert!(
            report
                .evidence
                .iter()
                .any(|item| item.code == "PROCESS_NOT_RETAINED")
        );
        let serialized = serde_json::to_string(&report).expect("serialize diagnostics");
        assert!(!serialized.contains("C:/Users"));
        assert!(!serialized.contains("dshHome"));
    }

    #[test]
    fn non_loopback_endpoint_is_redacted_fail_closed() {
        let view = RuntimeReportView {
            state: "healthy".to_string(),
            generation: 7,
            readiness: "verified".to_string(),
            process_ownership: "owned".to_string(),
            endpoint: Some(EndpointView {
                host: "0.0.0.0".to_string(),
                port: 4317,
            }),
            recovery: None,
            evidence: Vec::new(),
        };
        let (runtime, endpoint_redacted) = runtime_section_from_view(view);
        assert!(endpoint_redacted);
        assert!(runtime.endpoint.is_none());
    }

    #[test]
    fn loopback_endpoint_keeps_only_host_and_port() {
        let view = RuntimeReportView {
            state: "healthy".to_string(),
            generation: 7,
            readiness: "verified".to_string(),
            process_ownership: "owned".to_string(),
            endpoint: Some(EndpointView {
                host: "127.0.0.1".to_string(),
                port: 4317,
            }),
            recovery: None,
            evidence: Vec::new(),
        };
        let (runtime, endpoint_redacted) = runtime_section_from_view(view);
        assert!(!endpoint_redacted);
        let endpoint = runtime.endpoint.as_ref().expect("loopback endpoint");
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 4317);
        let serialized = serde_json::to_string(&runtime).expect("serialize runtime section");
        assert!(!serialized.contains("scheme"));
        assert!(!serialized.contains("source"));
        assert!(!serialized.contains("verification"));
    }

    #[test]
    fn ac_log_001_collect_redacts_credentials_and_catalog_secrets() {
        // The daemon report never carries the bootstrap token (the shell
        // only sees endpoint host/port); the redaction contract still must
        // keep catalog side-channel secrets out of the report.
        let catalog_environment: DshEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed DSH",
            "harness": {
                "mode": "executable",
                "path": "dsh",
                "args": ["--extra=secret-arg"],
            },
            "dshHome": format!("C:/Users/example/.dsh;{SECRET_COOKIE}"),
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "managed"
        }))
        .expect("catalog fixture");
        let catalog = catalog_with(catalog_environment);

        let diagnostics = collect_from_catalog(
            catalog,
            &healthy_connector(),
            &DshSurfaceState::default(),
            "managed-local",
        )
        .expect("diagnostics");

        assert_eq!(diagnostics.runtime.state, "healthy");
        assert_eq!(diagnostics.runtime.generation, 7);
        assert_eq!(diagnostics.runtime.readiness, "verified");
        assert!(diagnostics.process.retained);
        assert!(diagnostics.process.owned);

        let serialized = serde_json::to_string(&diagnostics).expect("serialize diagnostics");
        for secret in [
            SECRET_COOKIE,
            "sessionCookie",
            "--extra=secret-arg",
            "token=",
            "http://",
            "C:/Users/example/.dsh;",
        ] {
            assert!(
                !serialized.contains(secret),
                "diagnostics leaked {secret:?}: {serialized}"
            );
        }
        assert!(!serialized.contains("bootstrap"));
        assert!(!serialized.contains("dshHome"));

        // The endpoint exposes only the fixed loopback host and port.
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("diagnostics JSON");
        let endpoint = value["runtime"]["endpoint"]
            .as_object()
            .expect("endpoint object");
        let mut keys: Vec<_> = endpoint.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["host", "port"]);
        assert_eq!(endpoint["host"], serde_json::json!("127.0.0.1"));
        assert!(endpoint["port"].as_u64().is_some());

        // Evidence messages are bounded static strings.
        assert!(
            diagnostics
                .evidence
                .iter()
                .any(|item| item.code == "PROCESS_RETAINED")
        );
    }

    #[test]
    fn recovery_history_is_condensed_and_credential_free() {
        let view = RuntimeReportView {
            state: "safe_stop".to_string(),
            generation: 3,
            readiness: "failed".to_string(),
            process_ownership: "none".to_string(),
            endpoint: None,
            recovery: Some(RecoveryView {
                crash_count: 3,
                budget: 3,
                safe_stop: true,
            }),
            evidence: Vec::new(),
        };
        let (runtime, endpoint_redacted) = runtime_section_from_view(view);
        assert!(!endpoint_redacted);
        let recovery = runtime.recovery.as_ref().expect("recovery");
        assert_eq!(recovery.crash_count, 3);
        assert_eq!(recovery.budget, 3);
        assert!(recovery.safe_stop);
        let serialized = serde_json::to_string(&runtime).expect("serialize runtime section");
        assert!(!serialized.contains("windowStartUnixMs"));
        assert!(!serialized.contains("lastCrashAtUnixMs"));
    }
}
