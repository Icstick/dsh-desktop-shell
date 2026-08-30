# Adapter dsh-std Agent Rules

- 继承仓库根 `AGENTS.md`；本文件只收紧范围。
- 只修改 `crates/adapter-dsh-std/` 及当前工作项明确授权的 contract/test 文档。
- 开始前读取 `MOD-ADAPTER-DSH-STD` tracking、相关接口与 ADR-0018。
- 禁止把本模块未拥有的职责"顺手"实现。
- Public behavior 变化先更新 Schema/ADR/fixture 计划；conformance 坐标变更必须同步
  `conformance.rs` 常量、`fixtures/` 与 EXTERNAL_BASELINE/SOURCE_REGISTER 引用。
- L2 语义边界：不采用未稳定 wire、不跳过 Legacy/L0 fallback、alpha type 不穿越。
- 结束前更新工作项、module/interface 状态、evidence 与 `HANDOFF-*`。
