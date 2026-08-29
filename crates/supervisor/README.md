# Supervisor

**Module ID:** `MOD-SUPERVISOR`
**Target milestone:** M2
**Canonical status:** [MOD-SUPERVISOR](../../tracking/modules/MOD-SUPERVISOR.yaml)

## Purpose

管理 Environment、Backend state、health、restart、recovery 与 ownership。

## Owns

- state machine
- generation/instance
- restart coordination
- P0 Capability Broker 的 grant/lease/scope/generation enforcement 与 provider dispatch
- P2 wake

## Does not own

- DSH update/Profile mutation
- DSH tool/policy 决策与 Adapter mapping
- arbitrary process control

## Inputs

- validated StartSpec
- runtime requests
- health/process events

## Outputs

- RuntimeStatus
- audited lifecycle events

## Dependencies

- process-manager
- local-transport
- providers

## Interfaces

- `IF-ENVIRONMENT`
- `IF-RUNTIME-STATUS`
- `IF-RUNTIME-CONTROL`
- `IF-SCHEDULE-WAKE`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。

## M1 integrated foothold

- 当前实现位于 [`apps/desktop/src-tauri/src/managed_runtime.rs`](../../apps/desktop/src-tauri/src/managed_runtime.rs)，随 Tauri backend 生命周期运行；尚未拆入本 crate。
- 已实现 persisted Managed Environment 的 explicit start/status/exact-generation stop、retained process-tree ownership 与 output-plus-TCP readiness。
- 自动 restart/recovery、健康策略与 P2 daemon 仍未实现；拆分时必须保持现有 Schema、generation 与 ownership 语义。
