# Protocol Compatibility

**Module ID:** `MOD-PROTOCOL-COMPAT`
**Target milestone:** M5
**Canonical status:** [MOD-PROTOCOL-COMPAT](../../tracking/modules/MOD-PROTOCOL-COMPAT.yaml)

## Purpose

维护 Legacy/std/版本降级与 conformance matrix。

## Owns

- compat declarations
- migration fixtures
- deprecation evidence

## Does not own

- 改变 core contracts

## Inputs

- adapter results
- fixtures

## Outputs

- support matrix

## Dependencies

- protocol-fixtures

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
