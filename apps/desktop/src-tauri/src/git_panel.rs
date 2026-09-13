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

/// The one place a git process is built.
///
/// `optional_locks` pins `GIT_OPTIONAL_LOCKS=0` for reads so they never refresh
/// or lock the index. A mutation deliberately leaves the locks alone - it needs
/// them - and pins `GIT_EDITOR` so nothing can ever block on an editor.
fn run_git_raw(
    root: &Path,
    args: &[&str],
    optional_locks: bool,
) -> Result<std::process::Output, GitError> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    if optional_locks {
        command.env("GIT_OPTIONAL_LOCKS", "0");
    } else {
        command.env("GIT_EDITOR", "true");
    }
    command.output().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            GitError::unavailable("git is not installed or not on PATH")
        }
        _ => GitError::unavailable(format!("cannot run git: {error}")),
    })
}

/// Turn a finished invocation into stdout, or into the reason it failed.
fn output_or_reason(output: std::process::Output) -> Result<String, GitError> {
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

/// Run one git command inside the repository and return its stdout.
fn run_git(root: &Path, args: &[&str]) -> Result<String, GitError> {
    output_or_reason(run_git_raw(root, args, true)?)
}

/// Run one git command that is allowed to write (GIT-M2).
fn run_git_write(root: &Path, args: &[&str]) -> Result<String, GitError> {
    output_or_reason(run_git_raw(root, args, false)?)
}

/// Read-only status: branch (or detached) plus every changed path.
pub(crate) fn status(environment: Option<&DshEnvironment>) -> Result<GitStatusReport, GitError> {
    status_in(&repo_root(environment)?)
}

/// [status] against an explicit repository (the testable core).
pub(crate) fn status_in(root: &Path) -> Result<GitStatusReport, GitError> {
    let raw = run_git(
        root,
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
    let raw = run_git(root, &borrowed)?;

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
    let raw = run_git(root, &["log", &count, "--format=%H%x1f%an%x1f%at%x1f%s"])?;

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
        root,
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

/* ------------------------------------------------------------------ */
/* GIT-M2: the mutating half (stage / unstage / commit / discard)       */
/* ------------------------------------------------------------------ */

/// Upper bound of a commit message. Git has no useful limit of its own; this
/// one keeps a pasted file out of the index.
pub(crate) const MAX_COMMIT_MESSAGE_BYTES: usize = 8_000;

/// Result of a mutation: what was asked for, plus the status it produced, so
/// the UI never has to re-ask or guess.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitMutationReport {
    schema_version: u8,
    root: String,
    operation: &'static str,
    path: Option<String>,
    /// Set only when the operation had to take a different primitive than the
    /// obvious one.
    detail: Option<String>,
    status: GitStatusReport,
}

/// True when the repository has a commit to restore from.
fn has_head(root: &Path) -> Result<bool, GitError> {
    let output = run_git_raw(root, &["rev-parse", "--verify", "--quiet", "HEAD"], true)?;
    Ok(output.status.success())
}

/// Build the report every mutation ends with: the fresh status, taken after the
/// operation so it describes the state the user now has.
fn mutation_report(
    root: &Path,
    operation: &'static str,
    path: Option<&str>,
    detail: Option<String>,
) -> Result<GitMutationReport, GitError> {
    Ok(GitMutationReport {
        schema_version: GIT_SCHEMA_VERSION,
        root: root.to_string_lossy().into_owned(),
        operation,
        path: path.map(str::to_string),
        detail,
        status: status_in(root)?,
    })
}

/// Stage one path, or every change.
pub(crate) fn stage(
    environment: Option<&DshEnvironment>,
    path: Option<&str>,
    all: bool,
) -> Result<GitMutationReport, GitError> {
    stage_in(&repo_root(environment)?, path, all)
}

/// [stage] against an explicit repository (the testable core).
pub(crate) fn stage_in(
    root: &Path,
    path: Option<&str>,
    all: bool,
) -> Result<GitMutationReport, GitError> {
    let target = match (path, all) {
        (Some(_), true) => return Err(GitError::Malformed("stage takes a path or all, not both")),
        (None, false) => return Err(GitError::Malformed("stage needs a path or all")),
        (Some(path), false) => validate_relative(path)?,
        (None, true) => ".".to_string(),
    };
    // -A so a deletion stages as a deletion; -- so the path can never be read as
    // an option (validate_relative already refuses a leading dash).
    run_git_write(root, &["add", "-A", "--", &target])?;
    mutation_report(root, "stage", path, None)
}

/// Unstage one path, or the whole index.
pub(crate) fn unstage(
    environment: Option<&DshEnvironment>,
    path: Option<&str>,
    all: bool,
) -> Result<GitMutationReport, GitError> {
    unstage_in(&repo_root(environment)?, path, all)
}

/// [unstage] against an explicit repository (the testable core).
pub(crate) fn unstage_in(
    root: &Path,
    path: Option<&str>,
    all: bool,
) -> Result<GitMutationReport, GitError> {
    let target = match (path, all) {
        (Some(_), true) => {
            return Err(GitError::Malformed("unstage takes a path or all, not both"));
        }
        (None, false) => return Err(GitError::Malformed("unstage needs a path or all")),
        (Some(path), false) => validate_relative(path)?,
        (None, true) => ".".to_string(),
    };
    // `git restore --staged` restores the index from HEAD, so a repository with
    // no commits yet has nothing to restore from and the index is reset with
    // `rm --cached` instead. `--cached` never touches the worktree, so neither
    // primitive can lose anything on disk; the report says which one ran.
    let head = has_head(root)?;
    let args: Vec<&str> = if head {
        vec!["restore", "--staged", "--", &target]
    } else {
        vec!["rm", "--cached", "-r", "-f", "--", &target]
    };
    run_git_write(root, &args)?;
    let detail = (!head).then(|| {
        "the repository has no commits yet, so the index was reset with git rm --cached".to_string()
    });
    mutation_report(root, "unstage", path, detail)
}

/// Commit what is staged.
pub(crate) fn commit(
    environment: Option<&DshEnvironment>,
    message: &str,
) -> Result<GitMutationReport, GitError> {
    commit_in(&repo_root(environment)?, message)
}

/// [commit] against an explicit repository (the testable core).
pub(crate) fn commit_in(root: &Path, message: &str) -> Result<GitMutationReport, GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::Malformed("a commit needs a message"));
    }
    if message.len() > MAX_COMMIT_MESSAGE_BYTES {
        return Err(GitError::Malformed(
            "the commit message is longer than 8000 bytes",
        ));
    }
    // Both guards run before git does: an empty commit would otherwise come
    // back as a generic git failure and the surface would have to guess what
    // actually happened.
    if !status_in(root)?.entries.iter().any(|entry| entry.staged) {
        return Err(GitError::unavailable("there is nothing staged to commit"));
    }
    run_git_write(root, &["commit", "-m", message])?;
    mutation_report(root, "commit", None, None)
}

/// Throw away one path's unstaged edits.
pub(crate) fn discard(
    environment: Option<&DshEnvironment>,
    path: &str,
) -> Result<GitMutationReport, GitError> {
    discard_in(&repo_root(environment)?, path)
}

/// [discard] against an explicit repository (the testable core).
pub(crate) fn discard_in(root: &Path, path: &str) -> Result<GitMutationReport, GitError> {
    let target = validate_relative(path)?;
    // `--worktree` only: never `--staged`, never a revision, never `--hard`.
    // The worst case is one file's unstaged edits, which is exactly what the
    // confirmation dialog promises. (git 2.23 or newer.)
    run_git_write(root, &["restore", "--worktree", "--", &target])?;
    mutation_report(root, "discard", Some(&target), None)
}

/// Request: stage one path, or everything.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitStageRequest {
    schema_version: u8,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    all: bool,
}

impl GitStageRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == GIT_SCHEMA_VERSION
    }

    pub(crate) fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub(crate) fn all(&self) -> bool {
        self.all
    }
}

/// Request: unstage one path, or the whole index.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitUnstageRequest {
    schema_version: u8,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    all: bool,
}

impl GitUnstageRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == GIT_SCHEMA_VERSION
    }

    pub(crate) fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub(crate) fn all(&self) -> bool {
        self.all
    }
}

/// Request: commit what is staged.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitCommitRequest {
    schema_version: u8,
    message: String,
}

impl GitCommitRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == GIT_SCHEMA_VERSION
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

/// Request: throw away one path's unstaged edits.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GitDiscardRequest {
    schema_version: u8,
    path: String,
}

impl GitDiscardRequest {
    pub(crate) fn is_valid(&self) -> bool {
        self.schema_version == GIT_SCHEMA_VERSION
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
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

    /// The status entry for one path.
    fn entry_of<'a>(entries: &'a [GitEntry], path: &str) -> Option<&'a GitEntry> {
        entries.iter().find(|entry| entry.path == path)
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

    #[test]
    fn stage_then_unstage_round_trips_a_path() {
        let repo = TempRepo::new("stage");
        repo.write("a.txt", "one");

        let staged = stage_in(repo.path(), Some("a.txt"), false).expect("stage");
        assert_eq!(staged.operation, "stage");
        assert_eq!(staged.path.as_deref(), Some("a.txt"));
        assert!(staged.detail.is_none());
        let entry = entry_of(&staged.status.entries, "a.txt").expect("a.txt is in the report");
        assert!(entry.staged && !entry.unstaged && !entry.untracked);

        let unstaged = unstage_in(repo.path(), Some("a.txt"), false).expect("unstage");
        assert_eq!(unstaged.operation, "unstage");
        let entry = entry_of(&unstaged.status.entries, "a.txt").expect("a.txt is still reported");
        assert!(
            !entry.staged && entry.untracked,
            "it went back to untracked"
        );
        assert!(
            repo.path().join("a.txt").exists(),
            "unstaging never touches the worktree"
        );
    }

    #[test]
    fn stage_all_and_unstage_all_cover_every_change() {
        let repo = TempRepo::new("stage-all");
        repo.write("a.txt", "one");
        repo.write("b.txt", "two");

        let staged = stage_in(repo.path(), None, true).expect("stage all");
        assert_eq!(staged.path, None);
        assert_eq!(staged.status.entries.len(), 2);
        assert!(staged.status.entries.iter().all(|entry| entry.staged));

        let cleared = unstage_in(repo.path(), None, true).expect("unstage all");
        assert!(cleared.status.entries.iter().all(|entry| entry.untracked));
    }

    #[test]
    fn unstage_works_before_the_first_commit() {
        // A repository with no commits has no HEAD to restore from: the index
        // reset has to go through rm --cached, and the report says so.
        let repo = TempRepo::new("unborn");
        repo.write("a.txt", "one");
        stage_in(repo.path(), Some("a.txt"), false).expect("stage");

        let cleared =
            unstage_in(repo.path(), Some("a.txt"), false).expect("unstage on an unborn head");
        assert!(
            cleared
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("no commits yet"),
            "the report names the primitive: {:?}",
            cleared.detail
        );
        let entry = entry_of(&cleared.status.entries, "a.txt").expect("a.txt");
        assert!(entry.untracked && !entry.staged);
        assert!(repo.path().join("a.txt").exists());
    }

    #[test]
    fn commit_records_what_was_staged() {
        let repo = TempRepo::new("commit");
        repo.write("a.txt", "one");
        stage_in(repo.path(), Some("a.txt"), false).expect("stage");

        let committed = commit_in(repo.path(), "  first commit  ").expect("commit");
        assert_eq!(committed.operation, "commit");
        assert!(committed.status.clean, "the tree is clean after committing");

        let history = log_in(repo.path(), 10).expect("log");
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].subject, "first commit");
    }

    #[test]
    fn commit_guards_run_before_git_does() {
        let repo = TempRepo::new("commit-guards");
        repo.write("a.txt", "one");

        // Nothing staged.
        let nothing = commit_in(repo.path(), "nope").expect_err("nothing staged");
        assert!(
            matches!(nothing, GitError::Unavailable(_)),
            "got {nothing:?}"
        );
        assert!(nothing.message().contains("nothing staged"));

        // Empty and whitespace-only messages.
        for message in ["", "   ", "\n\t"] {
            assert!(matches!(
                commit_in(repo.path(), message),
                Err(GitError::Malformed(_))
            ));
        }

        // Over-long message.
        let long = "x".repeat(MAX_COMMIT_MESSAGE_BYTES + 1);
        assert!(matches!(
            commit_in(repo.path(), &long),
            Err(GitError::Malformed(_))
        ));

        // None of that created a commit.
        assert!(
            log_in(repo.path(), 10)
                .map(|report| report.entries.is_empty())
                .unwrap_or(true)
        );
    }

    #[test]
    fn discard_restores_the_worktree_and_nothing_else() {
        let repo = TempRepo::new("discard");
        repo.write("a.txt", "one");
        stage_in(repo.path(), Some("a.txt"), false).expect("stage");
        commit_in(repo.path(), "base").expect("commit");

        // Untracked work on two files: one staged change and one worktree change.
        repo.write("a.txt", "two");
        stage_in(repo.path(), Some("a.txt"), false).expect("stage the edit");
        repo.write("a.txt", "three");
        repo.write("b.txt", "keep me");

        let discarded = discard_in(repo.path(), "a.txt").expect("discard");
        assert_eq!(discarded.operation, "discard");
        assert_eq!(
            fs::read_to_string(repo.path().join("a.txt")).expect("read a.txt"),
            "two",
            "discard restores the worktree from the index, not from HEAD"
        );
        let entry = entry_of(&discarded.status.entries, "a.txt").expect("a.txt");
        assert!(entry.staged && !entry.unstaged, "the index is untouched");
        assert!(
            !entry.untracked,
            "and the file is still tracked and staged as modified"
        );
        assert_eq!(
            fs::read_to_string(repo.path().join("b.txt")).expect("read b.txt"),
            "keep me",
            "another path is never in the blast radius"
        );
    }

    #[test]
    fn mutations_refuse_paths_and_shapes_they_do_not_offer() {
        let repo = TempRepo::new("mutations-negative");
        repo.write("a.txt", "one");

        for path in ["../escape", "/abs", "C:/x", "--upload-pack=evil", ""] {
            assert!(
                matches!(
                    stage_in(repo.path(), Some(path), false),
                    Err(GitError::Malformed(_))
                ),
                "stage accepted {path:?}"
            );
            assert!(
                matches!(
                    unstage_in(repo.path(), Some(path), false),
                    Err(GitError::Malformed(_))
                ),
                "unstage accepted {path:?}"
            );
            assert!(
                matches!(discard_in(repo.path(), path), Err(GitError::Malformed(_))),
                "discard accepted {path:?}"
            );
        }

        // A path and "all" at once, and neither of them.
        assert!(matches!(
            stage_in(repo.path(), Some("a.txt"), true),
            Err(GitError::Malformed(_))
        ));
        assert!(matches!(
            stage_in(repo.path(), None, false),
            Err(GitError::Malformed(_))
        ));
        assert!(matches!(
            unstage_in(repo.path(), Some("a.txt"), true),
            Err(GitError::Malformed(_))
        ));
        assert!(matches!(
            unstage_in(repo.path(), None, false),
            Err(GitError::Malformed(_))
        ));
    }
}
