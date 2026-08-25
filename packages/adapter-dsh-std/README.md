# dsh-std Adapter

**Module ID:** `MOD-ADAPTER-DSH-STD`
**Target milestone:** M5
**Canonical status:** [MOD-ADAPTER-DSH-STD](../../tracking/modules/MOD-ADAPTER-DSH-STD.yaml)

## Purpose

把已知 dsh-std protocol/facet 映射到内部契约。

## Owns

- version mapping
- conformance fixtures
- publication/revoke

## Does not own

- core dependency
- 私有协议冒充标准

## Inputs

- known std versions
- internal contracts

## Outputs

- agreements and mapped invocations

## Dependencies

- capability-contracts

## Interfaces

- `IF-NEGOTIATION`
- `IF-INVOCATION`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
