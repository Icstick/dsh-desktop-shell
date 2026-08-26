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

## M0 Fixture Plan

- valid：Hello、Agreement、Invocation、success Result、error Result、Event、ScheduleWake。
- invalid：Hello 携带 method、Agreement 缺 replyTo、Invocation 携带 error、Result 同时携带 payload/error、unknown coordinate、duplicate requirement、wrong generation。
- compatibility：Legacy baseline、known dsh-std、unknown dsh-std 与 no-adapter degraded activation。

## Dependencies

- protocol-schemas

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
