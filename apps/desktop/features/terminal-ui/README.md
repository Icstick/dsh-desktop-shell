# Terminal UI

**Module ID:** `MOD-TERMINAL-UI`
**Target milestone:** M3
**Canonical status:** [MOD-TERMINAL-UI](../../../../tracking/modules/MOD-TERMINAL-UI.yaml)

## Purpose

显示和操作 Supervisor-owned Human Terminal sessions。

## Owns

- tabs
- xterm presentation
- resize/input gestures

## Does not own

- PTY process ownership
- Agent permission

## Inputs

- Terminal events/resource IDs

## Outputs

- human write/resize/close intents

## Dependencies

- terminal-provider

## Interfaces

- `IF-TERMINAL`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。
