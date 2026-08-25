# Protocol Fixtures

**Module ID:** `MOD-PROTOCOL-FIXTURES`
**Target milestone:** M2
**Canonical status:** [MOD-PROTOCOL-FIXTURES](../../tracking/modules/MOD-PROTOCOL-FIXTURES.yaml)

## Purpose

保存正向、负向和跨版本协议 fixture。

## Owns

- valid/invalid examples
- golden agreements/errors

## Does not own

- 生产 secrets
- 实现特定快照

## Inputs

- schemas

## Outputs

- contract test data

## Dependencies

- protocol-schemas

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
