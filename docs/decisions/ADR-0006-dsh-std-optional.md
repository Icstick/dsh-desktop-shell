---
id: ADR-0006
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0006: dsh-std Compatible but Not Required

## 背景

dsh-std 的 meta-protocol、adapter、facet 与独立版本方向契合，但截至 2026-08-25 仍为 early drafts，wire/auth/reconnect 等尚不稳定。

## 决策

Core contracts 不 import dsh-std alpha types。建立独立 adapter-dsh-std，把内部 capability 映射到已知标准版本；Legacy Adapter 长期保留。声明兼容某版本时必须通过对应 fixture/conformance。

## 替代方案

- Hard dependency：spec churn 会侵入 core。
- 完全忽略：失去互操作方向。
- Optional adapter：采用。

## 后果

需要维护两类 adapter，但任何一方变化只影响 mapping boundary；dsh-std 停止发展时 core 仍工作。

## 验证门禁

- dsh-std absent/known/unknown 三类 fixture。
- alpha type 不出现在 core/public UI。
- adapter 失败时 baseline 可用。

## 受影响模块

adapter-dsh-std、adapter-dsh、capability-contracts、compatibility
