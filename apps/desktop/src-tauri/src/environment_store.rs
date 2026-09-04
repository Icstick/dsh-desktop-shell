use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::fs::File;

use serde::{Deserialize, Serialize};

use crate::commands::{DshEnvironment, validate_environment_value};

const CATALOG_SCHEMA_VERSION: u8 = 1;
const MAX_ENVIRONMENTS: usize = 128;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentCatalog {
    schema_version: u8,
    revision: u64,
    active_environment_id: Option<String>,
    environments: Vec<DshEnvironment>,
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
    pub(crate) fn active_environment(&self) -> Option<&DshEnvironment> {
        let active_id = self.active_environment_id.as_deref()?;
        self.environments
            .iter()
            .find(|environment| environment.id() == active_id)
    }

    pub(crate) fn environment(&self, environment_id: &str) -> Option<&DshEnvironment> {
        self.environments
            .iter()
            .find(|environment| environment.id() == environment_id)
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn active_environment_id(&self) -> Option<&str> {
        self.active_environment_id.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreError {
    Corrupt,
    InvalidEnvironment,
    Capacity,
    NotFound,
    Unavailable,
}

pub(crate) fn load_catalog(path: &Path) -> Result<EnvironmentCatalog, StoreError> {
    if !path.exists() {
        return Ok(EnvironmentCatalog::default());
    }

    let bytes = fs::read(path).map_err(|_| StoreError::Unavailable)?;
    let catalog: EnvironmentCatalog =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?;
    validate_catalog(&catalog)?;
    Ok(catalog)
}

/// Switch the catalog's active environment without touching its
/// definition (B1 multi-profile switching). Unknown ids are rejected
/// (no silent activation of a missing environment).
pub(crate) fn set_active_environment(
    path: &Path,
    environment_id: &str,
) -> Result<EnvironmentCatalog, StoreError> {
    let mut catalog = load_catalog(path)?;
    if !catalog
        .environments
        .iter()
        .any(|environment| environment.id() == environment_id)
    {
        return Err(StoreError::InvalidEnvironment);
    }
    if catalog.active_environment_id.as_deref() == Some(environment_id) {
        return Ok(catalog);
    }
    catalog.active_environment_id = Some(environment_id.to_string());
    catalog.revision = catalog
        .revision
        .checked_add(1)
        .ok_or(StoreError::Capacity)?;
    validate_catalog(&catalog)?;
    write_catalog(path, &catalog)?;
    Ok(catalog)
}

pub(crate) fn save_environment(
    path: &Path,
    environment: DshEnvironment,
) -> Result<EnvironmentCatalog, StoreError> {
    if !validate_environment_value(environment.clone()).is_valid() {
        return Err(StoreError::InvalidEnvironment);
    }

    let mut catalog = load_catalog(path)?;
    let saved_id = environment.id().to_string();
    if let Some(existing) = catalog
        .environments
        .iter_mut()
        .find(|existing| existing.id() == environment.id())
    {
        *existing = environment;
    } else {
        if catalog.environments.len() >= MAX_ENVIRONMENTS {
            return Err(StoreError::Capacity);
        }
        catalog.environments.push(environment);
    }

    catalog
        .environments
        .sort_by(|left, right| left.id().cmp(right.id()));
    catalog.active_environment_id = Some(saved_id);
    catalog.revision = catalog
        .revision
        .checked_add(1)
        .ok_or(StoreError::Capacity)?;

    validate_catalog(&catalog)?;
    write_catalog(path, &catalog)?;
    Ok(catalog)
}

/// Remove one environment from the catalog. Removing the active
/// environment clears the active selection (the Shell returns to the
/// empty surface state). Unknown ids are rejected without touching the
/// catalog (no silent deletion).
pub(crate) fn remove_environment(
    path: &Path,
    environment_id: &str,
) -> Result<EnvironmentCatalog, StoreError> {
    let mut catalog = load_catalog(path)?;
    let original_len = catalog.environments.len();
    catalog
        .environments
        .retain(|environment| environment.id() != environment_id);
    if catalog.environments.len() == original_len {
        return Err(StoreError::NotFound);
    }
    if catalog.active_environment_id.as_deref() == Some(environment_id) {
        catalog.active_environment_id = None;
    }
    catalog.revision = catalog
        .revision
        .checked_add(1)
        .ok_or(StoreError::Capacity)?;
    validate_catalog(&catalog)?;
    write_catalog(path, &catalog)?;
    Ok(catalog)
}

fn validate_catalog(catalog: &EnvironmentCatalog) -> Result<(), StoreError> {
    if catalog.schema_version != CATALOG_SCHEMA_VERSION
        || catalog.environments.len() > MAX_ENVIRONMENTS
    {
        return Err(StoreError::Corrupt);
    }

    let mut ids = HashSet::new();
    for environment in &catalog.environments {
        if !ids.insert(environment.id())
            || !validate_environment_value(environment.clone()).is_valid()
        {
            return Err(StoreError::Corrupt);
        }
    }

    if catalog
        .active_environment_id
        .as_deref()
        .is_some_and(|active| !ids.contains(active))
    {
        return Err(StoreError::Corrupt);
    }
    Ok(())
}

fn write_catalog(path: &Path, catalog: &EnvironmentCatalog) -> Result<(), StoreError> {
    let parent = path.parent().ok_or(StoreError::Unavailable)?;
    fs::create_dir_all(parent).map_err(|_| StoreError::Unavailable)?;
    restrict_directory(parent)?;

    let next = sidecar_path(path, "next")?;
    let backup = sidecar_path(path, "bak")?;
    let payload = serde_json::to_vec_pretty(catalog).map_err(|_| StoreError::Unavailable)?;

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&next)
        .map_err(|_| StoreError::Unavailable)?;
    file.write_all(&payload)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|_| StoreError::Unavailable)?;
    restrict_file(&next)?;
    drop(file);

    let had_previous = path.exists();
    if had_previous {
        fs::copy(path, &backup).map_err(|_| StoreError::Unavailable)?;
        restrict_file(&backup)?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&backup)
            .and_then(|backup_file| backup_file.sync_all())
            .map_err(|_| StoreError::Unavailable)?;
        fs::remove_file(path).map_err(|_| StoreError::Unavailable)?;
    }

    if fs::rename(&next, path).is_err() {
        if had_previous {
            let _ = fs::copy(&backup, path);
            let _ = restrict_file(path);
        }
        return Err(StoreError::Unavailable);
    }
    sync_directory(parent)?;
    Ok(())
}

fn sidecar_path(path: &Path, suffix: &str) -> Result<PathBuf, StoreError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(StoreError::Unavailable)?;
    Ok(path.with_file_name(format!("{name}.{suffix}")))
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| StoreError::Unavailable)
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| StoreError::Unavailable)
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), StoreError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| StoreError::Unavailable)
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "dsh-desktop-catalog-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn environment(id: &str, dsh_home: &Path) -> DshEnvironment {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": id,
            "label": "Local DSH",
            "harness": { "mode": "executable", "path": "dsh" },
            "dshHome": dsh_home.to_string_lossy(),
            "profile": "default",
            "endpoint": { "host": "127.0.0.1", "port": "auto" },
            "ownership": "managed"
        }))
        .expect("environment fixture")
    }

    #[test]
    fn catalog_round_trips_and_increments_revision() {
        let directory = TestDirectory::new();
        let catalog_path = directory.0.join("state/environment-catalog-v1.json");
        let dsh_home = directory.0.join("user-dsh-home");
        fs::create_dir_all(&dsh_home).expect("create dsh home");

        let first = save_environment(&catalog_path, environment("local-dsh", &dsh_home))
            .expect("first save");
        let second = save_environment(&catalog_path, environment("work-dsh", &dsh_home))
            .expect("second save");
        let loaded = load_catalog(&catalog_path).expect("load catalog");

        assert_eq!(first.revision, 1);
        assert_eq!(second.revision, 2);
        assert_eq!(loaded.environments.len(), 2);
        assert_eq!(loaded.active_environment_id.as_deref(), Some("work-dsh"));
        assert_eq!(fs::read_dir(&dsh_home).expect("read dsh home").count(), 0);
    }

    #[test]
    fn previous_revision_is_preserved_as_backup() {
        let directory = TestDirectory::new();
        let catalog_path = directory.0.join("environment-catalog-v1.json");
        let dsh_home = directory.0.join("dsh-home");
        fs::create_dir_all(&dsh_home).expect("create dsh home");

        save_environment(&catalog_path, environment("local-dsh", &dsh_home)).expect("first save");
        save_environment(&catalog_path, environment("work-dsh", &dsh_home)).expect("second save");

        let backup = load_catalog(&sidecar_path(&catalog_path, "bak").expect("backup path"))
            .expect("load backup");
        assert_eq!(backup.revision, 1);
        assert_eq!(backup.environments.len(), 1);
    }

    #[test]
    fn corrupt_catalog_is_not_silently_overwritten() {
        let directory = TestDirectory::new();
        let catalog_path = directory.0.join("environment-catalog-v1.json");
        fs::write(&catalog_path, b"not json").expect("write corrupt catalog");
        let dsh_home = directory.0.join("dsh-home");

        let result = save_environment(&catalog_path, environment("local-dsh", &dsh_home));
        assert!(matches!(result, Err(StoreError::Corrupt)));
        assert_eq!(
            fs::read(&catalog_path).expect("catalog remains"),
            b"not json"
        );
    }

    #[test]
    fn set_active_environment_switches_and_persists() {
        let directory = TestDirectory::new();
        let catalog_path = directory.0.join("environment-catalog-v1.json");
        let dsh_home = directory.0.join("dsh-home");
        save_environment(&catalog_path, environment("local-dsh", &dsh_home)).expect("first");
        let after_first =
            save_environment(&catalog_path, environment("work-dsh", &dsh_home)).expect("second");
        assert_eq!(
            after_first.active_environment_id.as_deref(),
            Some("work-dsh")
        );

        // Switch back to the first environment.
        let switched = set_active_environment(&catalog_path, "local-dsh").expect("switch");
        assert_eq!(switched.active_environment_id.as_deref(), Some("local-dsh"));
        assert!(switched.revision > after_first.revision);

        // Idempotent when the target is already active (revision unchanged).
        let again = set_active_environment(&catalog_path, "local-dsh").expect("idempotent");
        assert_eq!(again.revision, switched.revision);

        // Unknown ids are rejected without touching the catalog.
        let error =
            set_active_environment(&catalog_path, "ghost-dsh").expect_err("unknown environment");
        assert!(matches!(error, StoreError::InvalidEnvironment));
        let loaded = load_catalog(&catalog_path).expect("reload");
        assert_eq!(loaded.active_environment_id.as_deref(), Some("local-dsh"));
    }

    #[test]
    fn remove_environment_deletes_entry_and_bumps_revision() {
        let directory = TestDirectory::new();
        let catalog_path = directory.0.join("environment-catalog-v1.json");
        let dsh_home = directory.0.join("dsh-home");
        save_environment(&catalog_path, environment("local-dsh", &dsh_home)).expect("first");
        let after_first =
            save_environment(&catalog_path, environment("work-dsh", &dsh_home)).expect("second");
        assert_eq!(
            after_first.active_environment_id.as_deref(),
            Some("work-dsh")
        );

        let removed =
            remove_environment(&catalog_path, "local-dsh").expect("remove non-active");
        assert!(removed.environment("local-dsh").is_none());
        assert!(removed.environment("work-dsh").is_some());
        assert!(removed.revision > after_first.revision);
        // The active environment is untouched when a different one is removed.
        assert_eq!(removed.active_environment_id.as_deref(), Some("work-dsh"));
    }

    #[test]
    fn remove_active_environment_clears_active_selection() {
        let directory = TestDirectory::new();
        let catalog_path = directory.0.join("environment-catalog-v1.json");
        let dsh_home = directory.0.join("dsh-home");
        save_environment(&catalog_path, environment("local-dsh", &dsh_home)).expect("first");
        save_environment(&catalog_path, environment("work-dsh", &dsh_home)).expect("second");

        let removed = remove_environment(&catalog_path, "work-dsh").expect("remove active");
        assert!(removed.environment("work-dsh").is_none());
        assert!(removed.environment("local-dsh").is_some());
        assert_eq!(removed.active_environment_id.as_deref(), None);
        let loaded = load_catalog(&catalog_path).expect("reload");
        assert_eq!(loaded.active_environment_id.as_deref(), None);
        // The cleared catalog is still a valid catalog (empty active is legal).
        assert!(validate_catalog(&loaded).is_ok());
    }

    #[test]
    fn remove_unknown_environment_is_rejected_without_touching_catalog() {
        let directory = TestDirectory::new();
        let catalog_path = directory.0.join("environment-catalog-v1.json");
        let dsh_home = directory.0.join("dsh-home");
        let saved =
            save_environment(&catalog_path, environment("local-dsh", &dsh_home)).expect("first");

        let error =
            remove_environment(&catalog_path, "ghost-dsh").expect_err("unknown environment");
        assert!(matches!(error, StoreError::NotFound));
        let loaded = load_catalog(&catalog_path).expect("reload");
        assert_eq!(loaded.revision, saved.revision);
        assert!(loaded.environment("local-dsh").is_some());
    }
}
