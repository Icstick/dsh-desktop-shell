---
id: ADR-0009
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0009: No Core Packaging, Node Management, or Desktop Plugin Market

## 背景

现有 DSH 生态已经拥有 Core 安装、Profile、Plugin Market 与 Scheduler 语义。Desktop 复制这些能力会产生状态分叉和供应链责任。

## 决策

Desktop 不下载 Core/Node、不直接运行 plugin install、不修改 Profile、不重建 Market。它可打开 DSH Market Surface、观察操作结果并协调 reload/restart。Scheduler 仍属 DSH；P2 只提供 wake guarantee。

## 替代方案

- Full distribution：偏离 user-owned Core。
- Desktop market：重复建设且需要理解 pnpm/Profile internals。
- 观察与协调：采用。

## 后果

用户需要自行管理 DSH 环境；Desktop 依赖清晰 diagnostics 提供可用体验。

## 验证门禁

- 仓库和产物不包含 DSH bundle/installer。
- Plugin 操作只接收语义事件。
- 没有 package-manager mutation API。

## 受影响模块

environment-settings、adapter-dsh、runtime-diagnostics、operations
