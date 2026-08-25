# Chaos Tests

**Module ID:** `MOD-TEST-CHAOS`
**Target milestone:** M2
**Canonical status:** [MOD-TEST-CHAOS](../../tracking/modules/MOD-TEST-CHAOS.yaml)

## Purpose

注入 crash、port/PID、disconnect、provider failure 与 race。

## Owns

- fault injection
- cleanup assertions
- safe-stop evidence

## Does not own

- 随机破坏用户环境

## Inputs

- isolated fixtures

## Outputs

- reliability evidence

## Dependencies

- supervisor
- fake-dsh

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
