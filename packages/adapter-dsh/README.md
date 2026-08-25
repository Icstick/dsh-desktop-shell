# Legacy DSH Adapter

**Module ID:** `MOD-ADAPTER-DSH`
**Target milestone:** M2
**Canonical status:** [MOD-ADAPTER-DSH](../../tracking/modules/MOD-ADAPTER-DSH.yaml)

## Purpose

吸收 DSH/Cordis 变化并映射内部 capability。

## Owns

- DSH discovery/mapping
- optional companion
- degradation reasons

## Does not own

- native provider
- Desktop UI
- Profile mutation

## Inputs

- DSH public/validated seams

## Outputs

- internal Hello/Invocation/Event

## Dependencies

- capability-contracts

## Interfaces

- `IF-NEGOTIATION`
- `IF-INVOCATION`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
