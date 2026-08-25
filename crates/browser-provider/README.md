# Browser Provider

**Module ID:** `MOD-BROWSER-PROVIDER`
**Target milestone:** M4
**Canonical status:** [MOD-BROWSER-PROVIDER](../../tracking/modules/MOD-BROWSER-PROVIDER.yaml)

## Purpose

启动和管理 Chromium/Edge/CDP provider、profiles、sessions 与 human takeover。

## Owns

- provider process
- profile/session isolation
- snapshot/action translation

## Does not own

- Agent policy
- raw CDP exposure

## Inputs

- leased Browser operations

## Outputs

- safe snapshot/result/events

## Dependencies

- capability-contracts

## Interfaces

- `IF-BROWSER`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
