# Compatibility Matrix Plan

Release matrix 维度：

- OS：Windows 10/11、macOS arm64/目标 Intel、Linux 目标发行版。
- WebView：WebView2、WKWebView、WebKitGTK，Linux X11/Wayland。
- DSH：latest、N-1、upstream main advisory。
- 来源：PATH、global、source checkout、custom executable。
- Ownership：Managed、Attached。
- Profile：clean、mature、plugin-heavy、broken boot。
- Port/process：free、occupied、hijacked、delayed release、stale PID、orphan child。
- Adapter：absent、legacy、known std、unknown std。
- Protocol：Hello/Agreement required/optional、success/error Result、unknown field/version、stale generation、lease revoke/expiry。
- Transport：native、fallback、invalid/replay。
- Provider：PTY/Browser crash/reconnect。
- DSH Surface navigation：exact origin、external HTTP(S) with/without gesture、loopback mismatch、credentialed URL、non-HTTP scheme、popup、download、permission。
- DSH native Surface：Windows WebView2 permission-deny/lifecycle；macOS WKWebView、Linux WebKitGTK 和 other 的 explicit unsupported/fail-closed。
- Managed readiness：owned/foreign output、legacy/authenticated root、malformed/duplicate token、auto/fixed/mismatched port、early exit、timeout、stale generation、graceful/forced tree cleanup、source checkout missing artifact、structured repository Node recipe。

正式支持声明只能来自可复查 matrix evidence。

## 当前冻结 Fixture 坐标

以下坐标只定义 M1/M5 测试输入，不构成支持声明：

- DSH latest：`@deepseek-ai/dsh@0.1.1-rc.2`，必须验证 registry integrity。
- DSH N-1：`@deepseek-ai/dsh@0.1.1-rc.1`，必须验证 registry integrity。
- DSH upstream advisory：`master@cd5ef8148158c3a752a658978873241fdf8e2bbc`；源码行为不能替代已发布 package fixture。该 coordinate 的 authenticated root 只形成 advisory compatibility input。
- dsh-std known candidates：分别覆盖 `@dsh-std/core@0.1.0-rc1` (`latest`) 与 `@dsh-std/core@0.1.1-rc.1` (`rc`)；M5 选择前运行 conformance。

SHA-1/SHA-512 artifact 值、发布时间和 immutable evidence 统一引用 [External Baseline](../research/EXTERNAL_BASELINE.md)，不得在本文件重复维护。

DSH Surface policy serialization 与 semantic coordinates 固定在 [`specs/webview/fixtures/`](../../specs/webview/fixtures/)；这些 fixture 不创建 WebView，也不构成任一 WebView2/WKWebView/WebKitGTK 平台支持声明。

M1 native Surface support 由 `ADR-0011` 限定为 Windows foothold。只有可复查 Windows WebView2 smoke/negative evidence 才能形成 Windows 支持声明；macOS、Linux 与其他平台的预期结果是 `unsupported_platform` 且不创建 WebView，不得记为功能通过。

截至 2026-08-28，Windows foothold 已完成 contract、Rust implementation、exact Shell-WebView ACL、54 项 Rust tests、25 项 frontend tests、strict Clippy、production build、responsive Shell visual QA，并通过真实 user-owned DSH WebView2 smoke/negative matrix（26/26，token exchange、clean-root、cross-origin/popup/download/permission deny、resize/hide/show/reload/unmount、stop 后 binding fail-closed）。Windows support coordinate 仍由 maintainer 评审后决定（当前 `implementation_review`，不是 `supported`）。macOS/Linux/other 仍只允许 `unsupported_platform` negative coordinate（target-host 证据待排期）。

Managed Runtime request/report serialization 与 semantic gates 固定在 [`specs/runtime/fixtures/`](../../specs/runtime/fixtures/)；M1 只验证 integrated P0 start/status/stop foothold，不构成 M2 recovery、daemon 或三平台 process-tree hardening 支持声明。

2026-08-28 的真实 user-owned advisory checkout smoke 发现 `dsh web:` 已从 credential-free root 变化为 fresh process-token root，且 Windows source checkout 的 built CLI 需要 Node。ADR-0012 将 token 限定为 backend-only generation binding，并复用 Environment v1 的 `nodePath` 做无 shell的结构化 launch。实现与真实 WebView2 token exchange evidence完成前，该 coordinate 仍为 `compatibility_review`。
