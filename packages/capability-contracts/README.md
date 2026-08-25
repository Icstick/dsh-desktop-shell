# Capability Contracts

**Module ID:** `MOD-CAPABILITY-CONTRACTS`
**Target milestone:** M2
**Canonical status:** [MOD-CAPABILITY-CONTRACTS](../../tracking/modules/MOD-CAPABILITY-CONTRACTS.yaml)

## Purpose

为 UI、Rust boundary 与 adapters 提供 DSH-neutral 协议模型。

## Owns

- coordinate/envelope/lease/error semantics
- consumer types later

## Does not own

- Cordis/DSH/std imports
- transport implementation

## Inputs

- normative JSON Schemas

## Outputs

- validated contract artifacts

## Dependencies

- specs

## Interfaces

- `IF-NEGOTIATION`
- `IF-INVOCATION`
- `IF-LEASE`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
