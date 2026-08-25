# Tracking System

本目录是项目执行状态真源。使用一记录一文件，避免多个 Agent 同时编辑大表。

## 读取顺序

`project.yaml -> CURRENT.md -> current milestone -> module -> interface -> work item -> latest handoff`

## 状态

`proposed | ready | in_progress | blocked | review | verified | done | superseded`

`tracking/project.yaml` 使用项目级状态 `in_review` 表示 M0 仍等待独立 owner 审查；工作项与模块继续使用上述统一状态。

只有具备可复查 evidence 的记录才能进入 `verified` 或 `done`。

## Claim

工作项允许一个主 owner，claim 是 24 小时 advisory lease。过期 claim 只能在检查原 session/branch 后回收，并在 handoff/review 中记录。人类 override 必须说明原因。

## 真源

- 状态：tracking。
- 规范：specs。
- 原因：ADR。
- 说明：docs/module docs。
