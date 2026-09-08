# SESSION-20260908-021-STABILIZATION

- Work item: WI-M9-STABILIZATION (0.2.1 stabilization: M6-C / M6-C4 debt + live-daemon-qa CI)
- Module: MOD-SUPERVISOR (daemon authorization chain), MOD-BROWSER-UI (C4), MOD-LOCAL-TRANSPORT (fixed port)
- Agent/human: agent (姐姐) + 妹妹 (user; 拍板「先推进0.2.1」, approval never)
- Branch/worktree: feat/021-stabilization
- Started at: 2026-09-08T04:10:00Z
- Claim expires at: 2026-09-09T05:00:00Z

## Context read

- tracking/CURRENT.md, tracking/project.yaml, tracking/README.md, tracking/templates/*
- docs/roadmap/ROADMAP-V021-PLUS.md (decided 2026-09-08), MILESTONES.md
- crates/daemon/src/server.rs, credential.rs, browser.rs; crates/supervisor/src/broker.rs + broker/agent.rs;
  crates/local-transport/src/lib.rs, server.rs; apps/desktop/src-tauri/src/browser.rs, daemon_client.rs, agent_broker.rs;
  .github/workflows/ci.yml, release.yml; scripts/qa/live-daemon-qa.mjs
- Tracking handoff HANDOFF-20260907-RELEASE-020 (v0.2.0 shipped 09-07; roadmap + WIs committed 4b18042)

## Roadmap (0.2.1 scope, ROADMAP-V021-PLUS.md)

1. M6-C debt: daemon lease revocation on disconnect (LeaseRevocationReason::Disconnect); envelope fixed port; TODO inventory.
2. M6-C4 debt: browser navigation state reporting to the daemon (navigate_session TODO); remaining C4 TODO inventory.
3. live-daemon-qa into CI (sidecar staging pattern already established).

## Actions

(TBD - filled per step)

## Evidence

(TBD - test outputs / gate runs per step)

## Decisions and risks

- WI milestone field = M9 (tracking schema ^M[0-9]+$); actual release = 0.2.1 (roadmap decided 2026-09-08). Same precedent as M9 WI planned rows.
- Disconnect revocation reuses revoke_agent_grants with a reason parameter; activation stays durably marked revoked (reconnect = fresh activation id, unaffected).
- Fixed-port bind touches local-transport public API -> MOD-LOCAL-TRANSPORT rules apply (schema/ADR/fixture before behavior change).

## Handoff

(TBD at session end)
