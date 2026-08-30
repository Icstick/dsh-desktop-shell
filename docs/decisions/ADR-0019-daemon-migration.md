# ADR-0019: Daemon Migration and Process Topology (M6)

- Status: accepted
- Date: 2026-08-30
- Milestone: M6 Daemon
- Owner: runtime-and-security-owner

## Context

ADR-0008 冻结了演进路径：P0 Supervisor 在 Tauri 进程内，M6 拆为独立 daemon 持有 DSH、PTY、Browser providers；Shell restart 不得影响资源存活（ADR-0008 验证门禁）。M5 已提供通信基础：local-transport（认证 loopback、framing/deadline）+ envelope（Invocation/Result/Event）+ broker 授权链 + 统一外源 API 参考闭环（external-api-example）。M6 需要把这些升级为真实服务端并迁移资源所有权。

## Decisions

### 决策 1：daemon 形态 = 独立 bin（`dsh-desktop-daemon`）
- 独立可执行文件（apps/desktop 的第二个 bin 或 crates/daemon 的 bin），与 Shell 分开构建/启动。
- 理由：升级/重启独立、崩溃隔离（Shell 崩溃不影响 daemon）、资源所有权清晰（ADR-0008 后果）、M7 发布/签名时作为独立服务处理。
- daemon 无 UI（无窗口创建）；**唯一例外：无**（browser 渲染在 Shell，决策 2）。

### 决策 2：browser 渲染归属 = 方案 A（maintainer 2026-08-30 确认）
- browser **窗口/渲染**留在 Shell（tauri WebView2 进程内）；**会话状态**（SessionRegistry、URL、binding）在 daemon。
- daemon 是浏览器会话状态的权威源；Shell 重启后 browser 窗口重建并从 daemon 恢复会话（id/url/状态），用户可继续。
- 后果：browser 不满足"跨 Shell restart 无中断渲染"（窗口会重建），但状态不丢；DSH/PTY 是真正的跨重启资源（决策 3）。
- WebView2 的会话 cookie/profile 数据在 Shell 侧 user-data-dir（M4 已隔离）——daemon 状态恢复不依赖窗口进程。

### 决策 3：资源迁移边界
- 迁入 daemon：managed_runtime 的 DSH 进程树（retained handle/generation/tree cleanup/endpoint release 语义保留——MOD-PROCESS-MANAGER extraction 要求）、PTY registry（ConPTY 子进程，AC-PTY-001 跨 DSH restart 已证明，现需跨 Shell restart）、browser 会话状态、M5-C adapter（$events 消费）、broker（授权链权威）。
- 留在 Shell：全部 WebView2 渲染（shell UI、browser 窗口、dsh-surface）、前端状态。
- 通信：daemon ↔ Shell 走 local-transport + envelope（M5-B2 参考闭环升级为真实服务端）；Shell 启动时从 daemon 获取 credential（一次性签发），命令经 Invocation/Result。
- handover 协议：Shell 启动 daemon（若未运行）→ 连接 → 资源接管确认（DSH/PTY 已在 daemon 持有则不重启）→ Shell 重建 UI。

### 决策 4：split-brain 防护
- daemon 单实例锁：Windows named mutex（`dev.dsh.desktop-shell.daemon`）+ 端口所有权双重检测。
- 第二个实例启动：检测到锁 → 退出（exit code 约定）并记录；Shell 连接已存在实例。
- 测试：双实例启动 → 第二实例拒绝；锁释放后可启动；Shell 重连到现存实例。

### 决策 5：统一外源 API 服务端（M5-B2 升级）
- daemon 内 local-transport bind（固定 loopback 端口）+ envelope 服务端（external-api-example 的 serve_connection/handle_envelope + GrantPolicy→broker 驱动——M5-E1 授权桥）。
- 能力面：system.ping、browser.*（list/status）、terminal.*（status）、runtime.*（managed 状态）、notification.*（M5-C 事件流转发）。
- 认证：local-transport 一次性 credential（daemon 签发）+ broker grant/lease（agent 协商沿用 M5 链路）。
- Event 路由：daemon 内订阅路由（external-api-example 缺的 Event 订阅补上）。

### 决策 6：Scheduler wake（IF-SCHEDULE-WAKE）
- 最小语义：daemon 内 scheduler 注册唤醒操作（按 ScheduleWake schema：wake 请求 → daemon 执行预定动作）。
- 激活 IF-SCHEDULE-WAKE（not_authorized→authorized）；M6 只做最小可用（定时/延迟触发 daemon 内动作），完整调度策略留 M7。

## Consequences

- 新增 crates/daemon（或 apps/desktop 第二 bin）；local-transport 服务端复用 M5-B2 逻辑。
- M1-M5 全部 surface 的通信路径变化（Shell 内直接调用 → daemon IPC）：terminal/browser/runtime 命令改为经 envelope。
- ADR-0008 门禁测试：Shell restart 后 DSH/PTY 存活（集成测试：杀 Shell → daemon 资源保留 → 新 Shell 重连）。
- split-brain 测试、flaky 根治、i18n（WI-M6-I18N 独立并行）。
- M7 发布时 daemon 升级/签名路径再设计。

## 风险

- 回归面大：M1-M5 所有命令路径改造（IPC 化）——分片迁移，每片保留 L0 fallback。
- daemon 崩溃策略：资源保留 vs 重建（PTY 子进程可保留，broker 状态重建）——M6-C 细化。
- Windows named mutex + 端口锁的竞态细节。
- 统一 API 首版本能力面收敛（避免全量暴露）。
