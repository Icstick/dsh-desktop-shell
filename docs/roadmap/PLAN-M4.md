# M4 Shared Browser — Execution Plan (2026-08-29)

> 规划先行：每个切片 contract-first（ADR/Schema/fixtures → 实现 → 门禁 → 证据 → tracking）。
> 沿用既有约束：Windows 优先（WebView2 生态）、macOS/Linux 结构性证据 + fail-closed；agent_automation 授权链至 M5（与 M3 终端同策略）。

## M4 退出标准（MILESTONES.md / M4.yaml / ACCEPTANCE.md）

1. Browser provider contract 冻结（IF-BROWSER v1alpha1 更新：request/report schema + fixtures，AC-BRW-001/002 细化）。
2. 至少两个 candidate provider PoC（嵌入式 WebView2 vs 外部浏览器 CDP），产出选型报告。
3. Human takeover（AC-BRW-002：human takeover 撤销 Agent mutation lease——M4 内 broker 接线，agent_automation 本身 fail-closed 至 M5）。
4. Profile isolation（独立 browser profile/user-data-dir，AC-BRW-001：Browser page 无 Desktop IPC）。

## Provider 选型分析（PoC 候选）

### Candidate A：嵌入式 WebView2（crates/browser-provider + wry/WebView2）
- 与 M1 dsh-surface 同栈：WebView2 permission-deny、navigation policy、Tauri capability 隔离经验直接复用。
- 隔离：独立 user-data-folder（profile isolation）、独立 webview label（capability 精确匹配 shell，browser label 零 privileged command）。
- 优点：无外部依赖、生命周期由 Desktop 管理、窗口内嵌 UI 一致。
- 缺点：human takeover 语义弱（agent 与 human 共用同一 webview，需 UI 状态切换）；与 dsh-surface 同进程需严格验证 capability 合并不越权。
- 风险点：RISK-WEBVIEW-PERMISSION-HOOK（Tauri/Wry API 漂移）；两 WebView 并存时的 permission 合并。

### Candidate B：外部浏览器 + 受管 CDP（Edge/Chrome，user-data-dir 隔离）
- 隔离：独立 user-data-dir 天然隔离 profile；进程独立于 Desktop（崩溃隔离）。
- human takeover：用户直接接管浏览器窗口（agent 断开 CDP 会话即释放）。
- 自动化：CDP 由 provider 内部封装（遵循 WEBVIEW_ISOLATION '禁止 raw CDP socket'——不把 CDP 暴露给 WebView/Agent，仅 provider 内部驱动）。
- 缺点：外部进程生命周期管理（owner 语义——Desktop 启动的受管浏览器 vs 用户自有浏览器 attach）；WebView2 之外引入新依赖（Edge 已随 Windows 11 提供）。
- 风险点：进程树 ownership、端口/凭据管理（复用 M2 local-transport 模式）、浏览器升级兼容。

### PoC 结果（2026-08-29，POC-M4B-REPORT.md）

- 双 candidate 全部 PASS P1-P5：A（wry 独立 crate）profile 隔离 198 项 EBWebView 结构、文本快照通过（双重 JSON 编码已识别）；B（零依赖 Node）1.5s 全流程、taskkill 零残留。
- **maintainer 拍板（2026-08-29）：默认 provider = A（WebView2 embedded）；B 搁置（M6 revisit）；wry 维持 0.55.1（tauri 锁定）+ webview2-com 直调 permission/capture（ADR-0017 决策 6）。**

### 选型建议（maintainer 已确认 2026-08-29）
- **默认 provider：Candidate A（WebView2 embedded）**——与既有技术栈一致、风险最低，M4-C 实现主路径。
- **Candidate B（外部 Edge + CDP）作为 PoC 对比**（M4-B 产出对比证据；是否升级为正式 provider 由 PoC 报告后 maintainer 决定）。
- **M4 范围：human_surface only**（用户浏览）；agent_automation fail-closed 至 M5（与 M3 终端同策略）；AC-BRW-002 的 agent lease 语义验收移至 M5。

## 切片划分

### M4-A 契约冻结（IF-BROWSER / MOD-BROWSER-*）
- ADR-0017：Shared Browser 架构——Browser 与 DSH Surface 分权、profile 隔离、human takeover 语义、agent_automation 至 M5 fail-closed。
- IF-BROWSER 更新：create/navigate/snapshot/interact/take_over/close 的 request/report schema + 正负 fixtures（禁止 caller 提交 endpoint/credential/profile 路径——沿用 IF-DSH-SURFACE-LIFECYCLE 模式）。
- AC-BRW-001/002 细化 + AC 目录更新；MOD-BROWSER-PROVIDER/UI/AGENT-ADAPTER module contract review（security_review: required）。
- 触发源梳理：Runtime/Notification 已有事件，browser 状态事件（load/error/closed）契约。

### M4-B Provider PoC
- PoC A：WebView2 embedded——最小 create/navigate/snapshot 回路 + profile 隔离验证 + capability 合并审查。
- PoC B：外部 Edge + CDP——受管启动（user-data-dir、--remote-debugging-port 或 pipe）、create/navigate/snapshot、human takeover（agent 释放 → 用户接管）。
- 产出 PoC 报告（证据：运行输出、隔离矩阵、takeover 演示）→ maintainer 选型确认。

### M4-C 选定 provider 实现（默认 A）
- crates/browser-provider：会话注册表（opaque id、Desktop-owned）、navigation policy、snapshot/interact 边界、teardown。
- Browser bridge commands + ACL 扩展（沿用 28→N 命令门禁）；Browser UI（rail + 面板，复用 xterm/terminal 模式）。
- AC-BRW-001 测试：Browser page 无 Desktop IPC（capability 三层闭合复验）。

### M4-D Human takeover / profile / broker 接线
- human takeover 状态机（agent 会话 ↔ human 会话切换；take_over 操作语义）。
- Capability Broker 接线：browser mutation lease（AC-BRW-002：human takeover 撤销 lease）——复用 ADR-0014 broker 机制，M4 只接 human 侧。
- profile isolation 证据（独立 user-data-dir、无凭据泄漏、日志脱敏）。

## 执行顺序

1. M4-A 契约冻结（ADR-0017 + schema/fixtures + AC + tracking）→ 提交。
2. M4-B 两个 PoC 并行（独立隔离目录/示例）→ 选型报告 → maintainer 确认。
3. M4-C 实现默认 provider（crates/browser-provider + bridge + UI + ACL）。
4. M4-D takeover/broker/profile 证据。
5. 集成门禁（Rust/frontend/ACL/specs）、tracking、HANDOFF-M4-BROWSER、推送。

## 明确不做（本阶段）

- agent_automation browser 模式（M5 adapter 授权链落地前 fail-closed）。
- macOS/Linux provider 运行时（结构性证据 + unsupported 语义，target-host 阶段）。
- raw CDP/WebView IPC 暴露给 WebView 或 Agent（WEBVIEW_ISOLATION 禁止 API）。
- Browser 扩展、书签/密码同步、devtools UI（非 Desktop 范围）。
- 接管用户自有浏览器会话（只受管 Desktop 启动的浏览器实例；Attached browser 语义 M6 再议）。
