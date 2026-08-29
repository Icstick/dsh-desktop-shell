# Shell UI

**Module ID:** `MOD-SHELL-UI`
**Target milestone:** M1
**Canonical status:** [MOD-SHELL-UI](../../../../tracking/modules/MOD-SHELL-UI.yaml)

## Purpose

提供 Activity Rail、Desktop layout、全局状态与 Surface 切换。

## Owns

- 导航与可访问性
- Desktop-only 状态展示
- Surface selection

## Does not own

- DSH 内部导航/DOM
- 进程与 resource lifecycle

## Inputs

- RuntimeStatus
- Environment selection

## Outputs

- 用户意图对应的结构化 Tauri commands

## Dependencies

- capability contracts

## Interfaces

- `IF-RUNTIME-STATUS`
- `IF-RUNTIME-CONTROL`
- `IF-ENVIRONMENT`
- `IF-DSH-SURFACE-POLICY`
- `IF-DSH-SURFACE-LIFECYCLE`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。

## Implementation entry

- [`ShellApp.tsx`](src/ShellApp.tsx)：Shell layout、surface selection、active Environment 恢复、Attached health evidence、Managed status/start/generation-bound stop，以及 verified binding 驱动的 native Surface mount/status/layout/reload/unmount。Environment restore/save 不自动启动 DSH。
- [`ActivityRail.tsx`](src/ActivityRail.tsx)：可访问导航入口。
- [`ShellApp.test.tsx`](src/ShellApp.test.tsx)：导航、discovery、validation、save、startup restore、Attached probe、Managed controls、policy preview、native lifecycle最小请求、rail hide、error reload、undersized viewport 与无 DOM bridge 证据。
