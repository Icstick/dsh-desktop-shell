# Design: Dev Workbench Phase 1 (0.3.0) - file manager + git panel

Scope: WI-M10-WORKBENCH-FS + WI-M10-WORKBENCH-GIT. Human-only surfaces;
every command goes through the ACL manifest; no agent bridge. Lightweight
editor (view + basic edit), no Monaco. Git is over the CLI (gitoxide stays
a later option).

## Roots and containment (the security core)

- Roots come from the environment catalog (form A): the active environment
  repository path, its dshHome, and its plugin directories, plus the
  workspace root(s). The file manager never browses outside roots.
- Every request carries a root id plus a relative path. Resolution:
  join, canonicalize, then require the canonical result to still be
  prefixed by the canonical root - otherwise fail closed (no TOCTOU on
  the raw string). `..` segments, UNC/device paths and absolute paths in
  the relative field are rejected outright.
- Symlinks: reported as links, never followed for directory expansion
  (prevents cycles and escape-by-link). File reads through links are
  resolved-then-checked like everything else.
- Size caps: tree listing truncated with a "more" marker; files above
  2 MiB open read-only with an explicit banner.

## File manager

- Tree: lazy per-directory listing, directories first then name order,
  hidden entries filtered by default with a toggle. Each root renders as
  a section header (repo / dshHome / plugins).
- Rows carry a runtime badge for the active environment (running /
  stopped / degraded) reusing the existing runtime status, so the user
  sees where DSH is live without leaving the surface.
- Editor: text view with line numbers and a status bar (path, encoding,
  dirty marker). UTF-8 (with BOM detection); non-UTF-8 opens read-only.
- Save: dirty-state confirm, atomic write (temp + rename), and a
  conflict check against the mtime/size recorded on open - if the file
  changed underneath, offer overwrite / save-as / discard instead of
  silently clobbering.

## Git panel

- Backend: `git` CLI invoked with `-C <repo root>`; porcelain-ish output
  parsed in Rust. Rationale: predictable behavior, no new dependency
  surface, easy fixtures in tests. gitoxide reconsidered only if CLI
  parsing proves brittle.
- Read-only views first: status (staged/unstaged/untracked), diff
  (unified, rendered in a read-only editor pane), log (paged), branch
  list with current marker.
- Mutations with explicit confirmation: stage/unstage (file and all),
  commit (message box; empty-message guard), and destructive actions
  (checkout / discard) behind a typed confirmation. No force-push, no
  history rewrite in Phase 1.
- Repository is exactly the environment-linked root; the panel refuses
  to run outside it. A repo with no commits yet still works (status +
  first commit).

## Command surface (ACL-registered, no agent access)

- fs: `fs.list_root`, `fs.read_dir`, `fs.read_file`, `fs.write_file`,
  `fs.stat` (all root-relative, all containment-checked).
- git: `git.status`, `git.diff`, `git.log`, `git.stage`, `git.unstage`,
  `git.commit` (all repo-root-scoped); destructive verbs added only with
  their confirmation flow.
- Schemas for each command join `specs/` with fixtures; the ACL manifest
  grows accordingly (test asserts the manifest covers every command).

## Layout

- Rail entry opens the workbench surface. Inside: left tree pane, right
  editor/diff pane, bottom status bar; git panel is a tab next to the
  file tree (same right-hand pane). Keyboard reachable throughout;
  unavailable/degraded states rendered explicitly (repo missing,
  environment stopped, path escaped).

## Delivery slices

1. FS-M1: roots + tree + read-only text view (no writes yet).
2. FS-M2: edit/save with atomic write and conflict handling.
3. GIT-M1: status/diff/log read-only over the linked repo.
4. GIT-M2: stage/unstage/commit + confirmed destructive verbs.

## Acceptance

- Unit: containment negative matrix (`..`, absolute, UNC, symlink
  escape, root prefix confusion), atomic write, conflict detection.
- Integration: fixture repo for git commands (status/diff/stage/commit
  round-trip); ACL manifest coverage test.
- Visual: keyboard-only walkthrough, empty/degraded/error states, both
  light-on-dark surfaces only (theme is dark-only).
- Gate: cargo + vitest + specs + ACL validation, as every slice.

## Non-goals (explicit)

- No agent access to these commands (agent tooling stays a DSH-plugin
  track). No Monaco. No terminal integration inside the workbench.
No remote/SSH (that is Phase 2, 0.5.0).
