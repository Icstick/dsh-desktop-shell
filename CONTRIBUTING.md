# Contributing

## 开始前

- 阅读 [START_HERE.md](START_HERE.md) 与 [AGENTS.md](AGENTS.md)。
- 选择或创建一个 `WI-*`。
- 变更 public contract、ownership、状态机或 trust boundary 前先走 ADR。

## 分支与提交

- 默认 `main` 禁止直接推送；通过短生命周期分支和 PR 合并。
- 分支建议：`docs/`、`feat/`、`fix/`、`security/`、`compat/`。
- 使用小而可审查的提交；协议与迁移说明必须同提交出现。
- 推荐 squash merge，发布维护分支仅在需要维护已发布 minor 时创建。

## Review 门禁

| 变更 | 最低要求 |
|---|---|
| 文档、UI 规范 | 1 review |
| Adapter / compatibility | 1 review + contract evidence |
| Public protocol | 2 reviews + ADR + migration note |
| Supervisor / process ownership | 2 reviews |
| Security / IPC / Browser / PTY | 2 reviews，至少一名 security owner |
| Release / signing | maintainer approval |

## Agent 参与

PR 必须说明 Human authored、Agent assisted 或 Primarily Agent generated，并列出 Agent 生成内容的人工复核范围。该字段用于分配审查强度，不用于评价贡献者。
