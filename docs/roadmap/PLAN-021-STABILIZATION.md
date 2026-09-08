# 0.2.1 stabilization plan (WI-M9-STABILIZATION, decided 2026-09-08)

Engineering-debt release after v0.2.0: no UX features. Source of truth:
ROADMAP-V021-PLUS.md. Branch: feat/021-stabilization.

## Work order

### 1. M6-C lease revocation on disconnect (smallest, self-contained)

- Broker: `revoke_agent_grants(activation_id, reason)` - reason becomes a
  parameter; existing callers (desktop human takeover x2) pass
  `HumanTakeover`, daemon disconnect passes `Disconnect`. Activation stays
  durably marked revoked in both cases (a disconnected activation result
  must never be reused; a reconnect negotiates a fresh activation id).
- Daemon: `serve_connection` teardown revokes every activation the
  connection negotiated (replaces the "TTL bounds them until then" gap).
- Tests: broker unit (Disconnect reason + idempotence + replay refusal) +
  daemon integration (connect -> Hello -> disconnect -> leases revoked).
- Files: crates/supervisor/src/broker/agent.rs (+tests), agent_broker.rs +
  browser.rs call sites (apps/desktop), crates/daemon/src/server.rs.

### 2. M6-C envelope fixed port

- Current: local-transport binds a random loopback port; the daemon owns
  the fixed claim port 37771 (single-instance + presence probe) and the
  real port travels in the credential file.
- Target: envelope server binds the fixed 37771 loopback port directly.
  Design questions to settle in the plan/ADR step before code:
  1. transport API: add `LocalServer::bind_on(addr, limits)` (public
     behavior change -> MOD-LOCAL-TRANSPORT rules: schema/ADR/fixture)?
  2. single-instance guard: envelope bind failure becomes the single-
     instance signal; claim guard listener removed or kept?
  3. Shell discovery: probe_claim_port semantics (plain TCP connect) vs
     connecting to the envelope listener (unauthenticated connection
     noise / concurrency-slot cost)?
  4. credential file port field: keep for schema compatibility?
- Tests affected: daemon tests/common, credential_reissue, split_brain,
  daemon_client in-process daemons (claim listener binds are everywhere).

### 3. M6-C4 render-side navigation state reporting

- Current: daemon owns the session registry (M6-C3); render-side
  navigation happens in the Shell WebView and is NOT reported back
  (TODO at apps/desktop/src-tauri/src/browser.rs:634/712), so the daemon
  state authority drifts on user navigation (link clicks, redirects).
- Target: accepted render-side navigations report (session id + url) to
  the daemon so its registry state stays current; decide whether the
  daemon also gains a `browser.navigate` envelope method (daemon-initiated
  navigation) or only a report path.
- Contract-first: browser-capability.schema.json + fixtures + desktop
  contracts.ts before implementation; IF-BROWSER / MOD-BROWSER-PROVIDER
  updates.

### 4. M6-C/M6-C4 TODO inventory

- Code markers: server.rs:36/39/310 (M6-C), browser.rs:634/712 (M6-C4),
  daemon_client.rs:28 (M6-C context), credential.rs:33 (port doc).
- After 1-3 land, remaining markers get owners or move to the deferred
  backlog in ROADMAP-V021-PLUS.md.

### 5. live-daemon-qa CI gap

- Current: ci.yml live-qa-windows job runs live-daemon-qa.mjs +
  live-m7-qa.mjs after debug build (sidecar staging fixed 09-07); no
  evidence upload; job status on main pushes unverified for this release.
- Gap: upload qa evidence artifacts + verify the job is green on main
  after this branch merges (evidence = job run link in the WI).

## Gates

- cargo fmt --check; cargo clippy --workspace --all-targets -- -D warnings;
  cargo test --workspace -- --test-threads=1
- desktop: pnpm check + pnpm test; node scripts/validate-specs.mjs
- Live: daemon integration tests per step; GUI verification only where a
  behavior is user-visible (C4 report path can be verified via the
  browser.list/status drift test instead of GUI).
