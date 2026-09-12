# Spike: local-transport peer identity on Windows Named Pipes (2026-09-12)

Input to ADR-0022 (proposed): its decision 2 asks for a spike before any
carrier migration. This is the Windows half, executed on this machine;
the Unix half is planned but not verified locally (see Limitations).

## What was verified

Spike project (throwaway, outside the repository):
`D:\DSH_workspace\.spike-peer-identity` - zero dependencies, raw Win32
FFI only. Reproduce with `cargo run -- server`.

1. **Named Pipe + the local-transport framing model fit.** The server
   creates the pipe with `CreateNamedPipeW`, the client opens it
   with plain `CreateFileW` (filesystem path `\\.\pipe\<name>`), and a
   u32-LE length-prefixed frame round-trips byte-exactly (23-byte JSON
   payload). The framing codec in `crates/local-transport` is already
   carrier-agnostic (`Read + Write`), so the frame layer needs no changes.
2. **Kernel-provided peer identity works.** `GetNamedPipeClientProcessId`
   returned the real client PID (35484, parent was 33156), and
   `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` +
   `QueryFullProcessImageNameW` resolved it to the client image path.
3. **The identity discriminates processes, not just connections.** The
   client was a separate child process; the server could compare its own
   image path against the peer image path.

Raw evidence (server run output):

```
[server] pipe: \\.\pipe\dsh-peer-spike-33156
[server] self pid: 33156 path: Some("...\target\debug\peer-id-spike.exe")
[server] GetNamedPipeClientProcessId ok=1 peer_pid=35484
[server] peer image path: Some("...\target\debug\peer-id-spike.exe")
[server] frame received: 23 bytes: {"hello":"from-client"}
RESULT peer_pid_resolved_and_distinct=true (self=33156, peer=35484)
RESULT peer_image_path_resolved=true
```

## Limitations (honest)

- **Unix half not verified locally** (Windows machine). Planned path:
  `UnixListener` + `getsockopt(SO_PEERCRED)` (Linux; via libc/nix)
  or `getpeereid` / `LOCAL_PEERPID` (macOS). Must be verified on the
  CI matrix (ubuntu + macos runners) before any carrier decision lands.
- The spike proves mechanism, not policy: image-path comparison rules
  (below), downgrade semantics and the loopback-TCP fallback story are
  ADR-0022 implementation-detail decisions.
- Same-user threat model only: peer identity tells you *which process*
  connected, not that its user intent is genuine; the gesture gate
  (ADR-0023) remains the layer for intent.

## Expected Shell path: where it comes from (proposal)

1. Explicit override first: `DSH_SHELL_EXPECTED_PATH` (tests, unusual
   installs).
2. Otherwise derive from the daemon binary layout: bundled sidecar
   (`<install>\dsh-desktop-shell.exe` next to the daemon) or dev layout
   (`target\debug|release\dsh-desktop-shell.exe`).
3. If neither resolves: **fail closed** - no `shell_control` authority for any
   connection (broker-relaxed path stays unavailable), with an
   auditable reason. Never fall back to "accept any peer".

## Impact on ADR-0022

- The B2 direction is mechanically viable on Windows with no new
  dependencies in the transport crate and no framing changes.
- Implementation should start by introducing the carrier split in
  `dsh-local-transport` (TCP today, Named Pipe/UDS next), keeping the
  existing TCP path as an explicit, reported degradation.
- Next step after this spike: decide whether to accept ADR-0022 and open
  the implementation WI (including the Unix spike on CI runners).
