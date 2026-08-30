# Legacy DSH Adapter

**Module ID:** `MOD-ADAPTER-DSH`
**Target milestone:** M2
**Canonical status:** [MOD-ADAPTER-DSH](../../tracking/modules/MOD-ADAPTER-DSH.yaml)

> 实现路径变更（2026-08-30，M5-C）：Rust 实现位于 `crates/adapter-dsh`（本目录保留为文档壳与模块登记镜像）。模块登记 `tracking/modules/MOD-ADAPTER-DSH.yaml` 的 path 已同步更新。

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