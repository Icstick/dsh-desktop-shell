//! Managed environment model and the persisted environment catalog
//! (M1 `environment-catalog-v1.json`, M6-C2 daemon decision).
//!
//! The Shell remains the **writer** of the catalog (it owns environment
//! editing UI); the daemon is a **reader**: every `runtime.*` invocation
//! resolves `environmentId` against the catalog the Shell persists at
//! `%APPDATA%/dev.dsh.desktop-shell/environment-catalog-v1.json` (the
//! daemon data directory, [`crate::CATALOG_FILE_NAME`]). The Shell never
//! ships environment JSON inside envelope requests — the id alone is the
//! wire contract (ADR-0019 decision 3, M6-C2).
//!
//! The serde shape is **byte-identical** to the Shell's
//! `DshEnvironment` (commands.rs) so the same persisted file parses on
//! both sides; validation mirrors `validate_environment_value`
//! (schemaVersion 1, id pattern, loopback-only endpoint, reserved
//! argument policy, nodePath restrictions).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Catalog file inside the daemon/Shell data directory (M1 contract).
pub const CATALOG_FILE_NAME: &str = "environment-catalog-v1.json";

/// Catalog schema version (bump on breaking shape change).
pub const CATALOG_SCHEMA_VERSION: u8 = 1;

/// Hard cap on catalog environments (mirrors the Shell store).
pub const MAX_ENVIRONMENTS: usize = 128;

/// Arguments the Supervisor owns; a catalog environment must not carry
/// them in `harness.args` (policy mirror of the Shell validation).
pub const RESERVED_ARGUMENTS: [&str; 4] = ["--host", "--port", "--no-open", "--trusted-host"];

/// A persisted Managed/Attached environment (M1 `DshEnvironment` shape).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedEnvironment {
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

/// Launch source kind of the harness (M1 semantics).
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HarnessMode {
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

/// Environment policy flags (auto-restart is the one the Supervisor
/// consumes; the rest are Shell-side).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EnvironmentPolicy {
    auto_restart_on_crash: Option<bool>,
    allow_native_adapter: Option<bool>,
}

impl EnvironmentPolicy {
    pub(crate) fn auto_restart_on_crash(&self) -> Option<bool> {
        self.auto_restart_on_crash
    }
}

impl ManagedEnvironment {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_managed(&self) -> bool {
        matches!(self.ownership, Ownership::Managed)
    }

    pub fn harness_mode(&self) -> HarnessMode {
        self.harness.mode
    }

    pub fn harness_path(&self) -> &str {
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

    pub fn profile(&self) -> &str {
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

    pub(crate) fn policy(&self) -> Option<&EnvironmentPolicy> {
        self.policy.as_ref()
    }

    /// Structural validation mirroring the Shell's
    /// `validate_environment_value`: every check that protects the
    /// launch recipe (loopback-only endpoint, reserved arguments,
    /// nodePath restrictions) is enforced here so the daemon can never
    /// launch from a catalog entry the Shell would reject.
    pub fn is_valid(&self) -> bool {
        if self.schema_version != CATALOG_SCHEMA_VERSION {
            return false;
        }
        if !is_valid_id(&self.id) {
            return false;
        }
        if self.label.trim().is_empty() || self.label.chars().count() > 128 {
            return false;
        }
        if self.harness.path.trim().is_empty() {
            return false;
        }
        if self.dsh_home.trim().is_empty() || self.profile.trim().is_empty() {
            return false;
        }
        if self.endpoint.host != "127.0.0.1" {
            return false;
        }
        if !is_valid_port(&self.endpoint.port) {
            return false;
        }
        if self.harness.args.len() > 64
            || self
                .harness
                .args
                .iter()
                .any(|argument| is_reserved_argument(argument))
        {
            return false;
        }
        if let Some(node_path) = self.node_path.as_deref() {
            if node_path.trim().is_empty() || !PathBuf::from(node_path).is_absolute() {
                return false;
            }
            if self.harness.mode != HarnessMode::Repository || !self.is_managed() {
                return false;
            }
        }
        true
    }
}

/// The persisted catalog (M1 `EnvironmentCatalog` shape; read-only on
/// the daemon side).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentCatalog {
    schema_version: u8,
    revision: u64,
    active_environment_id: Option<String>,
    environments: Vec<ManagedEnvironment>,
}

impl Default for EnvironmentCatalog {
    fn default() -> Self {
        Self {
            schema_version: CATALOG_SCHEMA_VERSION,
            revision: 0,
            active_environment_id: None,
            environments: Vec::new(),
        }
    }
}

impl EnvironmentCatalog {
    pub fn environment(&self, environment_id: &str) -> Option<&ManagedEnvironment> {
        self.environments
            .iter()
            .find(|environment| environment.id() == environment_id)
    }

    pub fn environments(&self) -> &[ManagedEnvironment] {
        &self.environments
    }

    pub fn active_environment_id(&self) -> Option<&str> {
        self.active_environment_id.as_deref()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// Why the catalog could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogError {
    /// The file exists but is not a valid v1 catalog (or contains an
    /// invalid environment): fail-closed, never launch from it.
    Corrupt,
    /// The file could not be read (I/O).
    Unavailable,
}

/// Load the persisted catalog. A missing file is an **empty** catalog
/// (the Shell may not have saved an environment yet); a corrupt file is
/// an error (fail-closed).
pub fn load_catalog(path: &Path) -> Result<EnvironmentCatalog, CatalogError> {
    if !path.exists() {
        return Ok(EnvironmentCatalog::default());
    }
    let bytes = fs::read(path).map_err(|_| CatalogError::Unavailable)?;
    let catalog: EnvironmentCatalog =
        serde_json::from_slice(&bytes).map_err(|_| CatalogError::Corrupt)?;
    validate_catalog(&catalog)?;
    Ok(catalog)
}

fn validate_catalog(catalog: &EnvironmentCatalog) -> Result<(), CatalogError> {
    if catalog.schema_version != CATALOG_SCHEMA_VERSION
        || catalog.environments.len() > MAX_ENVIRONMENTS
    {
        return Err(CatalogError::Corrupt);
    }
    let mut ids = HashSet::new();
    for environment in &catalog.environments {
        if !ids.insert(environment.id()) || !environment.is_valid() {
            return Err(CatalogError::Corrupt);
        }
    }
    if catalog
        .active_environment_id
        .as_deref()
        .is_some_and(|active| !ids.contains(active))
    {
        return Err(CatalogError::Corrupt);
    }
    Ok(())
}

/// Id pattern shared with the Shell (`^[a-z][a-z0-9-]{1,63}$`).
pub fn is_valid_id(value: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static CATALOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn environment(id: &str) -> ManagedEnvironment {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": id,
            "label": "Managed DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "managed"
        }))
        .expect("environment fixture")
    }

    fn test_node_path() -> String {
        if cfg!(windows) {
            "C:/Program Files/nodejs/node.exe".to_string()
        } else {
            "/usr/bin/node".to_string()
        }
    }

    #[test]
    fn validation_accepts_the_m1_managed_shapes() {
        assert!(environment("managed-local").is_valid());
        // Fixed loopback port.
        let fixed: ManagedEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "work",
            "endpoint": { "host": "127.0.0.1", "port": 4317 },
            "ownership": "managed",
            "policy": { "autoRestartOnCrash": true }
        }))
        .expect("fixed fixture");
        assert!(fixed.is_valid());
        // Repository recipe with an absolute node path.
        let repository: ManagedEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-src",
            "label": "Source DSH",
            "harness": { "mode": "repository", "path": "D:/dsh/apps/web/dist/main.js", "cwd": "D:/dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "nodePath": test_node_path(),
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "managed"
        }))
        .expect("repository fixture");
        assert!(repository.is_valid());
    }

    #[test]
    fn validation_rejects_unsafe_or_malformed_values() {
        // Attached ownership is not a Managed launch recipe.
        let attached: ManagedEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Attached DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "attached"
        }))
        .expect("attached fixture");
        assert!(!attached.is_managed());

        // Non-loopback host.
        let non_loopback: ManagedEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "managed-local",
            "label": "Managed DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "0.0.0.0", "port": "auto" },
            "ownership": "managed"
        }))
        .expect("non-loopback fixture");
        assert!(!non_loopback.is_valid());

        // Reserved argument smuggling.
        let mut smuggled = environment("managed-local");
        let value_json = serde_json::to_value(&smuggled).expect("serialize");
        let mut object = value_json.as_object().expect("object").clone();
        object.insert(
            "harness".into(),
            serde_json::json!({ "mode": "executable", "path": "dsh", "args": ["--no-open"] }),
        );
        smuggled = serde_json::from_value(serde_json::Value::Object(object)).expect("reparse");
        assert!(!smuggled.is_valid());

        // Bad id shape.
        let bad_id: ManagedEnvironment = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "Not Valid!",
            "label": "Managed DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": "C:/Users/example/.dsh",
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "managed"
        }))
        .expect("bad id fixture");
        assert!(!bad_id.is_valid());
    }

    #[test]
    fn missing_catalog_is_empty_and_corrupt_is_fail_closed() {
        let dir = std::env::temp_dir().join(format!(
            "dsh-mr-catalog-{}-{}",
            std::process::id(),
            CATALOG_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");

        let path = dir.join(CATALOG_FILE_NAME);
        let catalog = load_catalog(&path).expect("missing file loads as empty");
        assert_eq!(catalog.environments().len(), 0);

        fs::write(&path, b"not json").expect("corrupt catalog");
        assert!(matches!(load_catalog(&path), Err(CatalogError::Corrupt)));

        let catalog = EnvironmentCatalog {
            schema_version: CATALOG_SCHEMA_VERSION,
            revision: 1,
            active_environment_id: Some("managed-local".into()),
            environments: vec![environment("managed-local")],
        };
        fs::write(&path, serde_json::to_vec(&catalog).expect("serialize")).expect("valid catalog");
        let loaded = load_catalog(&path).expect("valid catalog loads");
        assert_eq!(loaded.environments().len(), 1);
        assert_eq!(
            loaded
                .environment("managed-local")
                .map(ManagedEnvironment::id),
            Some("managed-local")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalog_rejects_invalid_or_unknown_active_environment() {
        let dir = std::env::temp_dir().join(format!(
            "dsh-mr-catalog-{}-{}",
            std::process::id(),
            CATALOG_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(CATALOG_FILE_NAME);

        let invalid = EnvironmentCatalog {
            schema_version: CATALOG_SCHEMA_VERSION,
            revision: 1,
            active_environment_id: None,
            environments: vec![
                serde_json::from_value(serde_json::json!({
                    "schemaVersion": 1,
                    "id": "managed-local",
                    "label": "Managed DSH",
                    "harness": { "mode": "executable", "path": "dsh" },
                    "dshHome": "",
                    "profile": "default",
                    "endpoint": { "host": "127.0.0.1", "port": "auto" },
                    "ownership": "managed"
                }))
                .expect("invalid fixture"),
            ],
        };
        fs::write(&path, serde_json::to_vec(&invalid).expect("serialize")).expect("write");
        assert!(matches!(load_catalog(&path), Err(CatalogError::Corrupt)));

        let dangling = EnvironmentCatalog {
            schema_version: CATALOG_SCHEMA_VERSION,
            revision: 2,
            active_environment_id: Some("ghost".into()),
            environments: vec![environment("managed-local")],
        };
        fs::write(&path, serde_json::to_vec(&dangling).expect("serialize")).expect("write");
        assert!(matches!(load_catalog(&path), Err(CatalogError::Corrupt)));

        let _ = fs::remove_dir_all(&dir);
    }
}
