use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_EXPLICIT_PATHS: usize = 16;
const MAX_CANDIDATES: usize = 64;
const MAX_PATH_LENGTH: usize = 4096;

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
struct HarnessCandidate {
    id: String,
    source: DiscoverySource,
    mode: CandidateMode,
    requested_path: String,
    canonical_path: Option<String>,
    status: CandidateStatus,
    launchable: bool,
    version: Option<String>,
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

fn inspect_candidate(
    source: DiscoverySource,
    path: &Path,
    requested_path: String,
) -> HarnessCandidate {
    let canonical_path = fs::canonicalize(path)
        .ok()
        .and_then(|value| path_string(&value));

    if path.is_dir() {
        return HarnessCandidate {
            id: String::new(),
            source,
            mode: CandidateMode::Repository,
            requested_path,
            canonical_path,
            status: CandidateStatus::RequiresRecipe,
            launchable: false,
            version: None,
            evidence: vec![evidence(
                "DIRECTORY_REQUIRES_RECIPE",
                "warning",
                "A source directory requires a user-provided prebuilt launch recipe.",
            )],
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
        evidence: vec![evidence(
            "PATH_MISSING",
            "error",
            "The requested path does not exist.",
        )],
    }
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
    fn directory_requires_prebuilt_recipe() {
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
