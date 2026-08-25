# Local Transport Agent Rules

- 继承仓库根 `AGENTS.md`；本文件只收紧范围。
- 只修改 `crates/local-transport/` 及当前工作项明确授权的 contract/test 文档。
- 开始前读取 `MOD-LOCAL-TRANSPORT` tracking、相关接口与 ADR。
- 禁止把本模块未拥有的职责“顺手”实现。
- Public behavior 变化先更新 Schema/ADR/fixture 计划。
- local-only、replay/stale/oversize 防护；token 不证明单插件身份。
- 结束前更新工作项、module/interface 状态、evidence 与 `HANDOFF-*`。
