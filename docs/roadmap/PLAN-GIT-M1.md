# GIT-M1 slice plan (WI-M10-WORKBENCH-GIT)

Scope: read-only git over the environment-linked repository. Human-only, no agent bridge.
Design source: docs/roadmap/DESIGN-WORKBENCH-PHASE1.md ("Git panel"); layout rules:
docs/roadmap/SPEC-WORKBENCH-VISUAL.md.

## Backend (apps/desktop/src-tauri/src/git_panel.rs)

Git CLI, never a library: predictable output, no new dependency surface, and the
fixture story stays trivial. Every invocation goes through one helper that

- runs §git -C <repo root> <args>§ with §std::process::Command§ (no shell),
- sets §GIT_OPTIONAL_LOCKS=0§ so a read never refreshes or locks the index,
- caps stdout (512 KiB) and reports §truncated§ instead of returning unbounded text,
- maps "not a repository" / "git missing" onto UNAVAILABLE with the reason.

The repository is **exactly** the environment's harness repository path; a caller
never supplies a root. A caller may supply a repo-relative path, validated with the
same rules the file manager uses (§..§, absolute, drive/UNC) **plus one git-specific
rule: a leading §-§ is refused** so a path can never become an option.

Commands (all read-only):

- §git_status§ -> branch + detached flag + entries { path, indexStatus,
  worktreeStatus, staged, unstaged, untracked } (porcelain v1, -z, --no-renames,
  untracked-files=all; capped with a truncated flag).
- §git_diff§ -> unified diff text for the worktree or the index (§--cached§), optional
  repo-relative path; capped, with a truncated flag.
- §git_log§ -> up to 200 entries { hash, author, authoredAtUnixMs, subject }.
- §git_branches§ -> local branch names with the current one marked.

## Frontend (features/git-panel-ui)

- A tab strip at the top of the workbench: **Files | Git** (single-document bar stays
  inside the Files tab).
- Git tab: the repository root + branch as a document bar; the left column groups
  entries Staged / Unstaged / Untracked with counts (facet chips, GIT-M1 uses the
  §--wb-diff-*§ tokens for the diff pane); the right pane shows the diff read-only,
  with §+§/§-§ lines tinted and a hint when there is nothing to show.
- Empty and degraded states: "not a git repository" / "git is not installed" are
  explicit, not blank panes.


## As built (2026-09-13)

Frontend landed as three pieces, so neither panel owns the other:

- §features/workbench-ui/src/WorkbenchPanel.tsx§ — the tab host. A real roving-focus
  §role="tablist"§ (Arrow/Home/End move selection and focus, §tabIndex§ follows the
  active tab) over one body; only the active tab is mounted, so switching to Git stops
  the file tree from polling and vice versa. The active tab's subtitle sits at the right
  edge of the strip, which is why both panels dropped their in-panel heading - the page
  header plus the strip already name the surface, and a third "工作台 / Workbench" title
  in the panel body was pure repetition.
- §features/git-panel-ui/src/GitPanel.tsx§ — status groups (Staged / Unstaged /
  Untracked with counts), the repository root + branch in the document bar, the read-only
  diff pane and the two read-only lists the design doc also asks for (recent commits,
  branches) behind disclosures. A partially staged file (porcelain §MM§) is listed in
  both facets rather than silently folded into one.
- §apps/desktop/features/shell-ui/preview/main.tsx§ — the visual preview gained
  workbench fixtures (fake repo, status, diff, log, branches) so the surface can be
  reviewed without a desktop backend. This is the screenshot path for acceptance.

Decisions taken while building:

- The diff pane renders the backend's unified text as inline §<span>§s with the literal
  newlines kept, so copying the diff out of the pane still yields real lines. §+§/§-§ use
  §--wb-diff-added§ / §--wb-diff-removed§ with a §color-mix§ tint derived from the same
  token (no second hardcoded colour); §@@§ hunks take the accent.
- A 512 KiB diff can still be tens of thousands of lines, so rendering stops at 4000
  lines and says so (§git.diff.linesCapped§) instead of quietly shortening the file.
- Selecting an untracked entry asks for no diff at all - git has none - and the pane says
  that rather than showing an empty box.

Follow-up (not this slice): the git panel's left column is 220-320 px, so long
repo-relative paths wrap mid-segment. A resizable column (already in the visual spec) is
the right fix.

## Tests

- Rust: status (untracked -> modified -> staged), diff (worktree and cached), log,
  branches, path validation (../, absolute, leading dash), repo-root resolution,
  non-repository degradation. Fixtures use a real temporary §git init§ repo.
- vitest: the Git tab renders counts and the diff, an empty status shows the
  "clean" state, and a degraded report shows the reason.
- Gates: fmt, clippy -D warnings, cargo test, pnpm check, pnpm test, validate-acl,
  validate-specs, then CI on the branch.

## Explicitly deferred (GIT-M2)

Stage/unstage/commit, destructive verbs (checkout/reset) behind typed
confirmations, per-hunk staging, history rewriting (never in Phase 1), and any
remote/SSH work (0.5.0).
