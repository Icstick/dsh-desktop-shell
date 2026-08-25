# Process Manager

**Module ID:** `MOD-PROCESS-MANAGER`
**Target milestone:** M2
**Canonical status:** [MOD-PROCESS-MANAGER](../../tracking/modules/MOD-PROCESS-MANAGER.yaml)

## Purpose

创建和验证 Managed process identity、Windows Job Object/Unix process group 与 signals。

## Owns

- spawn/handle
- graceful/force termination
- tree cleanup
- endpoint release evidence

## Does not own

- health semantics
- DSH compatibility

## Inputs

- canonical launch spec

## Outputs

- process events and verified cleanup

## Dependencies

- None beyond normative specs.

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
