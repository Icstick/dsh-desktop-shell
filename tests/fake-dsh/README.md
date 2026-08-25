# Fake DSH

**Module ID:** `MOD-FAKE-DSH`
**Target milestone:** M1
**Canonical status:** [MOD-FAKE-DSH](../../tracking/modules/MOD-FAKE-DSH.yaml)

## Purpose

模拟 DSH start/health/web/adapter 行为及错误模式。

## Owns

- deterministic backend shapes
- startup delay/crash
- adapter variants

## Does not own

- 复制 DSH source
- 声称真实兼容

## Inputs

- scenario config

## Outputs

- test endpoint/process behavior

## Dependencies

- None beyond normative specs.

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
