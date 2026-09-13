//! Git panel backend (GIT-M1): read-only status / diff / log / branches over
//! the environment-linked repository.
//!
//! The git CLI is the backend on purpose: predictable output, no new dependency
//! surface, and fixtures stay trivial. Every invocation goes through [run_git],
//! which never uses a shell, pins `GIT_OPTIONAL_LOCKS=0` (a read must not
//! refresh or lock the index), and caps stdout instead of returning unbounded
//! text.
//!
//! The repository is exactly the environment's harness repository path - a
//! caller never supplies a root. A caller may supply a repo-relative path; it
//! is validated with the file manager's rules plus one git-specific rule: a
//! leading `-` is refused, so a path can never turn into an option.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::commands::DshEnvironment;

/// Wire schema version of every git request/report pair.
pub(crate) const GIT_SCHEMA_VERSION: u8 = 1;

/// Upper bound of a diff (or any captured stdout) we hand to the UI.
pub(crate) const MAX_TEXT_BYTES: usize = 512 * 1024;

/// Upper bound of status entries and log entries per report.
pub(crate) const MAX_STATUS_ENTRIES: usize = 2_000;
pub(crate) const MAX_LOG_ENTRIES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitError {
    /// The request is not one this surface offers (bad path, bad option).
    Malformed(&'static str),
    /// Not a repository, git missing, or the command failed for an
    /// environmental reason.
    Unavailable(String),
}

impl GitError {
    /// Convenience constructor so static reasons and formatted ones read the
    /// same at the call site.
    pub(crate) fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable(reason.into())
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "MALFORMED_MESSAGE",
            Self::Unavailable(_) => "UNAVAILABLE",
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::Malformed(reason) => (*reason).to_string(),
            Self::Unavailable(reason) => reason.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitEntry {
    path: String,
    index_status: String,
    worktree_status: String,
    staged: bool,
    unstaged: bool,
    untracked: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitStatusReport {
    schema_version: u8,
    root: String,
    branch: Option<String>,
    detached: bool,
    clean: bool,
    entries: Vec<GitEntry>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitDiffReport {
    schema_version: u8,
    root: String,
    scope: &'static str,
    path: Option<String>,
    text: String,
    additions: usize,
    deletions: usize,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitLogEntry {
    hash: String,
    author: String,
    authored_at_unix_ms: u64,
    subject: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitLogReport {
    schema_version: u8,
    root: String,
    entries: Vec<GitLogEntry>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitBranchesReport {
    schema_version: u8,
    root: String,
    branches: Vec<String>,
    current: Option<String>,
}

/// The repository a request may touch: exactly the environment's repository
/// root (never a caller-supplied path).
pub(crate) fn repo_root(environment: Option<&DshEnvironment>) -> Result<PathBuf, GitError> {
    let environment = environment.ok_or(GitError::unavailable("no active environment"))?;
    let path = environment
        .harness_repository_path()
        .ok_or(GitError::unavailable(
            "this environment does not point at a repository",
        ))?;
    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err(GitError::unavailable(
            "the repository directory does not exist",
        ));
    }
    Ok(root)
}

/// Validate a caller-supplied repo-relative path.
pub(crate) fn validate_relative(relative: &str) -> Result<String, GitError> {
    let normalized = relative.replace('\\', "/");
    if normalized.is_empty() || normalized.len() > 4096 {
        return Err(GitError::Malformed("the path is not usable"));
    }
    // A leading dash would be read as an option by git.
    if normalized.starts_with('-') {
        return Err(GitError::Malformed("the path is not usable"));
    }
    if normalized.starts_with('/') || normalized.contains(':') || normalized.starts_with("//") {
        return Err(GitError::Malformed(
            "absolute, UNC and device paths are not accepted",
        ));
    }
    for segment in normalized.split('/') {
        if segment == ".." {
            return Err(GitError::Malformed("parent segments are not accepted"));
        }
    }
    Ok(normalized)
}

/// Run one git command inside the repository and return its stdout.
fn run_git(root: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                GitError::unavailable("git is not installed or not on PATH")
            }
            _ => GitError::unavailable(format!("cannot run git: {error}")),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr.lines().next().unwrap_or("").trim();
        return Err(GitError::unavailable(if first.is_empty() {
            "git refused the command".to_string()
        } else {
            first.to_string()
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Read-only status: branch (or detached) plus every changed path.
pub(crate) fn status(environment: Option<&DshEnvironment>) -> Result<GitStatusReport, GitError> {
    status_in(&repo_root(environment)?)
}

/// [status] against an explicit repository (the testable core).
pub(crate) fn status_in(root: &Path) -> Result<GitStatusReport, GitError> {
    let raw = run_git(
        &root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--branch",
            "--no-renames",
            "--untracked-files=all",
        ],
    )?;

    let mut branch = None;
    let mut detached = false;
    let mut entries = Vec::new();
    let mut truncated = false;
    for record in raw.split('\0').filter(|record| !record.is_empty()) {
        if let Some(header) = record.strip_prefix("## ") {
            let head = header.split("...").next().unwrap_or(header).trim();
            if head.starts_with("HEAD") {
                detached = true;
            } else if let Some(name) = head.strip_prefix("No commits yet on ") {
                // A repository with no commits reports its branch this way;
                // such a repo must still work (DESIGN-WORKBENCH-PHASE1).
                branch = Some(name.trim().to_string());
            } else {
                branch = Some(head.to_string());
            }
            continue;
        }
        if entries.len() >= MAX_STATUS_ENTRIES {
            truncated = true;
            break;
        }
        let bytes = record.as_bytes();
        if bytes.len() < 4 {
            continue;
        }
        let index_status = &record[0..1];
        let worktree_status = &record[1..2];
        let path = record[3..].to_string();
        let untracked = index_status == "?";
        entries.push(GitEntry {
            path,
            index_status: index_status.to_string(),
            worktree_status: worktree_status.to_string(),
            staged: !untracked && index_status != " ",
            unstaged: !untracked && worktree_status != " ",
            untracked,
        });
    }

    Ok(GitStatusReport {
        schema_version: GIT_SCHEMA_VERSION,
        root: root.to_string_lossy().into_owned(),
        branch,
        detached,
        clean: entries.is_empty(),
        entries,
        truncated,
    })
}

/// Unified diff of the worktree (or the index with `staged`).
pub(crate) fn diff(
    environment: Option<&DshEnvironment>,
    path: Option<&str>,
    staged: bool,
) -> Result<GitDiffReport, GitError> {
    diff_in(&repo_root(environment)?, path, staged)
}

/// [diff] against an explicit repository (the testable core).
pub(crate) fn diff_in(
    root: &Path,
    path: Option<&str>,
    staged: bool,
) -> Result<GitDiffReport, GitError> {
    let mut args: Vec<String> = vec![
        "diff".into(),
        "--no-color".into(),
        "--unified=3".into(),
        "--no-ext-diff".into(),
    ];
    if staged {
        args.push("--cached".into());
    }
    let validated = match path {
        Some(path) => {
            let relative = validate_relative(path)?;
            args.push("--".into());
            args.push(relative.clone());
            Some(relative)
        }
        None => None,
    };
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let raw = run_git(&root, &borrowed)?;

    let truncated = raw.len() > MAX_TEXT_BYTES;
    let text = if truncated {
        // Cut on a line boundary so the diff stays readable.
        let mut cut = MAX_TEXT_BYTES;
        while cut > 0 && !raw.is_char_boundary(cut) {
            cut -= 1;
        }
        raw[..cut].to_string()
    } else {
        raw
    };

    Ok(GitDiffReport {
        schema_version: GIT_SCHEMA_VERSION,
        root: root.to_string_lossy().into_owned(),
        scope: if staged { "staged" } else { "worktree" },
        path: validated,
        additions: text
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .count(),
        deletions: text
            .lines()
            .filter(|line| line.starts_with('-') && !line.starts_with("---"))
            .count(),
        text,
        truncated,
    })
}

/// Recent commits on the current branch.
pub(crate) fn log(
    environment: Option<&DshEnvironment>,
    limit: usize,
) -> Result<GitLogReport, GitError> {
    log_in(&repo_root(environment)?, limit)
}

/// [log] against an explicit repository (the testable core).
pub(crate) fn log_in(root: &Path, limit: usize) -> Result<GitLogReport, GitError> {
    let capped = limit.clamp(1, MAX_LOG_ENTRIES);
    let count = format!("--max-count={capped}");
    let raw = run_git(&root, &["log", &count, "--format=%H%x1f%an%x1f%at%x1f%s"])?;

    let mut entries = Vec::new();
    let mut truncated = false;
    for line in raw.lines() {
        if entries.len() >= capped {
            truncated = true;
            break;
        }
        let mut fields = line.split('\u{1f}');
        let hash = fields.next().unwrap_or_default().to_string();
        let author = fields.next().unwrap_or_default().to_string();
        let authored = fields.next().unwrap_or_default();
        let subject = fields.next().unwrap_or_default().to_string();
        if hash.is_empty() {
            continue;
        }
        entries.push(GitLogEntry {
            hash,
            author,
            authored_at_unix_ms: authored.parse::<u64>().unwrap_or(0) * 1000,
            subject,
        });
    }

    Ok(GitLogReport {
        schema_version: GIT_SCHEMA_VERSION,
        root: root.to_string_lossy().into_owned(),
        entries,
        truncated,
    })
}

/// Local branches with the current one marked.
pub(crate) fn branches(
    environment: Option<&DshEnvironment>,
) -> Result<GitBranchesReport, GitError> {
    branches_in(&repo_root(environment)?)
}

/// [branches] against an explicit repository (the testable core).
pub(crate) fn branches_in(root: &Path) -> Result<GitBranchesReport, GitError> {
    let raw = run_git(
        &root,
        &[
            "for-each-ref",
            "--format=%(refname:short)%1f%(HEAD)",
            "refs/heads",
        ],
    )?;
    let mut branches = Vec::new();
    let mut current = None;
    for line in raw.lines() {
        let mut fields = line.split('\u{1f}');
        let name = fields.next().unwrap_or_default().trim().to_string();
        if name.is_empty() {
            continue;
        }
        if fields.next().unwrap_or_default().trim() == "*" {
            current = Some(name.clone());
        }
        branches.push(name);
    }
    Ok(GitBranchesReport {
        schema_version: GIT_SCHEMA_VERSION,
        root: root.to_string_lossy().into_owned(),
        branches,
        current,
    })
}
/// Request: repository status.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitStatusRequest {
    schema_version: u8,
}

impl GitStatusRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == GIT_SCHEMA_VERSION
    }
}

/// Request: load-local branches.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitBranchesRequest {
    schema_version: u8,
}

impl GitBranchesRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == GIT_SCHEMA_VERSION
    }
}

/// Request: unified diff, optionally narrowed to one repo-relative path.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitDiffRequest {
    schema_version: u8,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    staged: bool,
}

impl GitDiffRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == GIT_SCHEMA_VERSION
    }

    pub(crate) fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub(crate) fn staged(&self) -> bool {
        self.staged
    }
}

/// Request: recent commits.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitLogRequest {
    schema_version: u8,
    #[serde(default)]
    limit: Option<usize>,
}

impl GitLogRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == GIT_SCHEMA_VERSION
    }

    pub(crate) fn limit(&self) -> usize {
        self.limit.unwrap_or(50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command as StdCommand;

    /// A throwaway repository: real `git init` beats faking porcelain output.
    struct TempRepo(PathBuf);

    impl TempRepo {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!("dsh-git-{}-{tag}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create repo dir");
            let repo = Self(path);
            repo.git(&["init", "-b", "main"]);
            repo.git(&["config", "user.email", "test@example.invalid"]);
            repo.git(&["config", "user.name", "DSH Test"]);
            repo
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn git(&self, args: &[&str]) {
            let output = StdCommand::new("git")
                .arg("-C")
                .arg(&self.0)
                .args(args)
                .output()
                .expect("run git");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn write(&self, name: &str, content: &str) {
            fs::write(self.0.join(name), content).expect("write file");
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn status_walks_untracked_then_modified_then_staged() {
        let repo = TempRepo::new("status");
        repo.write("a.txt", "one");

        let untracked = status_in(repo.path()).expect("status");
        assert!(!untracked.clean);
        assert_eq!(untracked.branch.as_deref(), Some("main"));
        assert!(
            untracked
                .entries
                .iter()
                .any(|entry| entry.path == "a.txt" && entry.untracked)
        );

        repo.git(&["add", "a.txt"]);
        let staged_only = status_in(repo.path()).expect("status");
        assert!(
            staged_only
                .entries
                .iter()
                .any(|entry| entry.path == "a.txt" && entry.staged && !entry.untracked),
            "a staged add reports as staged"
        );

        repo.git(&["commit", "-m", "first"]);
        assert!(
            status_in(repo.path()).expect("status").clean,
            "a clean tree is clean"
        );

        repo.write("a.txt", "two");
        let modified = status_in(repo.path()).expect("status");
        assert!(
            modified
                .entries
                .iter()
                .any(|entry| entry.path == "a.txt" && entry.unstaged),
            "a worktree edit reports as unstaged"
        );
    }

    #[test]
    fn diff_covers_the_worktree_and_the_index() {
        let repo = TempRepo::new("diff");
        repo.write("a.txt", "one");
        repo.git(&["add", "a.txt"]);
        repo.git(&["commit", "-m", "first"]);
        repo.write("a.txt", "two");

        let worktree = diff_in(repo.path(), None, false).expect("diff");
        assert_eq!(worktree.scope, "worktree");
        assert!(
            worktree.text.contains("-one"),
            "removed line present: {}",
            worktree.text
        );
        assert!(worktree.text.contains("+two"));
        assert_eq!(worktree.additions, 1);
        assert_eq!(worktree.deletions, 1);

        // Nothing staged yet, so the cached diff is empty.
        assert!(
            diff_in(repo.path(), None, true)
                .expect("diff")
                .text
                .trim()
                .is_empty()
        );

        repo.git(&["add", "a.txt"]);
        let cached = diff_in(repo.path(), None, true).expect("diff");
        assert_eq!(cached.scope, "staged");
        assert!(cached.text.contains("+two"));

        // A single path narrows the diff.
        let scoped = diff_in(repo.path(), Some("a.txt"), true).expect("diff");
        assert_eq!(scoped.path.as_deref(), Some("a.txt"));
    }

    #[test]
    fn log_and_branches_describe_the_repository() {
        let repo = TempRepo::new("log");
        repo.write("a.txt", "one");
        repo.git(&["add", "a.txt"]);
        repo.git(&["commit", "-m", "first commit"]);
        repo.git(&["branch", "feature"]);

        let log = log_in(repo.path(), 10).expect("log");
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0].subject, "first commit");
        assert_eq!(log.entries[0].author, "DSH Test");
        assert!(log.entries[0].authored_at_unix_ms > 0);
        assert_eq!(log.entries[0].hash.len(), 40);

        let branches = branches_in(repo.path()).expect("branches");
        assert_eq!(branches.current.as_deref(), Some("main"));
        assert!(branches.branches.contains(&"feature".to_string()));
    }

    #[test]
    fn paths_are_validated_before_they_reach_git() {
        assert!(matches!(
            validate_relative("../secret"),
            Err(GitError::Malformed(_))
        ));
        assert!(matches!(
            validate_relative("/etc/passwd"),
            Err(GitError::Malformed(_))
        ));
        assert!(matches!(
            validate_relative("C:/windows"),
            Err(GitError::Malformed(_))
        ));
        // A leading dash would be read as an option.
        assert!(matches!(
            validate_relative("--upload-pack=evil"),
            Err(GitError::Malformed(_))
        ));
        assert!(matches!(validate_relative(""), Err(GitError::Malformed(_))));
        assert_eq!(
            validate_relative("src/main.rs").expect("plain path"),
            "src/main.rs"
        );
    }

    #[test]
    fn a_missing_environment_or_non_repository_degrades_cleanly() {
        assert!(matches!(repo_root(None), Err(GitError::Unavailable(_))));

        let dir = std::env::temp_dir().join(format!("dsh-git-not-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create dir");
        let error = status_in(&dir).expect_err("a plain directory is not a repository");
        assert!(matches!(error, GitError::Unavailable(_)), "got {error:?}");
        let _ = fs::remove_dir_all(&dir);
    }
}
