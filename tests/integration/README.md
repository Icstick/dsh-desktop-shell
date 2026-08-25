# Integration Tests

**Module ID:** `MOD-TEST-INTEGRATION`
**Target milestone:** M2
**Canonical status:** [MOD-TEST-INTEGRATION](../../tracking/modules/MOD-TEST-INTEGRATION.yaml)

## Purpose

验证 Supervisor、process、transport、adapter 和 provider 组合。

## Owns

- real/fake process flows
- resource survival

## Does not own

- release platform claims without real OS

## Inputs

- fake/real DSH

## Outputs

- integration evidence

## Dependencies

- supervisor
- fake-dsh

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
