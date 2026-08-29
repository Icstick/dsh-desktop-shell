# Current Project State

- Phase：`shell-mvp`
- Milestone：M3 Workbench —— done（2026-08-29 maintainer 接受）
- Status：M1/M2/M3 已接受并 squash 合并 main；下一里程碑 M4 Shared Browser（ready，待认领）
- Implementation authorized：`true`
- External baseline verified：2026-08-25
- Last updated：2026-08-29T10:05:00Z

## 当前状态

- M0 Architecture Freeze：done（main @ 44fa9bc merge）。
- M1 Shell MVP：done。REVIEW-M1-ACCEPTANCE（2026-08-29 maintainer 接受）；REVIEW-M1-NATIVE-ACCEPTANCE 9/9；Windows real-DSH WebView2 native smoke/compatibility 26/26（SMOKE-20260828-WEBVIEW2-NATIVE）；ADR-0012 authenticated bootstrap 端到端验证。已 squash 合并 main。
- M2 Reliable Runtime：done。REVIEW-M2-ACCEPTANCE；REVIEW-M2-HANDOFF-CONSISTENCY 证据面全过（failed 项为 tracking 同步问题，已全部修复）；四切片：restart/recovery/Safe Stop（ADR-0013）、diagnostics（AC-LOG-001）、local-transport（AC-IPC-001/002）、P0 capability broker（ADR-0014，AC-LEASE-001）；98 Rust / 25 vitest / 18 ACL / 41-34 specs。已 squash 合并 main。
- M3 Workbench：done。REVIEW-M3-ACCEPTANCE；REVIEW-M3-WORKBENCH 独立评审内容层全过（gate-counts 因本机无 Rust 工具链为静态核验；tracking-consistency 问题已在本收尾修复）；三切片：Notification（AC-NOT-001/002）、Persistent Terminal（ADR-0015，Windows ConPTY + xterm，AC-PTY-001）、Usage（AC-USG-001/002）；132 Rust / 29 vitest / 28 ACL / 53-55 specs。已 squash 合并 main。

## remaining（不阻塞，均如实记录）

- macOS/Linux target-host 实测证据（unsupported_platform / PTY）。
- live desktop QA：交互式 GUI/WebView2 smoke、真实 DSH restart 演示、TerminalPanel 前端自动化用例恢复、ConPTY reader 队列满场景检查。
- diagnostics 专项 UI 与 redacted log-export（当前由 RuntimePanel 只读块满足 AC-LOG-001）。
- agent_automation 终端模式与 DSH notification/usage adapter：至 M5（AC-TERM-001、ADR-0016 决策 5）。
- DSH graceful-stop stopDisposition=forced：待 DSH CLI 侧确认。

## 环境注意（2026-08-29）

当前开发机无 Rust 工具链（cargo/rustup 缺失，target/ 缓存为 D:\HostShare + C:\Users\ZOOT 的他机产物）；M1-M3 的 Rust 门禁在原构建环境验证，本机由 REVIEW-M3-WORKBENCH 静态精确核验一致（131+1 doctest）。前端依赖已恢复（pnpm install --frozen-lockfile）。M4 的 Rust 开发前需安装 rustup。

## 已完成（里程碑摘要）

- M0：Charter、10 ADR、Schema、威胁模型、tracking 体系；M0 review 全通过；implementation_authorized=true。
- M1：Environment catalog/discovery（non-executing）、Attached 分权、DSH Surface policy（exact-origin）、Managed runtime（generation-bound）、Windows native surface（WebView2 deny hooks）、真实 DSH 26/26 smoke。
- M2：supervisor restart/recovery/Safe Stop、crash-loop fuse、redacted diagnostics、authenticated loopback transport、P0 capability broker。
- M3：notification 内容策略 + 60s TTL 去重 + 审计 JSONL、persistent ConPTY terminal（Desktop-owned）、local-first usage（零网络）。

逐 slice 明细见 `tracking/sessions/` 与 `tracking/reviews/`。

## 当前门禁

`implementation_authorized: true` 允许在已认领工作项范围内进入实现，但不豁免 branch/session/lease、接口优先、ADR、模块安全审查、clean-room 与验证证据要求。

## 下一动作

M4 Shared Browser（WI-M4-BROWSER proposed，依赖 WI-M3-WORKBENCH done）：contract-first 规划——IF-BROWSER 契约冻结（schema/fixtures、AC-BRW-001/002 与安全隔离）+ provider PoC 选型（至少两个 candidate）+ PLAN-M4.md；规划完成后认领 WI-M4-BROWSER、建 `codex/wi-m4-browser` 分支、记录 session 与 24h advisory lease。
