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

## M1 integrated foothold

- 当前 process-tree 实现在 [`apps/desktop/src-tauri/src/managed_runtime.rs`](../../apps/desktop/src-tauri/src/managed_runtime.rs)，尚未抽取到本 crate。
- Windows 使用 retained Job Object 并设置 close-on-cleanup tree termination；Unix 使用 retained process group。只有 handle 所属 generation 可被 stop。
- Windows 路径已有 child-tree 与 endpoint release 测试；macOS/Linux 仍需真实平台验证。Windows graceful stop 尚无上游契约，因此当前报告 forced disposition。
