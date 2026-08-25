# Terminal Provider

**Module ID:** `MOD-TERMINAL-PROVIDER`
**Target milestone:** M3
**Canonical status:** [MOD-TERMINAL-PROVIDER](../../tracking/modules/MOD-TERMINAL-PROVIDER.yaml)

## Purpose

管理 Desktop-owned PTY session、resource registry、IO 与 resize。

## Owns

- PTY lifecycle
- opaque IDs
- output events
- cleanup

## Does not own

- Agent authorization
- UI rendering

## Inputs

- leased Terminal operations

## Outputs

- PTY events/results

## Dependencies

- capability-contracts

## Interfaces

- `IF-TERMINAL`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
