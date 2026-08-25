# Terminal Agent Adapter

**Module ID:** `MOD-TERMINAL-AGENT-ADAPTER`
**Target milestone:** M3
**Canonical status:** [MOD-TERMINAL-AGENT-ADAPTER](../../tracking/modules/MOD-TERMINAL-AGENT-ADAPTER.yaml)

## Purpose

把受控 Agent terminal tools 映射到 Terminal Automation。

## Owns

- tool mapping
- approval context
- safe result

## Does not own

- human terminal inheritance
- PTY implementation

## Inputs

- DSH tool calls
- Terminal agreement

## Outputs

- leased Terminal invocations

## Dependencies

- adapter-dsh
- terminal-provider

## Interfaces

- `IF-TERMINAL`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
