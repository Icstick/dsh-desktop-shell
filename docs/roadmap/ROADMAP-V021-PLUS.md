# Roadmap v0.2.1 → v0.5.0 (decided 2026-09-08)

User decisions: dev workbench (file manager + git panel) is the next
feature track after the 0.2.1 stabilization; editor starts lightweight
(view + basic edit); environment-context linking (form A) first; 0.2.1
stabilization debt comes first.

## 0.2.1 · Stabilization (engineering debt, no UX features)

- M6-C debt (DONE 2026-09-08): daemon lease revocation on disconnect
  (revoke_agent_grants reason parameter; Disconnect), envelope fixed
  port 37771 (LocalServer::bind_on; single instance + probe + connect in
  one), TODO inventory closed with the fixes.
- M6-C4 debt (DONE 2026-09-08): render-side navigation state reporting
  to the daemon - browser.navigate / browser.load-failed envelope
  methods, provider record_navigation (terminal ready state with the
  final url), desktop URL-aware mirror + out-of-line fail-open reports.
- live-daemon-qa into CI (job exists; evidence upload added so every
  main-push run keeps its QA evidence).

## 0.3.0 · Dev workbench Phase 1 (WI-M10-WORKBENCH-FS + -GIT)

Human-only surfaces; all commands go through the existing ACL manifest;
no agent bridge (agent tooling stays a DSH-plugin track, deferred).

- File manager: directory tree, text view + lightweight edit (CodeMirror
  class, no Monaco), save; environment linking: current env repo path /
  dshHome / plugin dirs quick-open; DSH runtime status awareness.
- Git panel: status / diff / stage / commit / log / branch over the
  linked repo (UI over gitoxide or the git CLI).

## 0.4.0 · Startup rollback (WI-M9-STARTUP-ROLLBACK)

ADR first: trigger definition, last-good bookkeeping, dirty-tree policy,
profile/plugin snapshot strategy, budget, UX surfacing.

## 0.5.0 · Workbench Phase 2 (WI-M10-WORKBENCH-SSH)

- SSH connection manager (reuses the terminal surface for remote PTYs),
  remote file browsing via sftp later.

## Priority queue (decided 2026-09-12, after the 0.2.1 merge)

Ordered by user-visible pain vs dependency depth; the workbench track
(0.3.0 -> 0.5.0) stays the main line, everything below slots between or
parallel to it.

1. **ADR-0022 peer-identity spike** (proposed; Windows Named Pipe + Unix
   UDS feasibility, expected-Shell-path rule) - small, unblocks the only
   path that can actually close H-2; schedule before or alongside 0.3.0.
2. **WI-M9-SAFE-MODE-RECOVERY** (proposed) - isolated recovery profile
   that never mutates the normal one; low-cost precursor to the 0.4.0
   startup rollback theme (ADR there can build on it).
3. **WI-M9-PLUGIN-FAULT-ATTRIBUTION** (proposed) - evidence chain from
   symptom to responsible plugin; pairs with the existing diagnostics
   surface, no new trust boundary.
4. **WI-M11-USER-GESTURE-GATE** (proposed; ADR-0023 accepted) - wire the
   dormant approval_required into a real USER_GESTURE_REQUIRED dispatch
   gate; security review required at implementation time.
5. **WI-M10-MULTI-BACKEND-FLEET** (proposed) - mixed local/remote session
   list and self-healing tunnels; depends on the SSH/tunnel groundwork of
   the 0.5.0 workbench Phase 2, so it follows it.

Backlog rule: proposed WIs are not startable until claimed with a branch
and evidence plan; the ordering above is a queue, not a commitment.

## Deferred backlog

- Browser C4 remainder: daemon-initiated navigate/snapshot envelope
  methods, handover re-attach flow after a Shell restart (0.2.1 shipped
  the render->daemon navigation sync half).
- Timer UI (WI-M9-TIMER-UI): design pass still needed (modes/presets).
- Terminal automation for agents (M3 debt; agent-safety design).
- Concurrent multi-profile B2 (M10+).
- Artwork commercial authorization (awaiting upstream reply; not blocking).
