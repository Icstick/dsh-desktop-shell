# Runtime Diagnostics

**Module ID:** `MOD-RUNTIME-DIAGNOSTICS`
**Target milestone:** M2
**Canonical status:** [MOD-RUNTIME-DIAGNOSTICS](../../../../tracking/modules/MOD-RUNTIME-DIAGNOSTICS.yaml)

## Purpose

展示 lifecycle、ownership、generation、health、错误与脱敏日志。

## Owns

- status view
- diagnostic export UX
- safe-stop recovery entry

## Does not own

- 解析 raw session
- 自动修复 Profile

## Inputs

- RuntimeStatus
- redacted events

## Outputs

- human-readable diagnosis and export request

## Dependencies

- supervisor
- log redaction

## Interfaces

- `IF-RUNTIME-STATUS`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。

## M1 implementation foothold

- 当前 [`ShellApp.tsx`](../shell-ui/src/ShellApp.tsx) 的 Runtime panel 呈现 canonical snapshot、Attached health evidence，以及 Managed state/generation/process ownership/readiness/instance/stop disposition/verified endpoint。
- M1 foothold 提供显式 Managed start 与二次确认的 generation-bound stop；它不实现日志导出、自动恢复、restart policy 或任意进程控制，这些能力仍按本模块 M2 范围跟踪。
- Attached reachability 的接口真源见 [`AttachedHealthReport` Schema](../../../../specs/runtime/attached-health-report.schema.json)。
- Managed report 的接口真源见 [`ManagedRuntimeReport` Schema](../../../../specs/runtime/managed-runtime-report.schema.json)。
