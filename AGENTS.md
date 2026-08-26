# Agent Operating Contract

本文件适用于本仓库全部 Agent、自动化和人类贡献者。子目录 `AGENTS.md` 只能收紧规则，不能放宽根级安全与架构不变量。

## 启动协议

1. 读取 `START_HERE.md`、`tracking/project.yaml`、`tracking/CURRENT.md`。
2. 读取当前 milestone、目标模块及相关 ADR。
3. 只认领一个主 `WI-*`；记录 session、branch/worktree 和 24 小时 advisory lease。
4. 验证依赖已满足，确认 `implementation_authorized`。

## 当前 M1 实现门禁

- `implementation_authorized: true` 只解除 M0 的全局禁码门禁，不豁免工作项认领、module boundary、ADR、security review 与 evidence 要求。
- 新增 `.rs`、`.ts`、`.tsx`、构建清单、锁文件或 workflow 必须属于已认领的 M1 工作项，并在独立 branch/worktree 中提交。
- 依赖安装、代码生成、构建与测试只能作为已认领实现任务的可追溯步骤执行；不得在状态转换 session 中顺带运行。
- 继续执行 user-owned External Core 与 clean-room/no-copy 边界；Desktop 不安装、构建或打包 DSH Core。

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
