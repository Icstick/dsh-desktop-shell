# Windows Real-DSH WebView2 Native Smoke — 2026-08-28

- **Work item**: [WI-M1-SHELL](../../tracking/work-items/WI-M1-SHELL.yaml) / [ADR-0011](../decisions/ADR-0011-platform-gated-native-dsh-surface.md) / [ADR-0012](../decisions/ADR-0012-authenticated-managed-web-bootstrap.md)
- **Gates exercised**: AC-WEB-005 (verified current-generation binding only), AC-WEB-006 (permission deny before load, exact-origin navigation, cross-origin/popup/download/permission deny), ADR-0012 token-exchange/clean-root/redaction gates, lifecycle smoke (resize/hide/show/reload/unmount) and credential-lifetime-on-stop.
- **Machine**: Windows, WebView2 Runtime 151.0.4129.107, desktop shell debug build
  `target/debug/dsh-desktop-shell.exe`, Tauri 2.11.5, Rust 1.98.0, Node 24.15.0.
- **Real DSH under test**: user-owned checkout `D:\deepseek-harness` prebuilt entry
  `apps\cli\lib\bin.js`, launched by the Supervisor with
  `node <entry> web --host 127.0.0.1 --port 0 --no-open`.
- **Driver**: [apps/desktop/scripts/smoke-native.mjs](../../apps/desktop/scripts/smoke-native.mjs) over the
  Tauri IPC seam (shell WebView) and Chrome DevTools Protocol (child WebView).

## Result

**26/26 checks passed.** Raw evidence:
[docs/testing/evidence/SMOKE-20260828-WEBVIEW2-NATIVE.json](evidence/SMOKE-20260828-WEBVIEW2-NATIVE.json)

| Area | Verified |
|---|---|
| Shell bootstrap | IPC seam available; initial snapshot `unconfigured`, no active environment |
| ADR-0012 recipe | Repository recipe validates; launch argv is exactly `node <entry> web --host 127.0.0.1 --port 0 --no-open` (no shell/package manager) |
| Persistence | `save_environment` persisted catalog `environment-catalog-v1.json` with `activeEnvironmentId` |
| Managed runtime | Start accepted; state `healthy` with `owned_generation_output_and_tcp` verified endpoint on 127.0.0.1 |
| Token hygiene | Runtime report and Surface status contain no `token=`/query/bootstrap URL |
| Native mount | Surface `ready` on `platform: windows` with exact verified origin; child WebView at clean root `http://127.0.0.1:<port>/` (no query) |
| Real DSH UI | Child WebView rendered the DSH UI (`DSH 本地构建`, workspace picker, composer) after token exchange and clean-root redirect |
| Layout | Hide → `hidden`; show/resize → `ready`, child inner size follows bounds (1200×800) |
| Reload | Returns to `ready` on the same exact origin |
| Cross-origin | `Page.navigate` to https://example.com/ denied; page stays on exact origin |
| Popup/new window | `window.open` returns `null`; no extra CDP target |
| Download | `<a download>` click produces no navigation and no extra target |
| Permissions | Notification → `denied`; Geolocation → denied (no prompt) |
| Privileged IPC | Child WebView `invoke` rejected by ACL (`not allowed on window "shell", webview "dsh-surface"`); no privileged native bridge |
| Unmount | Explicit unmount → `unmounted`, child WebView target closed |
| Stop | Runtime → `stopped`, endpoint released, `processOwnership: none`; no `node.exe` with `bin.js` remained (process-tree cleanup re-checked via Win32_Process) |
| Credential lifetime | After stop, status/mount fail closed: `not a verified current-generation Surface binding`; no stale-binding reuse |

## Notes and residual items

- `stopDisposition` was `forced`: the DSH CLI did not exit on the graceful signal, so the
  Supervisor's Job-Object cleanup terminated the tree. Credential/endpoint were dropped either
  way; this matches the fail-safe design (unit tests cover both dispositions).
- macOS/Linux `unsupported_platform` evidence: this session is Windows-only. The non-Windows
  branch of `mount_surface` now shares a single tested helper
  (`unsupported_platform_record`) and, because `mount_windows_surface` and the WebView2 deny
  hooks are `#[cfg(windows)]`, no WebView-creation code exists in a macOS/Linux binary
  (compile-time gating). A real macOS/Linux target-host run remains the final evidence for
  AC-WEB-007 and must not be claimed from this session.
- Environment used: `managed-real` (label "Real DSH checkout smoke (advisory 2026-08-28)"),
  `nodePath` `C:\Program Files\nodejs\node.exe`, `dshHome` `C:\Users\ZOOT\.dsh`.
