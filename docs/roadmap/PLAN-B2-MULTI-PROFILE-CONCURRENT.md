# PLAN-B2: Multi-Profile Concurrent Runtime（ADR-0020 草案）

> 预研规划（2026-08-30，maintainer 要求：先规划，评估塞入后续开发流程的位置）。
> 状态：**草案 + 决策记录（2026-09-02 maintainer D2）**——先记录不立项；触发条件 =
> 首个可用版本 v0.1.0 发布后，作为 feature 正式立项（届时按 tracking 规则建 WI-M9-*、走 ADR）。
> 范围与红线输入：`docs/multi-profile-field-evidence.md`（A/B/C/D/R 建议 + ROI 排序）与
> `docs/roadmap/PLAN-DEBUG-OPTIMIZATION.md` §4。

## 目标

「先启动 Shell，再通过 Shell 并发运行多个 DSH profile」——每个 profile 独立
进程树、独立会话、独立端口，互不干扰；Shell 内多 surface（每 profile 一个
WebView）或快速切换。

## 现状约束（调研结论，M7 起点）

1. **supervisor 单活跃**：`ManagedRuntimeSupervisor` 持有单一 `process` 槽位，
   同一时刻只运行一个 Managed 环境（`ManagedRuntimeError::Conflict`）。
   根因：M1-M2 设计决策（单一 DSH 实例管理、FM-4 防串号）。
2. **catalog 单文件共享**（2026-08-30 对齐审计确认）：Shell 与 daemon 共用
   `%APPDATA%/dev.dsh.desktop-shell/environment-catalog-v1.json`，daemon
   per-invocation 重新加载——**天然打通，无需同步机制**。
3. **dsh_surface 单实例**：Shell 内单 WebView 挂载（label 标识）；多 profile 同时
   可见需要多 surface（多窗口或 tab 化 WebView）。
4. **端口规划**：Managed 启动需显式端口（DSH 默认端口冲突）；当前 catalog 端口
   auto/显式混合，并发时需要**端口分配器**（daemon 分配空闲端口并回报）。

## 方案要点（草案）

### 决策 1：per-environment supervisor 状态表

`ManagedRuntimeState` 的单一 `process: Option<ManagedProcess>` 改为
`active: HashMap<environment_id, ManagedProcess>`：

- start(env) 不再全局互斥；同一环境重复 start = 幂等（返回现有状态）
- stop/status/restart 按 environment_id 路由
- 迁移：保留 `active_environment_id` 作为「前台 profile」语义（UI 焦点），
  与「后台运行集」分离
- 风险：FM-4（状态串号）回归面——所有 `environment_id` 关联检查改为 map key
  校验；auto_restart 与 recovery 改为 per-environment

### 决策 2：多 surface 架构（两选一）

- **A. 多 WebView tab**（推荐）：shell 窗口内 tab 切换，每 profile 一个 WebView
  实例（wry 多 webview，`data_directory` 独立 per profile —— M4 已建立隔离
  先例）；事件路由按 surface id
- **B. 多窗口**：每 profile 独立窗口（OS 窗口管理，Shell 主窗 + profile 窗）
  —— 简单但窗口管理成本高（taskbar 噪点）
- 决策 A 依赖 dsh_surface 的 label → 多实例改造（mount 支持多 surface）

### 决策 3：端口分配器

- daemon 维护端口分配表：`PORT_RANGE_START..=PORT_RANGE_END`（如 40000-41000）
  中取未占用端口；环境显式端口优先，冲突则报错
- 分配记录持久化（环境绑定端口，重启沿用）

### 决策 4：catalog 单文件共享（已验证）

- Shell 与 daemon 共用同一 catalog 文件（同目录同名，per-invocation 加载）；
  B2 直接依赖现有机制，无前置改动

## 风险

| 风险 | 等级 | 缓解 |
|---|---|---|
| supervisor 改造回归（FM-4 串号） | 高 | 逐环境路由 + 全量回归（managed-runtime 25+ 测试扩展） |
| 多 WebView 内存/CPU | 中 | tab 懒加载（非活跃 surface 挂起） |
| 端口耗尽/冲突 | 中 | 分配器 + 显式端口优先 |
| 多 DSH 会话的磁盘/资源竞争 | 低 | profile 天然隔离（独立 dshHome） |

## 里程碑定位（2026-08-30 排期：M7=向导+B1，M8=Stable Candidate 顺延）

- **M9 候选**（推荐）：M8（Stable Candidate）交付后，M9 做 supervisor
  per-environment 改造 + 端口分配器（纯后端，测试密集）；多 surface 放 M10
- **M10 候选**：多 surface tab 化（依赖 M9 的后端多实例）
- 也可在 M8 前插入——但 Stable Candidate（三平台/签名/SBOM）是发布硬门槛，
  建议 B2 后端放 M9 更稳

## 迁移路径

1. M7: catalog 打通 + B1 切换（单活跃）
2. M8: supervisor 状态表改造（后端并发）+ 端口分配器 + 回归
3. M9: 多 surface tab（前端多 WebView）+ 事件路由 + live QA