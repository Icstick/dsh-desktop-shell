---
id: ADR-0008
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0008: Integrated P0 Supervisor, Independent P2 Daemon

## 背景

首版直接安装 daemon 会引入多实例、升级、IPC、split-brain 与 service lifecycle；长期又需要 Shell、Supervisor、DSH 三生命周期隔离。

## 决策

P0 Supervisor 在 Tauri Rust process 内运行，但通过独立 crate/API 设计。M6/P2 经专门 ADR 和 migration 后拆为 daemon，持有 DSH、PTY、Browser providers。

## 替代方案

- 永久 integrated：Shell restart 会中断资源。
- 首版 daemon：范围和风险过大。
- 分阶段：采用。

## 后果

P0 简单且保留演进路径；P2 需要 ownership handover、daemon upgrade 和 recovery 设计。

## 验证门禁

- P0 API 无 UI-specific type。
- M6 Shell restart/resource survival 测试。
- daemon split-brain/upgrade ADR。

## 受影响模块

supervisor、local-transport、terminal-provider、browser-provider、apps/desktop
