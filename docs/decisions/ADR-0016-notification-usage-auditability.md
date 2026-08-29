---
id: ADR-0016
status: accepted
date: 2026-08-28
owner_role: interop-owner
---

# ADR-0016: Notification Content Policy and Local-First Usage Auditability

## 背景

M3 需要 Notification 与 Usage 可审计（M3 退出标准）。通知可能携带敏感内容，usage 数据可能涉及估算与来源，两者都不得外发。

## 决策

1. Notification 内容策略：`title_only` / `redacted_summary` / `explicit_body`。只有 `explicit_body` 允许 body；title 限长 128、body 限长 512（schema 约束）。桌面侧默认 `title_only`，UI 展示遵守策略。
2. Notification 可审计：每条通知写入 AppData 审计记录（id、event、title、contentPolicy、时间、dedupeKey、source），不写 body（除非 explicit_body 且用户开启）；dedupeKey 在 TTL 内折叠重复通知（AC-NOT-002）。
3. Usage 本地优先：usage-collector 记录 Desktop 事件（runtime 会话、terminal 会话、notification 触发），token 估算语义沿用 usage-capability schema（inputTokens/outputTokens/cost/currency/isEstimate）；记录本地持久化，**无网络外发**（AC-USG-002）。
4. Usage 隐私：usage 记录只含来源/周期/估算，绝不包含终端或通知内容（AC-USG-001）。
5. M5 之前 DSH adapter 产生的通知/usage 不接入（保持 Desktop 自有事件源）。

## 验证门禁

- AC-NOT-001/002、AC-USG-001/002 fixtures 与 Rust 测试。
- 审计记录序列化测试不含 body（除非 explicit_body 策略）与终端内容。

## 受影响模块

- MOD-SHELL-UI（Notification）
- MOD-USAGE-COLLECTOR / MOD-USAGE-UI
- IF-NOTIFICATION / IF-USAGE
