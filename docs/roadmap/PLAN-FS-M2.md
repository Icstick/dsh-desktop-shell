# FS-M2 slice plan (WI-M10-WORKBENCH-FS)

Scope: edit + save over the FS-M1 surface. Human-only, same containment core.
Design source: docs/roadmap/DESIGN-WORKBENCH-PHASE1.md ("Editor" + "Save" + slice list).

## Command surface (ACL-registered, no agent access)

- `fs_stat` (rootId, relativePath) -> { size, modifiedUnixMs, editable, reason }.
  Recorded on open as the conflict baseline, compared on save.
- `fs_write_file` (rootId, relativePath, content, expectedSize, expectedModifiedUnixMs, force) -> new stat.
  - Containment identical to the read path: root-relative, canonical prefix check, the parent must exist and stay
    inside the root.
  - `force = false` (default): both baselines must match what is on disk, otherwise `CONFLICT` so the UI can offer
    overwrite / discard.
  - `force = true`: deliberate overwrite after an explicit user confirmation; never a way around containment.
  - Refuses what the read path marks read-only: above the 2 MiB editing limit, non-UTF-8, non-regular files
    (a symlink target is re-checked after canonicalization).
  - Atomic write: temp file in the same directory, flushed, then renamed over the target; the temp file is removed
    on any failure.

## Frontend (features/file-manager-ui)

- Editor pane: textarea (no Monaco), status bar with path / encoding / dirty marker, save enabled only when dirty and
  editable, Ctrl+S / Cmd+S binding, Escape reverts after a confirm.
- Conflict flow: on `CONFLICT` offer overwrite / discard / keep editing; never silently clobber.
- Read-only files keep the FS-M1 banner, with save disabled and the reason in the tooltip.
- Unload guard: warn when navigating away with unsaved changes.

## Tests

- Rust: atomic write round-trip, conflict on mtime and on size, force overwrite, refusal of oversized/binary targets,
  the containment negative matrix on the write path, temp-file cleanup on failure.
- vitest: dirty marker, save call shape, conflict dialog (overwrite / discard), read-only disables save.
- Gates unchanged: fmt, clippy -D warnings, cargo test, pnpm check, pnpm test, validate-acl, validate-specs, then CI.

## Visual language (decision 2026-09-13)

Reference consulted: the Hermes desktop theming model (skill doc `themes.md`) - one
semantic palette drives CLI, TUI and GUI, with role-based keys (background, accent,
text/label/dim/border, ok/warn/error, `status_bar_{text,good,warn,bad,critical}`,
`diff_added/diff_removed` incl. word-level, `syntax_*`), a WCAG AA contrast floor
(~4.5:1 against the background) and recognizable green/amber/red.

What we take (structure, not identity):

- A semantic token layer for the workbench instead of ad-hoc colors: `--wb-surface`,
  `--wb-surface-raised`, `--wb-border`, `--wb-text`, `--wb-text-muted`, `--wb-accent`,
  `--wb-ok` / `--wb-warn` / `--wb-error`. Components stop naming raw colors, so a future
  theme can recolor the whole surface without touching the panel.
- The editor status bar uses the four-level status semantics (ok / warn / bad / critical)
  for encoding, dirty, over-limit and conflict states - the same vocabulary the runtime
  badge already uses for its states.
- `--diff-added` / `--diff-removed` (`+` word-level variants) are reserved now and consumed by
  GIT-M1's diff view, so the token names are settled before the second consumer exists.
- Contrast: every token pair used for text must clear ~4.5:1 against its surface; ok/warn/
  error stay recognizable.

### Hermes desktop, verified visually (2026-09-13, user launched it, cua screenshot)

Observed regions, second reading (1540x1028 window; read directly from the capture -
see the tooling note below):

- **Window chrome**: traffic-light controls top-left, a thin row of utility icons
  top-right; both compact (~32-40 px).
- **Left sidebar** (~200-220 px): "New session" with an inline shortcut hint, then
  "Capabilities", "Messaging", "Artifacts", then a **search field** ("Search
  sessions…"); a **"PINNED"** section header with an empty-state hint; a
  **"PROJECTS"** section header whose entries form a disclosure tree - "Main"
  expanded with three children (the active one highlighted with a soft pill),
  "DSH" / "dsh-work" / "nvim" collapsed.
- **Main area**: a **tab strip** across the top (two document tabs, the active one
  underlined in blue) with a "+" to open another, then the document body - rendered
  Markdown with headings, bold runs and inline code. It is a session/transcript
  reader, NOT a file editor.
- **Bottom strip**: attach "+", the input ("Send a follow-up"), a **model selector**
  ("Deepseek V4 Flash"), two small icon buttons and a round send button - input and
  state in a single strip.
- **Visual language**: near-white surfaces (#f0f0f8), white content blocks, hairline
  separators, the accent blue reserved for the active state, uppercase
  letter-spaced section headers, thin monochrome icons, sidebar rows ~28-32 px.
  LIGHT theme - the opposite of this Shell's dark identity.

No file tree, editor, diff or terminal surface is present, so the transferable part
is the **chrome grammar** (grouping, disclosure, tabs, one-strip status), not a
file-editor reference.

Tooling note (2026-09-13): the vision-router tools (vision_describe / vision_ocr / …)
are absent from this model's tool surface, and the earlier attempt died against a
rate-limited vision backend. The capture was therefore read with `read_image`, which
hands the image to the model itself; the PNG is written to disk first so the
conversation never carries a multi-megabyte base64 payload.

Adopted for FS-M2 (structure only):

- **Bottom status strip for the editor pane** (path · encoding · size · dirty ·
  conflict level) instead of only a header line: it matches both this family's
  grammar (Hermes puts its composer + status at the bottom) and editor convention.
- **Compact toolbar row at the top of the tree** (~32 px): refresh, show-hidden,
  collapse-all - small, flat, icon+label, the same weight class as Hermes' chrome.
- **Root groups get a header with an expander and an entry count**, mirroring the
  pinned `PROJECTS` group, instead of the bare label button FS-M1 shipped.
- **A filter box for the tree** (the closest sibling is Hermes' "Search sessions…"):
  **implemented in FS-M2 rather than deferred** - a repository tree without a filter
  stops being usable past a few hundred entries, and it is a pure client-side filter
  over the already-loaded listings.
- **Selection is a soft pill, not a border**, and the accent is reserved for the
  active element - the restraint our dark theme should mirror.
- **Inline shortcut hints** on actions (Hermes prints the shortcut next to "New
  session"); the workbench shows Ctrl/Cmd+S on save and the collapse-all key.
- **A tab strip over the right pane** so the open document's identity is visible at
  the top. FS-M2 ships a single-document strip (name + dirty dot); multi-document
  tabs stay deferred, but the strip is where they will land.

Not adopted (unchanged decision): the palette, typography, icon language, the YAML
skin engine, and any pixel-level imitation of its layout - this Shell has its own
dark theme and artwork, and the two identities must not be spliced.

What we deliberately do NOT take:

- The palette, typography, icon language or branding (desktop-shell has its own theme and
  artwork; two identities spliced together would be worse than either).
- The YAML skin engine: Hermes needs it because one skin drives three surfaces; this Shell
  has one surface, so a CSS token layer is the right size.
- Any layout transplant. Hermes desktop's own layout could not be read here: its sources are
  not on this machine and its packaged build disables the CDP port (the skill forbids
  relaunching the user's app to obtain one). If we later want a structural comparison,
  the options are the source checkout or an isolated instance, not the running app.

### Consolidated chrome grammar (three more surfaces read 2026-09-13)

Surfaces inspected besides the chat view: **Capabilities** (Skills / Tools / MCP / Browse Hub),
**Messaging** (integration list + detail form), **Artifacts** (facet chips + card grid + table).
All three are *master-detail* layouts and reuse the same primitives:

| Observed pattern | Decision for the workbench |
|---|---|
| Three columns: nav rail sidebar \| item list \| detail pane | FS-M1/M2 already ship tree \| view; keep two columns, no third |
| List rows carry **title + muted subtitle + chip + right-aligned control** | Adopt for tree rows: name + size right-aligned, plus a kind chip for links |
| **Search/filter field pinned at the top of the list column** ("Try 'matrix'") | Adopt: tree filter at the top of the tree column (with a placeholder that teaches, e.g. "Try 'crates'") |
| **Facet chips with counts** ("All 18 / Images 3 / Files 107 / Links 992") | Defer to GIT-M1 (All / Staged / Modified / Untracked with counts) |
| **Uppercase small-caps section labels** in the detail pane ("REQUIRED" / "RECOMMENDED") | Adopt for the file-metadata block (PATH / TYPE / SIZE) and the git commit composer |
| **Status chips in the detail header** ("Disabled" / "Needs setup") | Adopt: the editor header carries encoding / read-only / conflict chips - same semantic vocabulary as the four-level status bar |
| **Primary action anchored bottom-right** ("Save changes") | Adopt: Save lives at the right end of the bottom status strip |
| **Table with monospace paths, ellipsis truncation, footer pagination** ("1-100 of 1000 items") | Adopt as the shape of the tree's truncation: "1-2000 of N" + load-more; the same table shape returns for the GIT-M1 file list |
| **Inline shortcut hints** next to actions | Adopt (Ctrl/Cmd+S on Save, the collapse-all chord on the toolbar) |
| **Source/facet chips row above the list** ("Official (N) / GitHub / Well-known / Direct URL …") | Option for the workbench: roots as chips (repo / dsh-home / cwd) above the tree, so switching root never needs a scroll; settle this with the user before building the tree header |
| **Two-line rows**: bold name + chip + muted one-liner + right-aligned actions ("Preview / Install") | Adopt: tree rows keep name + right-aligned size, and the *selected* row gains a muted second line with its root-relative path |
| **Centered empty-state copy that teaches** ("Search the hub to browse installable skills…") | Adopt: centre the view pane's and the empty tree's hint (FS-M1 shipped it left-aligned) |
| **Section label + right-aligned meta** ("Featured skills … Update installed") | Adopt: the tree column header pairs "Roots" with a right-aligned entry count / refresh time |
| Palette, typography, icon set, YAML skin engine | Not adopted (this Shell has its own dark identity) |

This table is a reference for FS-M2's UI half and for GIT-M1; it is deliberately about
*structure* - nothing here licenses copying Hermes' visual identity.

## Explicitly deferred

- Save-as to a new path (needs a root-scoped picker), multi-file tabs, syntax highlighting, file watching
  (re-read on focus only).
