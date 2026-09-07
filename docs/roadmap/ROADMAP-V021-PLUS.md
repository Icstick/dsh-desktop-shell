# Roadmap v0.2.1 → v0.5.0 (decided 2026-09-08)

User decisions: dev workbench (file manager + git panel) is the next
feature track after the 0.2.1 stabilization; editor starts lightweight
(view + basic edit); environment-context linking (form A) first; 0.2.1
stabilization debt comes first.

## 0.2.1 · Stabilization (engineering debt, no UX features)

- M6-C debt: daemon lease revocation on disconnect (broker revoke with
  LeaseRevocationReason::Disconnect), envelope fixed port, + inventory of
  the remaining M6-C TODO(s) in code.
- M6-C4 debt: browser navigation state reporting to the daemon
  (navigate_session TODO), remaining C4 TODO(s).
- live-daemon-qa into CI (sidecar staging pattern established).

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

## Deferred backlog

- Timer UI (WI-M9-TIMER-UI): design pass still needed (modes/presets).
- Terminal automation for agents (M3 debt; agent-safety design).
- Concurrent multi-profile B2 (M10+).
- Artwork commercial authorization (awaiting upstream reply; not blocking).
