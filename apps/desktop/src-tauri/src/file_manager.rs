//! Dev workbench Phase 1 - file manager (WI-M10-WORKBENCH-FS, FS-M1).
//!
//! Human-only surface: every command goes through the Shell capability and
//! the ACL manifest, there is no agent bridge, and the module never takes a
//! caller-supplied absolute path. The security core is containment:
//!
//! - Roots come from the active environment (repository path, DSH home,
//!   harness cwd). The file manager never browses outside them.
//! - A request carries a root id plus a ROOT-RELATIVE path. Resolution is
//!   join -> canonicalize -> require the canonical result to still be
//!   prefixed by the canonical root, so a symlink cannot walk out and a
//!   string comparison cannot be confused by a sibling directory whose name
//!   merely starts with the root name (`root-evil` vs `root`).
//! - `..` segments, absolute paths, UNC/device paths and empty segments are
//!   rejected before any filesystem call.
//! - Symlinks are reported as links and never followed for directory
//!   expansion (no cycles, no escape-by-link); a file read through a link is
//!   resolved-then-checked like everything else.
//!
//! FS-M1 is read-only: tree listing plus a text view. Writes (atomic save,
//! conflict detection) land in FS-M2.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::commands::DshEnvironment;

/// Wire schema version of every FS request/report pair.
pub(crate) const FS_SCHEMA_VERSION: u8 = 1;

/// Upper bound of entries returned by one directory listing; the report says
/// when it truncated so the UI can offer "load more" later.
pub(crate) const MAX_DIR_ENTRIES: usize = 2_000;

/// Files above this size open read-only with an explicit banner (design
/// doc: no silent truncation, no silent write-back of a partial file).
pub(crate) const MAX_EDITABLE_BYTES: u64 = 2 * 1024 * 1024;

/// A browsable root derived from the active environment (form A).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootSpec {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    /// Where the root points, or None when the environment does not provide
    /// it (attached environment without a repository path, missing cwd).
    pub(crate) path: Option<PathBuf>,
    /// Why the root is unavailable, when it is.
    pub(crate) reason: Option<&'static str>,
}

/// Failure of a file-manager request. Every variant maps onto a
/// [CommandError](crate::commands::CommandError) code, and every containment
/// failure is malformed-request shaped: the caller asked for something the
/// surface never offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FsError {
    /// Unknown root id, bad relative path, or a path that escaped its root.
    Malformed(&'static str),
    /// The path does not exist / is not what the request assumed.
    Unavailable(&'static str),
    /// Filesystem failure outside the containment model.
    Io(String),
}

impl FsError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "MALFORMED_MESSAGE",
            Self::Unavailable(_) => "UNAVAILABLE",
            Self::Io(_) => "UNAVAILABLE",
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::Malformed(reason) | Self::Unavailable(reason) => (*reason).to_string(),
            Self::Io(message) => message.clone(),
        }
    }

    pub(crate) fn retryable(&self) -> bool {
        matches!(self, Self::Io(_))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FsRootReport {
    id: &'static str,
    label: &'static str,
    kind: &'static str,
    path: Option<String>,
    available: bool,
    reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FsRootsReport {
    schema_version: u8,
    environment_id: Option<String>,
    roots: Vec<FsRootReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FsEntryReport {
    name: String,
    kind: &'static str,
    size: u64,
    hidden: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FsDirReport {
    schema_version: u8,
    root_id: String,
    path: String,
    entries: Vec<FsEntryReport>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FsFileReport {
    schema_version: u8,
    root_id: String,
    path: String,
    size: u64,
    encoding: &'static str,
    read_only: bool,
    reason: Option<&'static str>,
    content: String,
}

/// The roots the active environment exposes (design doc form A).
///
/// The DSH plugin directories are deliberately NOT a separate root: they live
/// inside the DSH home (`profiles/*/node_modules`), which the `dsh-home` root
/// already covers, and inventing a layout-derived root here would encode a
/// DSH-layout assumption this module does not own.
pub(crate) fn roots_for(environment: Option<&DshEnvironment>) -> Vec<RootSpec> {
    let Some(environment) = environment else {
        return vec![
            RootSpec {
                id: "repo",
                label: "Repository",
                path: None,
                reason: Some("no active environment"),
            },
            RootSpec {
                id: "dsh-home",
                label: "DSH home",
                path: None,
                reason: Some("no active environment"),
            },
        ];
    };

    let repository = environment.harness_repository_path().map(PathBuf::from);
    let cwd = environment.harness_cwd().map(PathBuf::from);
    let dsh_home = PathBuf::from(environment.dsh_home());

    vec![
        RootSpec {
            id: "repo",
            label: "Repository",
            path: repository,
            reason: Some("this environment does not point at a repository"),
        },
        RootSpec {
            id: "dsh-home",
            label: "DSH home",
            path: Some(dsh_home),
            reason: None,
        },
        RootSpec {
            id: "harness-cwd",
            label: "Harness cwd",
            path: cwd,
            reason: Some("this environment has no working directory"),
        },
    ]
}

/// Root list report; the caller passes the active environment.
pub(crate) fn list_roots(environment: Option<&DshEnvironment>) -> FsRootsReport {
    let roots = roots_for(environment)
        .into_iter()
        .map(|spec| {
            let resolved = spec
                .path
                .as_ref()
                .and_then(|path| fs::canonicalize(path).ok());
            match (&spec.path, resolved) {
                (Some(path), Some(_)) => FsRootReport {
                    id: spec.id,
                    label: spec.label,
                    kind: root_kind(spec.id),
                    path: Some(path.to_string_lossy().into_owned()),
                    available: true,
                    reason: None,
                },
                (Some(path), None) => FsRootReport {
                    id: spec.id,
                    label: spec.label,
                    kind: root_kind(spec.id),
                    path: Some(path.to_string_lossy().into_owned()),
                    available: false,
                    reason: Some("the directory does not exist"),
                },
                (None, _) => FsRootReport {
                    id: spec.id,
                    label: spec.label,
                    kind: root_kind(spec.id),
                    path: None,
                    available: false,
                    reason: spec.reason,
                },
            }
        })
        .collect();
    FsRootsReport {
        schema_version: FS_SCHEMA_VERSION,
        environment_id: environment.map(|environment| environment.id().to_string()),
        roots,
    }
}

fn root_kind(id: &str) -> &'static str {
    match id {
        "repo" => "repository",
        "dsh-home" => "dshHome",
        _ => "cwd",
    }
}

/// The directory a root id points at, for the active environment.
pub(crate) fn root_path(
    environment: Option<&DshEnvironment>,
    root_id: &str,
) -> Result<PathBuf, FsError> {
    roots_for(environment)
        .into_iter()
        .find(|spec| spec.id == root_id)
        .ok_or(FsError::Malformed("unknown root"))?
        .path
        .ok_or(FsError::Unavailable(
            "this environment does not provide the requested root",
        ))
}

/// The containment core: canonicalize the root and the joined candidate, and
/// require the candidate to stay inside the root.
pub(crate) fn resolve_in(root: &Path, relative: &str) -> Result<(PathBuf, PathBuf), FsError> {
    let canonical_root = fs::canonicalize(root)
        .map_err(|_| FsError::Unavailable("the root directory does not exist"))?;

    let relative_path = validate_relative(relative)?;
    let candidate = canonical_root.join(relative_path);
    let canonical = fs::canonicalize(&candidate).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => FsError::Unavailable("the path does not exist"),
        _ => FsError::Io(format!("cannot resolve the path: {error}")),
    })?;
    if !canonical.starts_with(&canonical_root) {
        // Symlink or `..` trickery that survived the string checks.
        return Err(FsError::Malformed("the requested path escapes the root"));
    }
    Ok((canonical_root, canonical))
}

/// Reject anything that is not a plain relative path inside the root.
fn validate_relative(relative: &str) -> Result<PathBuf, FsError> {
    if relative.is_empty() || relative == "." {
        return Ok(PathBuf::new());
    }
    if relative.len() > 4096 {
        return Err(FsError::Malformed("the path is too long"));
    }
    let normalized = relative.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(FsError::Malformed("absolute paths are not accepted"));
    }
    // Windows drive-relative (C:foo), UNC (`////server`) and device (`////?/`)
    // forms never survive as an ordinary relative path.
    if normalized.contains(':') || normalized.starts_with("//") {
        return Err(FsError::Malformed(
            "absolute, UNC and device paths are not accepted",
        ));
    }
    let mut path = PathBuf::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(FsError::Malformed("parent segments are not accepted"));
        }
        if segment.contains('\\') || segment.contains(':') {
            return Err(FsError::Malformed("the path component is not accepted"));
        }
        let candidate = Path::new(segment);
        if matches!(
            candidate.components().next(),
            Some(Component::RootDir | Component::Prefix(_))
        ) {
            return Err(FsError::Malformed("the path component is not accepted"));
        }
        path.push(segment);
    }
    Ok(path)
}

/// Directory listing (read-only). Directories first, then name order; hidden
/// entries (dot-prefixed) are filtered unless requested.
pub(crate) fn read_dir(
    environment: Option<&DshEnvironment>,
    root_id: &str,
    relative: &str,
    show_hidden: bool,
) -> Result<FsDirReport, FsError> {
    let root = root_path(environment, root_id)?;
    read_dir_in(&root, root_id, relative, show_hidden)
}

/// [read_dir] against an explicit root (the testable core).
pub(crate) fn read_dir_in(
    root: &Path,
    root_id: &str,
    relative: &str,
    show_hidden: bool,
) -> Result<FsDirReport, FsError> {
    let (_, directory) = resolve_in(root, relative)?;
    let metadata = fs::metadata(&directory).map_err(|error| FsError::Io(error.to_string()))?;
    if !metadata.is_dir() {
        return Err(FsError::Unavailable("the path is not a directory"));
    }

    let mut entries = Vec::new();
    let mut truncated = false;
    let read = fs::read_dir(&directory).map_err(|error| FsError::Io(error.to_string()))?;
    for entry in read {
        let entry = entry.map_err(|error| FsError::Io(error.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let hidden = name.starts_with('.');
        if hidden && !show_hidden {
            continue;
        }
        if entries.len() >= MAX_DIR_ENTRIES {
            truncated = true;
            break;
        }
        // file_type() reports the LINK itself, never the target: a symlink is
        // listed as a link and is not expanded.
        let file_type = entry
            .file_type()
            .map_err(|error| FsError::Io(error.to_string()))?;
        let kind = if file_type.is_symlink() {
            "link"
        } else if file_type.is_dir() {
            "dir"
        } else {
            "file"
        };
        let size = entry
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        entries.push(FsEntryReport {
            name,
            kind,
            size,
            hidden,
        });
    }

    entries.sort_by(|left, right| {
        let left_dir = left.kind == "dir";
        let right_dir = right.kind == "dir";
        right_dir
            .cmp(&left_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(FsDirReport {
        schema_version: FS_SCHEMA_VERSION,
        root_id: root_id.to_string(),
        path: relative_path_string(&directory, relative),
        entries,
        truncated,
    })
}

/// Read a text file for the view pane (read-only in FS-M1).
///
/// UTF-8 (BOM detected and stripped) is the only writable encoding later;
/// anything else - and anything above [MAX_EDITABLE_BYTES] - comes back with
/// `readOnly` set and a reason the UI shows as a banner.
pub(crate) fn read_file(
    environment: Option<&DshEnvironment>,
    root_id: &str,
    relative: &str,
) -> Result<FsFileReport, FsError> {
    let root = root_path(environment, root_id)?;
    read_file_in(&root, root_id, relative)
}

/// [read_file] against an explicit root (the testable core).
pub(crate) fn read_file_in(
    root: &Path,
    root_id: &str,
    relative: &str,
) -> Result<FsFileReport, FsError> {
    let (_, path) = resolve_in(root, relative)?;
    let metadata = fs::metadata(&path).map_err(|error| FsError::Io(error.to_string()))?;
    if metadata.is_dir() {
        return Err(FsError::Unavailable("the path is a directory"));
    }
    if !metadata.is_file() {
        return Err(FsError::Unavailable("the path is not a regular file"));
    }

    let size = metadata.len();
    let oversize = size > MAX_EDITABLE_BYTES;
    let bytes = if oversize {
        read_prefix(&path, MAX_EDITABLE_BYTES as usize)?
    } else {
        fs::read(&path).map_err(|error| FsError::Io(format!("cannot read the file: {error}")))?
    };

    let (encoding, text) = match String::from_utf8(strip_bom(&bytes).to_vec()) {
        Ok(text) => {
            let encoding = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
                "utf-8-bom"
            } else {
                "utf-8"
            };
            (encoding, text)
        }
        // Not text: show a lossy preview, never pretend it is editable.
        Err(error) => (
            "binary",
            String::from_utf8_lossy(error.as_bytes()).into_owned(),
        ),
    };
    let read_only = oversize || encoding == "binary";
    let reason = if oversize {
        Some("the file is larger than the 2 MiB editing limit")
    } else if encoding == "binary" {
        Some("the file is not valid UTF-8")
    } else {
        None
    };

    Ok(FsFileReport {
        schema_version: FS_SCHEMA_VERSION,
        root_id: root_id.to_string(),
        path: relative_path_string(&path, relative),
        size,
        encoding,
        read_only,
        reason,
        content: text,
    })
}

fn read_prefix(path: &Path, limit: usize) -> Result<Vec<u8>, FsError> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|error| FsError::Io(error.to_string()))?;
    let mut buffer = Vec::with_capacity(limit);
    file.take(limit as u64)
        .read_to_end(&mut buffer)
        .map_err(|error| FsError::Io(format!("cannot read the file: {error}")))?;
    Ok(buffer)
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)
}

/// Report the path the way the request addressed it: root-relative, forward
/// slashes, never the resolved absolute path (the UI has no use for it and
/// the module does not leak it).
fn relative_path_string(_absolute: &Path, relative: &str) -> String {
    let normalized = relative.replace('\\', "/");
    if normalized.is_empty() || normalized == "." {
        return ".".to_string();
    }
    normalized.trim_start_matches('/').to_string()
}
/// Request: list the roots of the active environment.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FsRootsRequest {
    schema_version: u8,
}

impl FsRootsRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == FS_SCHEMA_VERSION
    }
}

/// Request: list one directory inside a root.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FsReadDirRequest {
    schema_version: u8,
    root_id: String,
    relative_path: String,
    #[serde(default)]
    show_hidden: bool,
}

impl FsReadDirRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == FS_SCHEMA_VERSION && valid_root_id(&self.root_id)
    }

    pub(crate) fn root_id(&self) -> &str {
        &self.root_id
    }

    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub(crate) fn show_hidden(&self) -> bool {
        self.show_hidden
    }
}

/// Request: read one file inside a root.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FsReadFileRequest {
    schema_version: u8,
    root_id: String,
    relative_path: String,
}

impl FsReadFileRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == FS_SCHEMA_VERSION && valid_root_id(&self.root_id)
    }

    pub(crate) fn root_id(&self) -> &str {
        &self.root_id
    }

    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }
}

/// Root ids are a fixed, short vocabulary; anything else is a malformed
/// request and never reaches the filesystem.
fn valid_root_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Temp root per test (pid + tag), removed on drop.
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!("dsh-fm-{}-{tag}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp root");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, relative: &str, content: &[u8]) {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent");
            }
            fs::write(path, content).expect("write fixture");
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn containment_rejects_parent_and_absolute_forms() {
        assert!(matches!(
            validate_relative(".."),
            Err(FsError::Malformed(_))
        ));
        assert!(matches!(
            validate_relative("a/../../b"),
            Err(FsError::Malformed(_))
        ));
        assert!(matches!(
            validate_relative("/etc/passwd"),
            Err(FsError::Malformed(_))
        ));
        assert!(matches!(
            validate_relative("C:/windows"),
            Err(FsError::Malformed(_))
        ));
        assert!(matches!(
            validate_relative("//server/share"),
            Err(FsError::Malformed(_))
        ));
        // The benign forms resolve to the root itself.
        assert_eq!(validate_relative("").expect("root"), PathBuf::new());
        assert_eq!(validate_relative(".").expect("root"), PathBuf::new());
        assert_eq!(
            validate_relative("a/b.txt").expect("nested"),
            PathBuf::from("a").join("b.txt")
        );
        // Windows separators normalize onto the same relative path.
        assert_eq!(
            validate_relative(raw_string()).expect("windows separators"),
            PathBuf::from("a").join("b.txt")
        );
    }

    fn raw_string() -> &'static str {
        "a\\b.txt"
    }

    #[test]
    fn containment_rejects_escape_through_a_symlink() {
        let root = TempRoot::new("escape");
        let sibling = TempRoot::new("escape-sibling");
        sibling.write("secret.txt", b"not yours");
        // A sibling whose name merely starts with the root name must not be
        // accepted by a prefix check.
        let link = root.path().join("link");
        if !create_dir_symlink(&sibling.0, &link) {
            // Symlink creation needs a privilege/flag on some platforms; the
            // string-level checks above still cover the rest.
            return;
        }
        let error = resolve_in(root.path(), "link/secret.txt").expect_err("escape refused");
        assert!(matches!(error, FsError::Malformed(_)), "got {error:?}");
    }

    #[cfg(unix)]
    fn create_dir_symlink(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(windows)]
    fn create_dir_symlink(target: &Path, link: &Path) -> bool {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }

    #[test]
    fn read_dir_sorts_directories_first_and_filters_hidden() {
        let root = TempRoot::new("listing");
        root.write("zeta.txt", b"z");
        root.write("alpha.txt", b"a");
        root.write("nested/inner.txt", b"i");
        root.write(".hidden", b"h");

        let report = read_dir_in(root.path(), "repo", "", false).expect("list");
        let names: Vec<&str> = report
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, vec!["nested", "alpha.txt", "zeta.txt"]);
        assert_eq!(report.entries[0].kind, "dir");
        assert_eq!(report.entries[1].kind, "file");
        assert_eq!(report.entries[1].size, 1);
        assert!(!report.truncated);

        let with_hidden = read_dir_in(root.path(), "repo", "", true).expect("list hidden");
        assert!(
            with_hidden
                .entries
                .iter()
                .any(|entry| entry.name == ".hidden" && entry.hidden),
            "hidden entries appear only when requested"
        );
    }

    #[test]
    fn read_dir_refuses_files_and_reports_missing_paths() {
        let root = TempRoot::new("dir-errors");
        root.write("file.txt", b"x");
        assert!(matches!(
            read_dir_in(root.path(), "repo", "file.txt", false),
            Err(FsError::Unavailable(_))
        ));
        assert!(matches!(
            read_dir_in(root.path(), "repo", "missing", false),
            Err(FsError::Unavailable(_))
        ));
    }

    #[test]
    fn read_file_reports_encoding_truncation_and_readonly() {
        let root = TempRoot::new("read");
        root.write("plain.txt", b"hello");
        root.write("bom.txt", b"\xef\xbb\xbfbom");
        root.write("binary.bin", &[0xff, 0xfe, 0x00]);
        root.write("big.txt", &vec![b'a'; (MAX_EDITABLE_BYTES + 16) as usize]);

        let plain = read_file_in(root.path(), "repo", "plain.txt").expect("plain");
        assert_eq!(plain.content, "hello");
        assert_eq!(plain.encoding, "utf-8");
        assert!(!plain.read_only);
        assert_eq!(plain.size, 5);

        let bom = read_file_in(root.path(), "repo", "bom.txt").expect("bom");
        assert_eq!(bom.encoding, "utf-8-bom");
        assert_eq!(bom.content, "bom", "the BOM is stripped from the view");

        let binary = read_file_in(root.path(), "repo", "binary.bin").expect("binary");
        assert_eq!(binary.encoding, "binary");
        assert!(binary.read_only);
        assert!(binary.reason.is_some());

        let big = read_file_in(root.path(), "repo", "big.txt").expect("big");
        assert!(big.read_only, "oversized files open read-only");
        assert_eq!(big.content.len() as u64, MAX_EDITABLE_BYTES);
        assert_eq!(big.size, MAX_EDITABLE_BYTES + 16);
    }

    #[test]
    fn read_file_refuses_directories_and_unknown_roots() {
        let root = TempRoot::new("file-errors");
        root.write("nested/inner.txt", b"i");
        assert!(matches!(
            read_file_in(root.path(), "repo", "nested"),
            Err(FsError::Unavailable(_))
        ));
        assert!(matches!(
            root_path(None, "nope"),
            Err(FsError::Malformed(_))
        ));
        assert!(
            matches!(root_path(None, "repo"), Err(FsError::Unavailable(_))),
            "without an environment every root is unavailable"
        );
    }

    #[test]
    fn root_id_vocabulary_is_closed() {
        assert!(valid_root_id("dsh-home"));
        assert!(!valid_root_id(""));
        assert!(!valid_root_id("Repo"));
        assert!(!valid_root_id("a b"));
        assert!(!valid_root_id(&"x".repeat(33)));
    }
}
