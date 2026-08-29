---
id: ADR-0011
status: accepted
date: 2026-08-28
owner_role: architecture-owner
---

# ADR-0011: Platform-gated Native DSH Surface

## 背景

M1 需要把 Supervisor 发布的 current-generation Managed endpoint 承载到 Shell window 内的原生 child WebView，同时继续满足 ADR-0004 的零 privileged IPC、零 DOM injection 和 fail-closed 页面权限边界。

仓库锁定 `tauri=2.11.5`、`tauri-runtime-wry=2.11.4` 与 `wry=0.55.1`。Tauri 的 child `WebviewBuilder` 需要 `unstable` feature，并提供 navigation、new-window、download 与 page-load hooks；其 navigation hook 只给 URL，不提供可验证的 user-gesture 证据。锁定的 Wry 在 Windows WebView2 上没有跨平台 builder-level permission deny API，但可通过 Tauri `with_webview` 和锁定的 `webview2-com=0.38.2` 挂接原生 `PermissionRequested` deny handler。对应版本的 WKWebView 路径在未设置 permission handler 时会 grant media capture；当前 Tauri builder 未暴露足以证明该 deny handler 已安装的稳定 API。

以上外部事实于 `verified_on: 2026-08-28` 核验，来源见 [External Baseline](../research/EXTERNAL_BASELINE.md) 与 [Source Register](../compliance/SOURCE_REGISTER.yaml)。

## 决策

1. M1 原生 DSH Surface 只在 Windows 创建 `dsh-surface` child WebView；实现必须在首个远程 document load 前安装 WebView2 `PermissionRequested` 全拒绝 handler。
2. macOS、Linux 和其他平台在 permission-deny 证据可用前返回结构化 `unsupported_platform` 状态并保持 unmounted；不得回退到 iframe、独立 privileged window 或系统浏览器自动打开。
3. child WebView 只绑定 Supervisor 当前持有、current generation 且 readiness 已验证的 Managed endpoint。caller 只能提供 Environment ID、expected generation、bounds 和 visibility，不能提供 URL、origin、label、permission 或 capability。
4. 原生 navigation hook 只允许 verified exact-origin HTTP main-frame URL。由于 hook 没有可信 user-gesture 证据，所有 cross-origin navigation 在 Surface 内拒绝；外链委派留给未来有显式 human confirmation provenance 的 Shell flow。
5. popup/new-window、download、clipboard permission、media/geolocation/notification 等页面 permission 一律拒绝；不注入 initialization script，不调用 page `eval`，不 patch renderer。
6. `dsh-surface` 不匹配任何 Tauri capability、custom command permission 或 remote URL ACL。Surface lifecycle commands 仅由 `shell` label 的最小 permission 调用。
7. child WebView API 的 `unstable` feature 是受控依赖例外；升级 Tauri/Wry 时必须重跑平台 permission 与 lifecycle matrix，不能把 feature 名称当作稳定性承诺。

## 替代方案

- 等待跨平台统一 permission hook 后再交付：安全边界简单，但会阻断 Windows M1 foothold 和后续 lifecycle 验证。
- 在 macOS/Linux 接受 WebKit 默认 prompt/grant：无法证明 fail-closed，拒绝。
- iframe 或独立 WebviewWindow：无法满足同一 Shell surface 布局/隔离目标，且不会自动解决权限边界。
- 注入脚本拦截页面 API：违反 ADR-0004，且可被页面行为或导航绕过。
- 允许 cross-origin 并依赖 external opener：native hook 缺少 user-gesture/human-confirmation provenance，拒绝。

## 后果

- Windows 可以先形成可验证的 native Surface lifecycle、loading/error/reconnect foothold。
- M1 不宣称 macOS/Linux native DSH Surface 支持；这些平台明确、可观测地 fail closed。
- Windows 实现包含小范围 platform-specific COM 代码，需要 pinned dependency、负向测试与升级审计。
- 原生 Surface 的 external navigation 比 policy evaluator 更严格；现有 `delegate_with_user_action` 仍是未来 Shell-confirmed flow 的 contract，不由当前 native hook 消费。

## 验证门禁

- `AC-WEB-005`：只有 verified current-generation Managed binding 可 mount，stale/unowned/unready binding 拒绝。
- `AC-WEB-006`：Windows 在 load 前安装全 permission deny，exact-origin navigation 可用，cross-origin/popup/download/permission 全拒绝。
- `AC-WEB-007`：macOS/Linux/other 返回 `unsupported_platform` 且无 WebView 被创建。
- ACL inventory 证明 `dsh-surface` 无 capability、permission 与 remote URL access；request Schema 不允许 endpoint/origin/URL/label/permission 字段。
- Rust contract/negative tests、React lifecycle tests、Windows native smoke evidence和 dependency/version review 同时通过后，接口状态才可 `verified`。

## 受影响模块

- `MOD-HARNESS-SURFACE`
- `MOD-SHELL-UI`
- `MOD-SUPERVISOR`

