# Harness Surface

**Module ID:** `MOD-HARNESS-SURFACE`
**Target milestone:** M1
**Canonical status:** [MOD-HARNESS-SURFACE](../../../../tracking/modules/MOD-HARNESS-SURFACE.yaml)

## Purpose

承载原版 DSH Web UI，并提供 loading、error、reconnect 与 route hint 恢复。

## Owns

- native child WebView slot 与 lifecycle presentation
- origin/navigation policy
- reconnect overlay

## Does not own

- DOM injection
- renderer fork
- native bridge

## Inputs

- healthy endpoint
- generation
- route hint

## Outputs

- surface state, fail-closed policy preview and user-visible diagnostics

## Dependencies

- shell-ui
- supervisor status

## Interfaces

- `IF-RUNTIME-STATUS`
- `IF-DSH-SURFACE-POLICY`
- `IF-DSH-SURFACE-LIFECYCLE`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。

## Implementation entry

- [`HarnessSurface.tsx`](src/HarnessSurface.tsx)：测量可视 slot、呈现 loading/ready/error/unsupported/minimum-bounds 状态，并提供显式 generation-bound retry；它不在 DOM 创建 iframe/webview、不注入页面，也不接收 endpoint/URL。
- [`dsh_surface.rs`](../../src-tauri/src/dsh_surface.rs)：消费 Supervisor verified binding，创建 fixed-label Windows child WebView，安装 navigation/new-window/download/permission deny hook，并实现 mount/status/layout/reload/unmount；非 Windows 在创建前 fail closed。
- [`dsh_surface_policy.rs`](../../src-tauri/src/dsh_surface_policy.rs)：从 persisted fixed-loopback Environment 派生 policy，并以 sanitized decision 评估 same-origin、external、loopback mismatch、credential/scheme、popup/download/permission。
- [`ShellApp.test.tsx`](../shell-ui/src/ShellApp.test.tsx)：验证最小 lifecycle request、rail 隐藏、generation reload、undersized viewport gate、无 iframe/webview/script DOM 与 policy evidence。

## Native lifecycle contract

`IF-DSH-SURFACE-LIFECYCLE` 定义 generation-bound mount/status/layout/reload/unmount。caller 只提交 Environment ID、expected generation 与 logical bounds/visibility；backend 从 retained Managed runtime 派生 verified origin并固定 label `dsh-surface`。ADR-0011 将 M1 创建行为限制在 Windows；其他平台显式 `unsupported_platform` 且不创建 WebView。

当前实现与 automated evidence 已进入 review；Windows 对真实 user-owned DSH 的 WebView2 smoke/negative matrix 仍是支持声明前的独立门禁。
