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
- P2 wake

## Does not own

- DSH update/Profile mutation
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
