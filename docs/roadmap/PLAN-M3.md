# M3 Workbench — Execution Plan (2026-08-28)

> 规划先行：每个切片 contract-first（ADR/Schema/fixtures → 实现 → 门禁 → 证据 → tracking）。
> 本阶段继续搁置：macOS/Linux target-host 测试；交互式 GUI/WebView2 smoke（保留既有证据与 driver）。

## M3 退出标准（M3.yaml / ROADMAP / ACCEPTANCE.md）

1. Persistent Terminal 跨 DSH restart（AC-PTY-001：DSH restart 不终止 Desktop-owned PTY）。
2. Usage 与 Notification 可审计（新增 AC-NOT-* / AC-USG-* 契约）。
3. Diagnostics redaction 通过（AC-LOG-001 已在 M2 通过；M3 补齐 diagnostics UI/日志导出入口）。

## 切片划分

### M3-A Notification（IF-NOTIFICATION，MOD-SHELL-UI）
- 契约：ADR-0016（notification 审计与内容策略）；notification request/report schema + fixtures；AC-NOT-001（contentPolicy 约束：title_only/redacted_summary/explicit_body）、AC-NOT-002（dedupeKey 去重与 TTL 过期）。
- 实现：backend notification service（registry：dedupe、TTL、可审计事件日志）；Shell UI 通知面板；ACL 扩展。
- 触发源：runtime_changed（Supervisor 状态转换事件）、approval/question 等由后端记录。

### M3-B Persistent Terminal（IF-TERMINAL，MOD-TERMINAL-PROVIDER + MOD-TERMINAL-UI）
- 契约：ADR-0015（Desktop-owned PTY、Windows ConPTY、Terminal Surface/Automation 分离：human_surface 先实现，agent_automation 留 M5）；terminal create/write/resize/close request、PTY report、output event schema + fixtures；AC-PTY-001（DSH restart 不终止 PTY）。
- 实现：
  - crates/terminal-provider：Windows ConPTY（CreatePseudoConsole + 管道 IO + resize + 关闭），spawn 用户 shell（cmd/powershell），opaque PTY id，输出事件，清理。
  - Desktop PTY 会话管理：PTY 进程是 Desktop-owned（不属于 DSH process tree）→ Managed DSH stop/restart 不影响 PTY 存活（AC-PTY-001 测试：start DSH → 建 PTY → stop/restart DSH → PTY 仍可 IO）。
  - 终端 UI：xterm.js（前端依赖，作为已认领工作项的可追溯安装），Tauri event 推送输出。
- 边界：agent_automation 模式拒绝（M5 前 fail-closed）；PTY 不经 DSH 授权不放行。

### M3-C Usage（IF-USAGE，MOD-USAGE-COLLECTOR + MOD-USAGE-UI）
- 契约：ADR-0016（usage 本地优先、可审计）；usage record/snapshot schema + fixtures；AC-USG-001（快照可审计：来源/周期/token 估算/是否 estimate）。
- 实现：usage collector（backend Rust 记录 Desktop 事件：runtime 会话、terminal 会话、notification；token 估算来自 usage-capability 语义，本地持久化，无网络外发）；Usage UI 面板展示估算与来源。
- 边界：只记录估算与元数据，不记录终端/通知内容（隐私）。

### M3-D Diagnostics 完成（MOD-RUNTIME-DIAGNOSTICS）
- 若时间允许：独立 diagnostics UI 页 + 日志导出（redacted）入口；否则记入 handoff remaining（M2 的 diagnostics report 已满足 AC-LOG-001）。

## 执行顺序

1. 契约冻结（ADR-0015/0016 + 全部 schema/fixtures + AC 增补 + IF/MOD tracking）→ 提交。
2. M3-A 与 M3-C 子代理并行（文件隔离）；M3-B 主代理实现（ConPTY 核心）。
3. 集成：全量门禁（Rust/前端/ACL/specs）、tracking、HANDOFF-M3-WORKBENCH、推送。

## 明确不做（本阶段）

- macOS/Linux PTY（ConPTY 仅 Windows；Unix 留待 target-host 阶段）。
- agent_automation 终端模式（M5 adapter 阶段）。
- 任何网络外发的 usage/telemetry。
