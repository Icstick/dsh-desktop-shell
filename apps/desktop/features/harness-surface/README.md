# Harness Surface

**Module ID:** `MOD-HARNESS-SURFACE`
**Target milestone:** M1
**Canonical status:** [MOD-HARNESS-SURFACE](../../../../tracking/modules/MOD-HARNESS-SURFACE.yaml)

## Purpose

承载原版 DSH Web UI，并提供 loading、error、reconnect 与 route hint 恢复。

## Owns

- DSH WebView container
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

- surface state and user-visible diagnostics

## Dependencies

- shell-ui
- supervisor status

## Interfaces

- `IF-RUNTIME-STATUS`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。
