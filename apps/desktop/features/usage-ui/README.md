# Usage UI

**Module ID:** `MOD-USAGE-UI`
**Target milestone:** M3
**Canonical status:** [MOD-USAGE-UI](../../../../tracking/modules/MOD-USAGE-UI.yaml)

## Purpose

展示 DSH 侧标准化的 usage、来源、范围和估算标记。

## Owns

- aggregation views
- source/estimate labels
- filters

## Does not own

- 解析 DSH internal logs
- 读取 provider credentials

## Inputs

- UsageTelemetry

## Outputs

- visual summaries

## Dependencies

- usage-collector

## Interfaces

- `IF-USAGE`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。
