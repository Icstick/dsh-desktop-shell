# GIT-M2 slice plan (WI-M10-WORKBENCH-GIT)

Scope: the mutating half of the git panel - stage / unstage / commit, plus the one
destructive verb the work item names, behind a typed confirmation. Human-only; no agent
bridge. Design source: docs/roadmap/DESIGN-WORKBENCH-PHASE1.md ("Git panel", slice 4);
layout rules: docs/roadmap/SPEC-WORKBENCH-VISUAL.md.

## Backend (apps/desktop/src-tauri/src/git_panel.rs)

Four new commands, all repo-root-scoped like the read-only four: the repository is still
exactly the environment's harness repository path, and a caller-supplied path is still
validated by §validate_relative§ (no §..§, no absolute/drive/UNC, **no leading §-§**).

- §git_stage§ { path?, all? } -> §git add -A -- <path>§ (or §git add -A§ for all).
- §git_unstage§ { path?, all? } -> §git restore --staged -- <path>§ (or §git restore --staged .§).
  **Unborn HEAD**: a repository with no commits has no HEAD to restore from, so the
  primitive is §git rm --cached -r -- <path>§ (and §git rm --cached -r -- .§) instead. The
  command picks the primitive from §git rev-parse --verify HEAD§ and says which it used in
  the report, so the UI never claims an operation it did not perform.
- §git_commit§ { message } -> §git commit -m <message>§. Guards: the message is trimmed and
  must be non-empty and within 8000 bytes; something must actually be staged. Both guards
  are typed (§MALFORMED§ / §UNAVAILABLE§ with a readable reason) and run **before** git is
  invoked - the UI cannot hand git a no-op commit and then report success.
- §git_discard§ { path } -> §git restore --worktree -- <path>§, or §git checkout -- <path>§
  on a git too old for §restore§. This is the destructive one: it throws away uncommitted
  worktree changes for exactly one path. It never takes §--hard§, never §-f§, never a
  revision, and never touches the index - so it can only ever lose one file's unstaged
  edits, which is the blast radius the UI describes.

Every mutation reports the resulting status, so the UI never has to guess: the report
carries the fresh §GitStatusReport§ (the same shape §git_status§ returns).

## Frontend (features/git-panel-ui)

- Group headers gain "stage all" / "unstage all"; each entry row gains a narrow action
  ("stage" / "unstage" / "discard") that is a real button, keyboard reachable, and only
  renders where the operation means something.
- A commit box under the diff pane: message textarea + Commit, disabled unless something
  is staged and the message is non-empty; §Ctrl/Cmd+Enter§ commits. The count of staged
  entries is on the button.
- Discard opens the typed-confirmation dialog: the confirm button stays disabled until the
  user types the shown word (zh "放弃" / en "DISCARD"). Nothing in this panel destroys work
  on a single click.
- After any mutation the panel reloads the status and keeps the current selection when that
  path still has changes; every failure renders the backend message verbatim in role=alert.
- The panel stops advertising itself as read-only: the docbar chip and the toolbar hint
  change with what the panel can now do.

## Tests

- Rust, on a real temporary repository: stage -> status shows staged; unstage -> back to
  unstaged; **unstage on an unborn HEAD** (the §rm --cached§ path); commit -> appears in the
  log and the status goes clean; commit with an empty message and with nothing staged both
  refuse **without** invoking git; discard restores a modified file and refuses a path
  outside the repository (the shared validate_relative matrix already covers the shapes).
- vitest: the action buttons issue the right request, the commit box gating (nothing
  staged / empty message / both satisfied), the typed-confirmation gate, and a failure
  surfacing the backend message.
- Gates: fmt, clippy -D warnings, cargo test, pnpm check, pnpm test, validate-acl,
  validate-specs, CI on the branch.

## As built (2026-09-13)

- Whole-tree actions sit on the **改动 / Changes** header (stage all / unstage all), not
  on each group header: the backend takes a path or everything, so a per-group button
  would have promised a scope it cannot express.
- **Discard** is available on staged and unstaged entries but never on an untracked one
  (git has nothing to restore for those). The confirmation word is the localized
  §git.discard.word§ (zh "放弃" / en "DISCARD").
- The panel no longer advertises itself as read-only: the docbar chip is gone and the
  toolbar hint says staging and committing happen here.
- The commit box belongs to the index, not to the selected file, so it lives outside the
  diff's selection branch and stays put while the diff scrolls.
- Two things the first screenshot caught: a disabled action kept the weight of a live one
  (no §:disabled§ rule existed for §.git-panel__action§), and two row actions left the path
  column too narrow, so the column now runs 240-360 px and the row actions use a smaller
  type size.

## Explicitly deferred

Per-hunk staging, §--amend§, branch switching / checkout of a revision, stash, remote and
SSH work (0.5.0), and history rewriting (never in Phase 1).
