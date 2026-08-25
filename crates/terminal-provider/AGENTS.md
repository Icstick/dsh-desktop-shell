# Terminal Provider Agent Rules

- 继承仓库根 `AGENTS.md`；本文件只收紧范围。
- 只修改 `crates/terminal-provider/` 及当前工作项明确授权的 contract/test 文档。
- 开始前读取 `MOD-TERMINAL-PROVIDER` tracking、相关接口与 ADR。
- 禁止把本模块未拥有的职责“顺手”实现。
- Public behavior 变化先更新 Schema/ADR/fixture 计划。
- workspace/cwd scope、Surface/Automation 分权、revoke 清理。
- 结束前更新工作项、module/interface 状态、evidence 与 `HANDOFF-*`。
