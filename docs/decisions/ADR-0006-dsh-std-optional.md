---
id: ADR-0006
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0006: dsh-std Compatible but Not Required

## 背景

dsh-std 的 meta-protocol、adapter、facet 与独立版本方向契合。2026-08-25 的 [External Baseline](../research/EXTERNAL_BASELINE.md) 再次确认其代码与提案仍为 early drafts，且 registry 的 `latest` 与 `rc` 指向不同版本；wire/auth/reconnect 等不能视为稳定契约。

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
- known fixture 绑定精确版本与 artifact integrity，不仅绑定 dist-tag。
- alpha type 不出现在 core/public UI。
- adapter 失败时 baseline 可用。

## 受影响模块

adapter-dsh-std、adapter-dsh、capability-contracts、compatibility
