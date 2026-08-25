# Agent Operating Contract

本文件适用于本仓库全部 Agent、自动化和人类贡献者。子目录 `AGENTS.md` 只能收紧规则，不能放宽根级安全与架构不变量。

## 启动协议

1. 读取 `START_HERE.md`、`tracking/project.yaml`、`tracking/CURRENT.md`。
2. 读取当前 milestone、目标模块及相关 ADR。
3. 只认领一个主 `WI-*`；记录 session、branch/worktree 和 24 小时 advisory lease。
4. 验证依赖已满足，确认 `implementation_authorized`。

## 当前 M0 特殊限制

- 禁止新增 `.rs`、`.ts`、`.tsx`、`Cargo.toml`、`package.json`、锁文件或 build workflow。
- 禁止安装依赖、运行代码生成或构建项目。
- 允许修改 Markdown、YAML、JSON Schema 与 GitHub 模板。

## 架构不变量

- User-owned External Core；Desktop 不管理 DSH 发行。
- Managed/Attached 显式分权；Attached 默认拒绝 stop/restart/kill。
- DSH WebView 与 Browser WebView 无 privileged native bridge。
- 所有 Agent native action 先经过 DSH tool/policy，再进入 Capability Broker。
- Capability 独立版本化；DSH-specific type 不穿越 Adapter boundary。
- `dsh-std` 是 optional adapter，不是 core dependency。
- Terminal Surface/Automation 与 Browser Surface/Automation 必须分权。
- 状态真源在 `tracking/`，接口真源在 `specs/`，原因真源在 ADR。

## 变更协议

- Interface、Schema、状态机、transport、trust boundary 或 ownership 变化：先提 ADR 或更新现有 ADR。
- 公开协议变化：更新 Schema、changelog、compatibility fixture 计划和 migration note。
- 一个工作项一个主 owner；并行工作通过独立模块或独立工作项拆分。
- 不把聊天、模型记忆或未提交实现当作项目真源。

## 证据要求

完成声明必须链接可复查证据：测试输出、Schema 校验、文档链接、review 记录或发布 artifact。未验证时使用 `review`，不得使用 `verified` 或 `done`。

## Handoff

Session 结束前：

1. 更新 `WI-*` 的状态、evidence、blocked_by、next_action。
2. 更新相关 `MOD-*` / `IF-*`。
3. 新建 `HANDOFF-*`，列出已完成、未完成、验证、风险、精确下一步。
4. 释放或延长 claim；过期 claim 的回收必须留下记录。
