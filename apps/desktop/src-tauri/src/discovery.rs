use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_EXPLICIT_PATHS: usize = 16;
const MAX_CANDIDATES: usize = 64;
const MAX_PATH_LENGTH: usize = 4096;
const DSH_ROOT_PACKAGE_NAME: &str = "@deepseek-ai/dsh-root";
const REPO_ENTRY_REL: &str = "apps/cli/src/bin.ts";
const REPO_LOADER_REL: &str = "scripts/register-tsx-esm.mjs";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessDiscoveryRequest {
    schema_version: u8,
    explicit_paths: Vec<String>,
    include_dsh_path: bool,
    include_path: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessDiscoveryReport {
    schema_version: u8,
    scanned_sources: Vec<DiscoverySource>,
    deferred_sources: Vec<DiscoverySource>,
    candidates: Vec<HarnessCandidate>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum DiscoverySource {
    Explicit,
    DshPath,
    Path,
    Global,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum CandidateMode {
    Executable,
    Repository,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum CandidateStatus {
    Available,
    Missing,
    RequiresRecipe,
    Unverified,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryInfo {
    repo_root: String,
    entry: String,
    loader: Option<String>,
    needs_install: bool,
    needs_build: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HarnessCandidate {
    id: String,
    source: DiscoverySource,
    mode: CandidateMode,
    requested_path: String,
    canonical_path: Option<String>,
    status: CandidateStatus,
    launchable: bool,
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<RepositoryInfo>,
    evidence: Vec<DiscoveryEvidence>,
}

#[derive(Debug, Clone, Serialize)]
struct DiscoveryEvidence {
    code: &'static str,
    severity: &'static str,
    message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscoveryError {
    MalformedRequest,
}

pub(crate) fn discover_harnesses(
    request: HarnessDiscoveryRequest,
) -> Result<HarnessDiscoveryReport, DiscoveryError> {
    let dsh_path = request
        .include_dsh_path
        .then(|| env::var_os("DSH_PATH"))
        .flatten()
        .map(PathBuf::from);
    let path_directories = request
        .include_path
        .then(|| env::var_os("PATH"))
        .flatten()
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default();
    discover_with_sources(request, dsh_path, path_directories)
}

fn discover_with_sources(
    request: HarnessDiscoveryRequest,
    dsh_path: Option<PathBuf>,
    path_directories: Vec<PathBuf>,
) -> Result<HarnessDiscoveryReport, DiscoveryError> {
    validate_request(&request)?;

    let mut scanned_sources = vec![DiscoverySource::Explicit];
    let mut inputs = request
        .explicit_paths
        .iter()
        .map(|path| (DiscoverySource::Explicit, PathBuf::from(path)))
        .collect::<Vec<_>>();

    if request.include_dsh_path {
        scanned_sources.push(DiscoverySource::DshPath);
        if let Some(path) = dsh_path {
            inputs.push((DiscoverySource::DshPath, path));
        }
    }
    if request.include_path {
        scanned_sources.push(DiscoverySource::Path);
        for directory in path_directories {
            for candidate_name in path_candidate_names() {
                let path = directory.join(candidate_name);
                if path.is_file() {
                    inputs.push((DiscoverySource::Path, path));
                }
            }
        }
    }

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for (source, path) in inputs {
        if candidates.len() >= MAX_CANDIDATES {
            break;
        }
        let Some(requested_path) = path_string(&path) else {
            continue;
        };
        let candidate = inspect_candidate(source, &path, requested_path);
        let key = deduplication_key(&candidate);
        if seen.insert(key) {
            candidates.push(candidate);
        }
    }

    for (index, candidate) in candidates.iter_mut().enumerate() {
        candidate.id = format!("candidate-{:04}", index + 1);
    }

    Ok(HarnessDiscoveryReport {
        schema_version: 1,
        scanned_sources,
        deferred_sources: vec![DiscoverySource::Global],
        candidates,
    })
}

fn validate_request(request: &HarnessDiscoveryRequest) -> Result<(), DiscoveryError> {
    if request.schema_version != 1
        || request.explicit_paths.len() > MAX_EXPLICIT_PATHS
        || request
            .explicit_paths
            .iter()
            .any(|path| path.trim().is_empty() || path.chars().count() > MAX_PATH_LENGTH)
    {
        return Err(DiscoveryError::MalformedRequest);
    }
    Ok(())
}

/// Canonicalize a path into a display-friendly absolute string.
///
/// Windows `fs::canonicalize` returns verbatim paths (prefix `\\?\`)
/// which look alien in the UI, so the prefix is stripped here once for
/// every surfaced path.
fn canonicalize_path(path: &Path) -> Option<String> {
    let value = fs::canonicalize(path).ok()?;
    let text = path_string(&value)?;
    Some(strip_verbatim_prefix(&text))
}

#[cfg(windows)]
fn strip_verbatim_prefix(text: &str) -> String {
    text.strip_prefix("\\\\?\\").unwrap_or(text).to_string()
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(text: &str) -> String {
    text.to_string()
}

fn inspect_candidate(
    source: DiscoverySource,
    path: &Path,
    requested_path: String,
) -> HarnessCandidate {
    let canonical_path = canonicalize_path(path);

    if path.is_dir() {
        return match probe_repository(path, &requested_path) {
            Ok((repository, version)) => {
                let mut repo_evidence = vec![evidence(
                    "REPO_RECOGNIZED",
                    "info",
                    "Directory is a recognized DeepSeek Harness source repository.",
                )];
                if repository.loader.is_none() {
                    repo_evidence.push(evidence(
                        "LOADER_MISSING",
                        "warning",
                        "The repository is missing the TS loader (scripts/register-tsx-esm.mjs); the CLI cannot start from TypeScript sources without it.",
                    ));
                }
                HarnessCandidate {
                    id: String::new(),
                    source,
                    mode: CandidateMode::Repository,
                    requested_path,
                    canonical_path,
                    status: CandidateStatus::Available,
                    launchable: true,
                    version,
                    repository: Some(repository),
                    evidence: repo_evidence,
                }
            }
            Err(RepoProbeError::NotADshRepo) => HarnessCandidate {
                id: String::new(),
                source,
                mode: CandidateMode::Repository,
                requested_path,
                canonical_path,
                status: CandidateStatus::RequiresRecipe,
                launchable: false,
                version: None,
                repository: None,
                evidence: vec![evidence(
                    "NOT_A_DSH_REPO",
                    "error",
                    "The directory is not a recognized DeepSeek Harness source repository.",
                )],
            },
            Err(RepoProbeError::EntryMissing) => HarnessCandidate {
                id: String::new(),
                source,
                mode: CandidateMode::Repository,
                requested_path,
                canonical_path,
                status: CandidateStatus::RequiresRecipe,
                launchable: false,
                version: None,
                repository: None,
                evidence: vec![evidence(
                    "REPO_ENTRY_MISSING",
                    "error",
                    "The repository is missing its CLI entry point (apps/cli/src/bin.ts).",
                )],
            },
        };
    }

    if path.is_file() {
        let launchable = is_directly_executable(path);
        return HarnessCandidate {
            id: String::new(),
            source,
            mode: CandidateMode::Executable,
            requested_path,
            canonical_path,
            status: if launchable {
                CandidateStatus::Available
            } else {
                CandidateStatus::Unverified
            },
            launchable,
            version: None,
            repository: None,
            evidence: vec![if launchable {
                evidence(
                    "FILE_CANDIDATE",
                    "info",
                    "A regular-file launch candidate exists; discovery did not execute it.",
                )
            } else {
                evidence(
                    "WRAPPER_REQUIRES_ADAPTER",
                    "warning",
                    "The file requires an explicit non-shell launch adapter before use.",
                )
            }],
        };
    }

    HarnessCandidate {
        id: String::new(),
        source,
        mode: CandidateMode::Executable,
        requested_path,
        canonical_path: None,
        status: CandidateStatus::Missing,
        launchable: false,
        version: None,
        repository: None,
        evidence: vec![evidence(
            "PATH_MISSING",
            "error",
            "The requested path does not exist.",
        )],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepoProbeError {
    NotADshRepo,
    EntryMissing,
}

/// Probe a directory as a DeepSeek Harness source repository (read-only).
///
/// Recognition: root package name `@deepseek-ai/dsh-root` OR the structural
/// fallback (pnpm workspace + CLI entry + TS loader all present) so that renamed
/// forks stay recognizable. Never executes or writes anything.
fn probe_repository(path: &Path, requested_path: &str) -> Result<(RepositoryInfo, Option<String>), RepoProbeError> {
    let has_workspace = path.join("pnpm-workspace.yaml").is_file();
    let has_entry = path.join(REPO_ENTRY_REL).is_file();
    let has_loader = path.join(REPO_LOADER_REL).is_file();
    let (package_name, package_version) = read_package_identity(path)
        .map(|(name, version)| (Some(name), version))
        .unwrap_or((None, None));

    let recognized = package_name.as_deref() == Some(DSH_ROOT_PACKAGE_NAME)
        || (has_workspace && has_entry && has_loader);
    if !recognized {
        return Err(RepoProbeError::NotADshRepo);
    }
    if !has_entry {
        return Err(RepoProbeError::EntryMissing);
    }

    let repo_root = canonicalize_path(path).unwrap_or_else(|| requested_path.to_string());
    let repository = RepositoryInfo {
        repo_root,
        entry: REPO_ENTRY_REL.to_string(),
        loader: has_loader.then(|| REPO_LOADER_REL.to_string()),
        needs_install: !path.join("node_modules").is_dir(),
        needs_build: !path.join("apps/web/dist/index.html").is_file(),
    };
    Ok((repository, package_version))
}

fn read_package_identity(path: &Path) -> Option<(String, Option<String>)> {
    let raw = fs::read_to_string(path.join("package.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let name = value.get("name")?.as_str()?.to_string();
    let version = value
        .get("version")
        .and_then(|entry| entry.as_str())
        .map(str::to_string);
    Some((name, version))
}

fn evidence(
    code: &'static str,
    severity: &'static str,
    message: &'static str,
) -> DiscoveryEvidence {
    DiscoveryEvidence {
        code,
        severity,
        message,
    }
}

fn deduplication_key(candidate: &HarnessCandidate) -> String {
    let key = candidate
        .canonical_path
        .as_deref()
        .unwrap_or(&candidate.requested_path);
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key.to_string()
    }
}

fn path_string(path: &Path) -> Option<String> {
    let value = path.to_string_lossy().into_owned();
    (value.chars().count() <= MAX_PATH_LENGTH).then_some(value)
}

#[cfg(windows)]
fn path_candidate_names() -> &'static [&'static str] {
    &["dsh.exe", "dsh.cmd", "dsh.bat", "dsh.com"]
}

#[cfg(not(windows))]
fn path_candidate_names() -> &'static [&'static str] {
    &["dsh"]
}

#[cfg(windows)]
fn is_directly_executable(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

#[cfg(unix)]
fn is_directly_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(any(windows, unix)))]
fn is_directly_executable(_path: &Path) -> bool {
    false
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
            let path = env::temp_dir().join(format!(
                "dsh-desktop-discovery-test-{}-{id}",
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

    fn request(paths: Vec<String>) -> HarnessDiscoveryRequest {
        HarnessDiscoveryRequest {
            schema_version: 1,
            explicit_paths: paths,
            include_dsh_path: false,
            include_path: false,
        }
    }

    #[test]
    fn canonical_paths_do_not_carry_verbatim_prefix() {
        let directory = TestDirectory::new();
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        let canonical = report.candidates[0]
            .canonical_path
            .as_deref()
            .expect("canonical path");
        assert!(
            !canonical.starts_with("\\\\?\\"),
            "canonical path must not carry the Windows verbatim prefix: {canonical}"
        );
        assert!(Path::new(canonical).is_dir());
    }

    #[test]
    fn missing_explicit_path_is_retained_as_evidence() {
        let directory = TestDirectory::new();
        let missing = directory.0.join("missing-dsh");
        let report = discover_with_sources(
            request(vec![missing.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        assert_eq!(report.candidates.len(), 1);
        assert!(matches!(
            report.candidates[0].status,
            CandidateStatus::Missing
        ));
        assert!(!report.candidates[0].launchable);
    }

    #[test]
    fn empty_directory_is_not_a_dsh_repo() {
        let directory = TestDirectory::new();
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        assert!(matches!(
            report.candidates[0].status,
            CandidateStatus::RequiresRecipe
        ));
        assert!(!report.candidates[0].launchable);
        assert!(has_evidence(&report.candidates[0], "NOT_A_DSH_REPO"));
        assert!(report.candidates[0].repository.is_none());
    }

    fn write_file(directory: &Path, relative: &str, content: &[u8]) {
        let path = directory.join(relative);
        fs::create_dir_all(path.parent().expect("relative parent"))
            .expect("create parent directories");
        fs::write(path, content).expect("write fixture file");
    }

    fn full_repo_markers(directory: &TestDirectory) {
        write_file(
            &directory.0,
            "package.json",
            br#"{"name":"@deepseek-ai/dsh-root","version":"0.2.0-test"}"#,
        );
        write_file(
            &directory.0,
            "pnpm-workspace.yaml",
            b"packages:
  - apps/*
  - packages/*
",
        );
        write_file(&directory.0, "apps/cli/src/bin.ts", b"console.log('dsh')");
        write_file(
            &directory.0,
            "scripts/register-tsx-esm.mjs",
            b"export {};
",
        );
        write_file(&directory.0, "node_modules/.pnpm/.keep", b"");
        write_file(&directory.0, "apps/web/dist/index.html", b"<html></html>");
    }

    fn has_evidence(candidate: &HarnessCandidate, code: &str) -> bool {
        candidate.evidence.iter().any(|entry| entry.code == code)
    }

    #[test]
    fn recognized_source_repo_is_available_with_details() {
        let directory = TestDirectory::new();
        full_repo_markers(&directory);
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        let candidate = &report.candidates[0];
        assert!(matches!(candidate.status, CandidateStatus::Available));
        assert!(candidate.launchable);
        assert!(matches!(candidate.mode, CandidateMode::Repository));
        assert_eq!(candidate.version.as_deref(), Some("0.2.0-test"));
        assert!(has_evidence(candidate, "REPO_RECOGNIZED"));
        let repository = candidate.repository.as_ref().expect("repository info");
        assert_eq!(repository.entry, "apps/cli/src/bin.ts");
        assert_eq!(
            repository.loader.as_deref(),
            Some("scripts/register-tsx-esm.mjs")
        );
        assert!(!repository.needs_install);
        assert!(!repository.needs_build);
    }

    #[test]
    fn repo_without_node_modules_needs_install() {
        let directory = TestDirectory::new();
        full_repo_markers(&directory);
        fs::remove_dir_all(directory.0.join("node_modules")).expect("remove node_modules");
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        let repository = report.candidates[0]
            .repository
            .as_ref()
            .expect("repository info");
        assert!(repository.needs_install);
        assert!(!repository.needs_build);
    }

    #[test]
    fn repo_without_web_assets_needs_build() {
        let directory = TestDirectory::new();
        full_repo_markers(&directory);
        fs::remove_file(directory.0.join("apps/web/dist/index.html")).expect("remove dist html");
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        let repository = report.candidates[0]
            .repository
            .as_ref()
            .expect("repository info");
        assert!(!repository.needs_install);
        assert!(repository.needs_build);
    }

    #[test]
    fn non_repo_directory_with_foreign_package_is_rejected() {
        let directory = TestDirectory::new();
        write_file(
            &directory.0,
            "package.json",
            br#"{"name":"some-other-project","version":"1.0.0"}"#,
        );
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        let candidate = &report.candidates[0];
        assert!(matches!(
            candidate.status,
            CandidateStatus::RequiresRecipe
        ));
        assert!(!candidate.launchable);
        assert!(has_evidence(candidate, "NOT_A_DSH_REPO"));
    }

    #[test]
    fn renamed_fork_is_recognized_by_structure() {
        let directory = TestDirectory::new();
        full_repo_markers(&directory);
        write_file(
            &directory.0,
            "package.json",
            br#"{"name":"my-private-fork","version":"0.9.0"}"#,
        );
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        let candidate = &report.candidates[0];
        assert!(matches!(candidate.status, CandidateStatus::Available));
        assert_eq!(candidate.version.as_deref(), Some("0.9.0"));
    }

    #[test]
    fn root_package_without_entry_reports_missing_entry() {
        let directory = TestDirectory::new();
        write_file(
            &directory.0,
            "package.json",
            br#"{"name":"@deepseek-ai/dsh-root","version":"0.2.0-test"}"#,
        );
        // name matches, but no structural markers at all
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        let candidate = &report.candidates[0];
        assert!(matches!(
            candidate.status,
            CandidateStatus::RequiresRecipe
        ));
        assert!(has_evidence(candidate, "REPO_ENTRY_MISSING"));
        assert!(candidate.repository.is_none());
    }

    #[test]
    fn repo_without_loader_reports_warning() {
        let directory = TestDirectory::new();
        full_repo_markers(&directory);
        fs::remove_file(directory.0.join("scripts/register-tsx-esm.mjs"))
            .expect("remove loader");
        let report = discover_with_sources(
            request(vec![directory.0.to_string_lossy().into_owned()]),
            None,
            vec![],
        )
        .expect("discover");
        let candidate = &report.candidates[0];
        assert!(matches!(candidate.status, CandidateStatus::Available));
        assert!(candidate.launchable);
        assert!(has_evidence(candidate, "LOADER_MISSING"));
        let repository = candidate.repository.as_ref().expect("repository info");
        assert!(repository.loader.is_none());
    }

    #[test]
    fn duplicate_candidates_are_suppressed_without_execution() {
        let directory = TestDirectory::new();
        let candidate = directory
            .0
            .join(if cfg!(windows) { "dsh.exe" } else { "dsh" });
        fs::write(&candidate, b"must not execute").expect("write candidate");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700))
                .expect("mark executable");
        }

        let mut request = request(vec![candidate.to_string_lossy().into_owned()]);
        request.include_dsh_path = true;
        request.include_path = true;
        let report =
            discover_with_sources(request, Some(candidate.clone()), vec![directory.0.clone()])
                .expect("discover");
        assert_eq!(report.candidates.len(), 1);
        assert!(report.candidates[0].version.is_none());
        assert_eq!(
            fs::read(&candidate).expect("candidate unchanged"),
            b"must not execute"
        );
    }

    #[test]
    fn rejects_oversized_request() {
        let report = discover_with_sources(
            request(
                (0..=MAX_EXPLICIT_PATHS)
                    .map(|index| format!("p{index}"))
                    .collect(),
            ),
            None,
            vec![],
        );
        assert!(matches!(report, Err(DiscoveryError::MalformedRequest)));
    }
}
