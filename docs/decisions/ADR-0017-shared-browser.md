# ADR-0017: Shared Browser Surface (M4)

- Status: accepted
- Date: 2026-08-29
- Milestone: M4 Shared Browser
- Owner: browser-and-security-owner

## Context

M1 建立了 DSH Surface（WebView2、exact-origin、零 privileged IPC）；M4 需要独立的 Browser surface：用户在不离开 Desktop 的情况下浏览网页，并具备 profile 隔离与可审计的导航语义。RISK-BROWSER（credential/bridge exposure, critical）要求 isolated profile、无 raw CDP/WebView IPC、human takeover。AC-BRW-001（Browser page 无 Desktop IPC）与 AC-BRW-002（human takeover 撤销 Agent mutation lease）已在验收目录。

M4 之前没有任何 browser 能力：`specs/protocol/browser-capability.schema.json` 仅定义 broker 能力形状（method/mode/params），无操作级 request/report schema、无 fixture、无实现。

## Decisions

### 决策 1：Browser 是与 DSH Surface 完全分权的独立 WebView
- 独立 webview label `browser`；capability 精确匹配 `webviews: ["shell"]`（发起命令的 Shell WebView），`browser` 与 `dsh-surface` 均不匹配任何 privileged command permission。
- Browser 与 DSH WebView 无 privileged native bridge；ACL 三层闭合（AppManifest command inventory / permission / capability）纳入 validate-acl.mjs 门禁。
- Browser navigation 使用独立 navigation policy（见决策 3），与 DSH Surface 的 exact-origin 策略分离。

### 决策 2：M4 只实现 human_surface；agent_automation fail-closed 至 M5
- 与 ADR-0015（Terminal）同策略：request schema 中 `mode: "human_surface"`（const），agent_automation 由 schema fixture 与 bridge 双层拒绝。
- `interact`、`take_over` 操作**不在 M4 接口范围内**（IF-BROWSER operations = create/navigate/snapshot/close）；M5 agent 授权链落地时再扩展接口与 schema（M4 fail-closed 由 `agent_automation` mode 拒绝与 `screenshot` NOT_SUPPORTED 承担）。
- AC-BRW-002（human takeover 撤销 Agent mutation lease）依赖 M5 agent 授权链；M4 验收项调整为"agent_automation 模式请求 fail-closed"，AC 的 lease 撤销语义移至 M5 验收。

### 决策 2 修订（2026-08-30，M5-E3）：interact/take_over 扩展接口
- M5 授权链落地（ADR-0018 决策 7）后开放两个操作，schema 与 bridge 双层表达：
  - `interact`（browser-interact-request.schema.json）：**仅 agent_automation 模式**（`mode` const；human 自己操作浏览器，不提供 human interact）。payload 携带 agent 授权对象（agentId/activationId/generation/scope，与 terminal create 同形状），经共享 capability broker（agent_broker::BrokerState）的 ADR-0014 dispatch 门禁（browser capability + grant + owner + generation + scope 覆盖 + 有效 lease）后才执行。
  - `take_over`（browser-takeover-request.schema.json）：human 操作（sessionId + `target: "human"`，无 mode——操作本身是语义）。撤销绑定到该 session 的全部 agent activation lease（`Broker::revoke_agent_grants`，持久撤销），并将 session 标记 human-controlled；此后同一 activation 的 interact 拒绝。
- WebView2 无 CDP 输入 API：interact 的执行为最小实现——`evaluate_script`（ExecuteScript）在页面内派发 DOM 事件（click=MouseEvent 序列、type=原生 value setter + input/change、key=KeyboardEvent、scroll=window.scrollBy）；所有 caller 参数经 serde_json 编码为 JSON 字符串字面量，杜绝脚本注入；页面无 privileged Desktop IPC（AC-BRW-001）不受影响。
- IF-BROWSER operations 扩展为 create/navigate/snapshot/interact/take_over/close；AC-BRW-002 转 M5 verified 条件（takeover 撤销 lease + 后续 interact 拒绝，见 docs/testing/ACCEPTANCE.md）。

### 决策 3：导航与 profile 隔离策略
- 导航：仅 HTTP(S) scheme；URL 带 userinfo（credential）拒绝；长度上限 2048；file:/custom scheme/download/popup/permission 默认拒绝（沿用 M1 WebView2 deny 模式）。Browser URL 是用户意图（与 DSH Surface backend-derived URL 不同），但方案面仍由 backend 强制。
- profile：独立 user-data-dir（Desktop 拥有、AppData 下 `browser-profiles`，与 environment-catalog 同级），不共享 DSH 或默认 WebView2 数据；report/日志不得出现 profile 路径。
- WebView2 permission deny handler 在 remote navigation 前安装（与 ADR-0011 同模式）；`on_navigation` 策略独立于 DSH Surface evaluator。

### 决策 4：provider 抽象与 PoC
- `crates/browser-provider` 定义 `BrowserProvider` trait（create/navigate/snapshot/close + 状态事件），M4-C 实现 WebView2 provider（默认）。
- M4-B 以最小 PoC 验证外部 Edge + CDP 候选（受管 user-data-dir + --remote-debugging-port）；CDP 会话仅由 provider 内部驱动，禁止 raw CDP socket 暴露（WEBVIEW_ISOLATION 禁止 API）。PoC 报告后 maintainer 决定是否将 CDP provider 升级为正式实现。

### 决策 5：会话与所有权
- Browser 会话 opaque id `brw-<ms>-<seq>`（不泄 profile 路径/进程细节）；report 只暴露 id/state/currentUrl（sanitized）/createdAt。
- WebView2 provider 的浏览器页面运行于 Desktop 进程内（WebView2 子进程树归 Desktop）；CDP provider（若升级）的外部浏览器进程必须由 provider 管理进程树与端口生命周期（复用 M2 supervisor 模式），Attached 语义不在 M4。
- 所有 browser 命令经 Shell-only ACL；caller 除导航 URL 外不得提交 endpoint/profile/credential/capability 参数。

### 决策 6（2026-08-29 增补）：wry 版本策略——维持 tauri 锁定版本，permission/capture 用 webview2-com 直调

- 技术证据：tauri 2.11.5 依赖树锁定 `tauri-runtime-wry 2.11.4 -> wry 0.55.1`；升级 wry 0.56.1 需要升级 tauri（依赖树大更新 + M1-M3 回归面），不满足 M4 时间盒。
- 决策：维持 wry 0.55.1；permission deny 与截图（CapturePreview）通过 `webview2-com 0.38.2` 直调 `ICoreWebView2` 实现——与 M1 dsh-surface 的 PermissionRequested deny 同一模式（已验证）。
- PoC A 已确认：wry 0.55 的 `evaluate_script` 不返回值（须 `evaluate_script_with_callback`，且回调双重 JSON 编码）；无 capture API。
- wry 0.56 的 `with_permission_handler` 在 tauri 升级时（M6/M7）再评估。

## Consequences

- specs/browser/ 新增操作级 schema 与 fixtures（browser-create/navigate/snapshot/close/report + interact/takeover fail-closed fixtures）。
- IF-BROWSER spec 指针从 `specs/protocol/browser-capability.schema.json` 指向 `specs/browser/browser-report.schema.json`。
- validate-specs 门禁覆盖新 schema/fixtures；validate-acl 在 M4-C 扩展命令时同步。
- RISK-BROWSER 在 M4-C 实现与隔离矩阵验证后由 mitigating 评估为 mitigated（PoC 前保持 mitigating）。
- M5 需要为 browser mutation 建立 broker lease 授权链（AC-BRW-002）与 interact/take_over 实现。
## M8 增补：非 Windows 平台降级策略（2026-08-30，maintainer 拍板）

- **决策**：非 Windows 构建使用 **tauri/wry 默认 webview**（wkwebview/webkitgtk）创建
  browser 窗口，**无 WebView2 增强**（deny hooks、permission 拦截、ExecuteScript
  快照/交互均为 webview2-com 直调，Windows-only）。
- **能力标注**：降级平台的 browser 能力标记为 degraded——navigate/snapshot/
  interact 走 tauri eval 等价路径（wry 跨平台 API），权限拦截不可用（页面权限
  由 webview 默认策略决定）。
- **过渡**：当前实现为 fail-closed（非 Windows 返回 unsupported_platform，
  browser.rs 631-639）；降级实现排 M8-B（依赖 CI 验证，无本机 mac/linux 环境）。
- **安全影响**：AC-BRW-001（无 privileged IPC）不依赖 WebView2 直调——wry 默认
  webview 同样无 native bridge；降级仅失去 deny-hook 级权限控制（记录在案）。
