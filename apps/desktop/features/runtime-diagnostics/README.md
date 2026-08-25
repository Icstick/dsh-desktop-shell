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
