# M6 Daemon — Execution Plan (2026-08-30)

> 规划先行：M6 是 M1-M7 中最架构性的里程碑——进程拓扑从"单一 Tauri 进程"变为"Shell UI + 独立 daemon"。ADR-0008 已冻结演进路径（P2 daemon 经专门 ADR + migration）。

## M6 退出标准（MILESTONES / WI-M6-DAEMON / ADR-0008 验证门禁）

1. Shell restart 不影响 DSH/PTY/Browser 资源存活（ADR-0008 门禁）。
2. split-brain 测试通过（多 daemon 实例竞争检测与拒绝）。
3. 统一外源 API 真实服务端接线（M5-B2 遗留：local-transport bind + broker GrantPolicy + envelope 服务在 daemon 内）。
4. Scheduler wake（IF-SCHEDULE-WAKE）可用。

## 范围梳理（M1-M5 积累的 M6 项）

| 来源 | 内容 | 切片 |
|------|------|------|
| ADR-0008 | daemon 化：Supervisor 拆出独立进程，持有 DSH/PTY/Browser | M6-A/B/C |
| WI-M6-DAEMON | Shell restart + split-brain acceptance；daemon migration ADR | M6-A/D |
| M5-B2 remaining | 统一外源 API 真实服务端接线（tauri bind + broker GrantPolicy + Event 路由 + generation 流） | M6-B |
| IF-SCHEDULE-WAKE | scheduler wake 激活（not_authorized→authorized） | M6-D |
| 已知 flaky | diagnostics ac_log_001（node 子进程并行干扰）、local-transport malformed_handshake/limits 时序 | M6-E |
| M4 remaining | interact GUI live QA、CDP provider revisit（可选） | M6-E |
| M5-D remaining | dsh-std 稳定性再评估（rc 漂移） | M6-E |

## 切片划分

### M6-A 架构冻结（ADR-0019 Daemon Migration）
- 进程拓扑决策：独立 exe（dsh-desktop-daemon）vs 同一 exe detached 模式（--daemon）；**推荐独立 bin（apps/desktop 或 crates 级 bin）**，理由：升级/重启独立、崩溃隔离、资源所有权清晰。
- 通信：daemon ↔ Shell UI 走 local-transport（已有认证 loopback + envelope——M5-B2 的参考闭环升级为真实服务端）；daemon ↔ 内部资源（PTY/browser 会话）进程内直连。
- Ownership handover：managed_runtime 的 DSH 进程树、PTY registry、browser sessions 从 tauri 进程迁移到 daemon；handover 协议（启动 daemon → 资源接管 → Shell 重连，不中断 DSH/PTY）。
- Split-brain：daemon 单实例锁（Windows named mutex / 端口所有权）+ 竞争检测（第二个实例启动时检测并退出或接管策略）；测试覆盖。
- 升级/恢复：daemon 崩溃重启策略（资源保留 vs 重建）；UI 重启时 daemon 存活。
- 已知缺口盘点：当前所有权（managed_runtime/PTY/browser 在 tauri 进程内的持有点）→ 迁移清单。

### M6-B daemon 进程骨架 + 统一 API 服务端
- daemon bin：local-transport bind（固定 loopback 端口 + 一次性 credential 签发机制——Shell 启动时获取）、envelope 服务端（复用 M5-B2 external-api-example 的 serve_connection/handle_envelope + SessionState + GrantPolicy→broker 接线）。
- 能力面：list_browsers、terminal 会话状态、runtime 状态、notification 转发（M5-C adapter 移到 daemon 内消费 $events）。
- 统一 API example 升级：external-api-example 从 standalone 参考变成 daemon 的真实服务端（或 daemon 内嵌同款逻辑，example 保留为客户端示例）。
- 认证：local-transport credential（daemon 签发、Shell 持有）+ broker grant（agent 协商仍走 M5 链路）。

### M6-C 资源迁移（Shell restart 存活）
- managed_runtime：DSH 进程树 ownership 从 tauri 迁到 daemon（retained handle/generation/tree cleanup/endpoint release 语义保留——MOD-PROCESS-MANAGER 的 extraction 要求）。
- PTY registry：迁移到 daemon（AC-PTY-001 跨 DSH restart 已证明；现在要跨 Shell restart）。
- Browser sessions：迁移到 daemon（WebView2 窗口在 daemon 进程——**注意**：WebView2 窗口需要窗口宿主！browser 窗口是 tauri 窗口——daemon 无 UI。设计决策：browser/terminal **会话状态**在 daemon，**渲染**在 Shell（tauri webview 通过 IPC 连 daemon）？这改变 M4 的架构（M4 是 tauri 内直接 WebView2）。或者 daemon 持有"资源进程"（ConPTY 子进程、DSH 进程树），browser WebView 仍在 Shell（Shell restart 会丢 browser 窗口但会话状态在 daemon 可恢复？）——**这是 M6-A 的关键架构讨论点**，写进 ADR-0019 决策。
- 测试：Shell kill/restart 后 DSH/PTY 存活（模拟 Shell 重启的集成测试）。

### M6-D Scheduler wake + split-brain 防护
- IF-SCHEDULE-WAKE：wake 操作（调度唤醒——什么唤醒？定时任务/延迟操作？按 ScheduleWake schema 现状实现最小语义）。
- split-brain 测试：双 daemon 启动 → 第二个检测到实例 → 拒绝/接管；锁释放后恢复。
- daemon 健康/重启：Shell 检测 daemon 丢失 → 重连/重启策略。

### M6-E 收尾与根治
- known flaky 根治：ac_log_001（测试隔离/时序修复）、local-transport malformed_handshake/limits（统计同步）。
- interact GUI live QA（真实 WebView2 ExecuteScript 路径）。
- dsh-std 稳定性再评估（rc 标签漂移跟踪）。
- 全量门禁、独立评审、HANDOFF-M6-DAEMON、maintainer 验收、合并 main。

## 执行顺序

1. M6-A ADR-0019（架构决策：进程拓扑/通信/handover/split-brain/资源迁移边界——**browser 渲染与状态的归属是本里程碑最需要 maintainer 确认的决策**）。
2. M6-B daemon 骨架 + 统一 API 服务端（不依赖迁移，可先行）。
3. M6-C 资源迁移 + Shell restart 存活测试。
4. M6-D scheduler wake + split-brain。
5. M6-E flaky 根治 + live QA + 收尾。

## 明确不做（本阶段）

- macOS/Linux target-host（持续搁置）。
- CDP provider 升级（M6-E 仅 revisit 评估，不实现）。
- 发布/签名（M7）。
- 三平台 hardening（M7）。

## 架构决策（maintainer 2026-08-30 确认）

- **browser 渲染归属 = 方案 A**：browser 窗口 Shell-owned（tauri WebView2 渲染），会话/provider 状态在 daemon（跨 Shell restart 存活）；DSH 进程树与 PTY 是真正的跨重启资源，browser 窗口重启后重建 + 状态恢复。ADR-0019 按此冻结。

## M6-F i18n 中英文切换（maintainer 2026-08-30 新增小任务）

- 目标：Shell UI 中英文切换选项（Settings 面板入口，持久化）。
- 范围：核心 UI 文案（rail 入口、面板标题、主要按钮/状态）提取为翻译 key，zh/en 双语；语言选择持久化（localStorage）。
- 归属：WI-M6-I18N；与 daemon 架构独立，可并行。

## 风险

- ~~browser 渲染归属~~（方案 A 已确认）。
- daemon 化回归面大（M1-M5 全部 surface 的通信路径变化）。
- daemon 化回归面大（M1-M5 全部 surface 的通信路径变化）。
- split-brain/锁的 Windows 语义细节。
- 升级路径（daemon 自身更新）超出 M6（M7 发布时设计）。
