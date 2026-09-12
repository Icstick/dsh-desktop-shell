# Daemon client pending reap — handoff (2026-09-12)

- Work item: WI-M11-DAEMON-CLIENT-PENDING-REAP (status `done`, claim released).
- Landed on `feat/m12-peer-identity` as **a78db59** (cherry-picked from the implementation worktree, which was then removed). Based on 440edff; that branch is not yet on main.
- Why there and not its own branch: WI-M12 slice 4/5 ("daemon + Shell client carrier switch") edits this same file, so one branch removes a rebase nobody needs.
- Touches: `apps/desktop/src-tauri/src/daemon_client.rs` only (+ this WI + this handoff).

## What changed

A caller that times out used to leave its entry in the worker's `pending` table until a reply arrived; a daemon that never answers grew that table for the life of the Process. The leak was documented in a comment at the timeout arm, not fixed.

- `ClientCommand` gains `Abandon { id }`, and its `Invoke.envelope` is now boxed (clippy `large_enum_variant` fires once a second, much smaller variant exists).
- `DaemonClient::invoke` sends the abandon best-effort when `recv_timeout` returns `Timeout`, then returns `DaemonCommandError::Timeout` as before.
- `worker_loop` drains `Abandon` by removing the entry; a late reply for that id then finds nothing and is dropped, exactly as before for an unknown id.
- `DaemonClient` carries `invoke_timeout: Duration` (default `INVOKE_TIMEOUT`) so the timeout path is testable without waiting a minute.

## Verification (reproducible)

```
cd dsh-desktop-shell-wt-pending-reap
$env:CARGO_TARGET_DIR = '<main checkout>\target'   # reuse the dependency cache
cp <main checkout>\apps\desktop\src-tauri\binaries\dsh-desktop-daemon-x86_64-pc-windows-msvc.exe apps\desktop\src-tauri\binaries\   # build.rs needs the sidecar; the dir is gitignored
cargo test -p dsh-desktop-shell --lib      # 172 passed, 0 failed
cargo clippy -p dsh-desktop-shell --all-targets -- -D warnings   # clean
```

- New test: `daemon_client::tests::a_timed_out_invocation_is_abandoned_to_the_worker` asserts the client queues `Invoke` then `Abandon` naming the same id. It does **not** drive `worker_loop` (needs a live `LocalClient`).

## Risks / open ends

1. **Same file as slice 4/5**: that slice switches the daemon and Shell client carrier. The abandon/reap arm is carrier-independent, so keep it when the transport call sites change — do not drop the timeout arm while rewriting them.
2. **rustfmt toolchain difference**: `cargo fmt` on rustc 1.98 rewrote four committed files under `crates/local-transport/`. Those edits were reverted here, but the repo's `fmt` gate will fail on that branch as it stands — worth resolving on the ADR-0022 branch, not here.
3. The worker-side drop is covered by construction (exhaustive match) plus the existing late-reply path, not by an end-to-end test; noted in the WI rather than claimed.

## Next step (exact)

Nothing pending for this WI. When WI-M12 slice 4/5 lands: keep the abandon/reap arm, and re-run `cargo test -p dsh-desktop-shell --lib` + `cargo clippy -p dsh-desktop-shell --all-targets -- -D warnings` on the merged result.
