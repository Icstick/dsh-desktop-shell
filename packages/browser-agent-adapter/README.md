# Browser Agent Adapter

**Module ID:** `MOD-BROWSER-AGENT-ADAPTER`
**Target milestone:** M4
**Canonical status:** [MOD-BROWSER-AGENT-ADAPTER](../../tracking/modules/MOD-BROWSER-AGENT-ADAPTER.yaml)

## Purpose

把 DSH browser tools 映射到 scoped Browser capability。

## Owns

- tool semantics
- snapshot refs
- action mapping

## Does not own

- raw CDP
- provider profile

## Inputs

- DSH tool calls
- Browser agreement

## Outputs

- leased Browser invocations

## Dependencies

- adapter-dsh
- browser-provider

## Interfaces

- `IF-BROWSER`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
