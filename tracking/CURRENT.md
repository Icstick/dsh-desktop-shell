# Current Project State

- Phase：`shell-mvp`
- Milestone：M2 Reliable Runtime（M1 收尾证据已提交，见 HANDOFF-M1-NATIVE-ACCEPTANCE）
- Status：`in_progress`
- Implementation authorized：`true`
- External baseline verified：2026-08-25
- Last updated：2026-08-28T17:10:00Z

## 当前状态

- Maintainer 已接受 [HANDOFF-M0-REVIEW](handoffs/HANDOFF-M0-REVIEW.yaml)，M0 与 [WI-M0-REVIEW](work-items/WI-M0-REVIEW.yaml) 已完成。
- Maintainer 已接受 [HANDOFF-M1-DSH-SURFACE-NATIVE-SLICE](handoffs/HANDOFF-M1-DSH-SURFACE-NATIVE-SLICE.yaml)；接受证据见 [REVIEW-M1-DSH-SURFACE-NATIVE-SLICE-ACCEPTANCE](reviews/REVIEW-M1-DSH-SURFACE-NATIVE-SLICE-ACCEPTANCE.yaml)。M1 与 [WI-M1-SHELL](work-items/WI-M1-SHELL.yaml) 仍为 `in_progress`，当前由 [SESSION-20260828-M1-NATIVE-ACCEPTANCE-CONT](sessions/SESSION-20260828-M1-NATIVE-ACCEPTANCE-CONT.yaml) 持有 active claim（至 2026-08-29T15:18:33Z）。
- Windows real-DSH WebView2 native smoke/compatibility matrix 已通过（26/26）：真实 user-owned DSH checkout 经 ADR-0012 repository recipe 启动，DSH UI 在 dsh-surface child WebView 内完成 token exchange 并停留于 clean exact-origin root；cross-origin/popup/download/permission deny、resize/hide/show/reload/unmount、stop 后 binding fail-closed 与进程树清理全部验证；证据见 [SMOKE-20260828-WEBVIEW2-NATIVE](../docs/testing/SMOKE-20260828-WEBVIEW2-NATIVE.md)、[evidence JSON](../docs/testing/evidence/SMOKE-20260828-WEBVIEW2-NATIVE.json) 与 [HANDOFF-M1-NATIVE-ACCEPTANCE](handoffs/HANDOFF-M1-NATIVE-ACCEPTANCE.yaml)。ADR-0012 相关 5 个提交已推送至 `origin/codex/wi-m1-shell-native-acceptance`。
- 剩余：macOS/Linux `unsupported_platform` target-host 实测证据与 M1 最终 acceptance review（独立评审已由 REVIEW-M1-NATIVE-ACCEPTANCE 子代理执行）。
- M2 Reliable Runtime：由 [SESSION-20260828-M2-RELIABLE-RUNTIME](sessions/SESSION-20260828-M2-RELIABLE-RUNTIME.yaml) 持有 [WI-M2-RUNTIME](work-items/WI-M2-RUNTIME.yaml) claim；按 [PLAN-M2](../docs/roadmap/PLAN-M2.md) 完成 M2-A Supervisor restart/recovery/Safe Stop（ADR-0013）、M2-B Diagnostics（AC-LOG-001）、M2-C local-transport（AC-IPC-001/002）、M2-D P0 Capability Broker（ADR-0014，AC-LEASE-001）；Rust 98 tests（54 桌面 + 28 local-transport + 15 supervisor + 1 doctest）、vitest 25、ACL 18 commands、41 schemas/34 fixtures 全通过；M2 收尾审查与加固（async 命令、restart force-unmount、死代码移除、注释与文档统一）已完成，审查记录见 REVIEW-M2-HANDOFF-CONSISTENCY / REVIEW-M2-HARDENING-SECURITY / REVIEW-M2-HARDENING-REDUNDANCY / REVIEW-M2-HARDENING-DOCS；收尾证据见 [HANDOFF-M2-RELIABLE-RUNTIME](handoffs/HANDOFF-M2-RELIABLE-RUNTIME.yaml)。本阶段搁置 macOS/Linux 与交互式 GUI 测试。

## 已完成

- 文档型仓库、Charter、架构、代码地图和治理基线。
- 10 个初始 ADR。
- 协议/config/tracking JSON Schema。
- M0–M7 路线、模块与接口登记。
- Threat model、compatibility、test、operations 和 clean-room policy。
- 结构化质量门禁通过；证据见 [REVIEW-M0-STRUCTURE](reviews/REVIEW-M0-STRUCTURE.yaml)。
- Architecture、Security、Interop 语义审查通过；证据分别见 [REVIEW-M0-ARCHITECTURE](reviews/REVIEW-M0-ARCHITECTURE.yaml)、[REVIEW-M0-SECURITY](reviews/REVIEW-M0-SECURITY.yaml)、[REVIEW-M0-INTEROP](reviews/REVIEW-M0-INTEROP.yaml)。
- 全仓最终门禁通过；证据见 [REVIEW-M0-FINAL-GATE](reviews/REVIEW-M0-FINAL-GATE.yaml)。
- 外部 baseline 已刷新并固定 repository revision、registry artifact、Tauri security 语义与许可边界；证据见 [REVIEW-M0-EXTERNAL-BASELINE](reviews/REVIEW-M0-EXTERNAL-BASELINE.yaml)。
- Baseline-aware M0 复审已修正 DSH Managed launch 与 Tauri custom command ACL 门禁，且未发现需要替代的 ADR；证据见 [REVIEW-M0-BASELINE-REASSESSMENT](reviews/REVIEW-M0-BASELINE-REASSESSMENT.yaml)。
- Maintainer 已显式批准实现授权；证据见 [REVIEW-M0-AUTHORIZATION](reviews/REVIEW-M0-AUTHORIZATION.yaml)。
- 首个 M1 纵向切片已建立 Tauri 2 / React / Rust workspace、Activity Rail、read-only Environment validation、Harness placeholder 和精确 Shell-only custom-command ACL；证据见 [REVIEW-M1-SHELL-SLICE-1](reviews/REVIEW-M1-SHELL-SLICE-1.yaml) 与 commit `1c5f5ab`。
- 前端 ACL、TypeScript、9 项 Vitest 和生产构建通过；Rust fmt/check、3 项单测与 `clippy -D warnings` 通过。未安装、启动或修改用户 DSH。
- Maintainer 已明确接受首个 slice handoff；接受记录见 [REVIEW-M1-SHELL-SLICE-1-ACCEPTANCE](reviews/REVIEW-M1-SHELL-SLICE-1-ACCEPTANCE.yaml)。该决定不关闭 `WI-M1-SHELL` 或 M1。
- EnvironmentCatalog v1、Harness discovery request/report 与 fixtures 已 contract-first 冻结；实现位于 commit `36c6af6`，UI 集成位于 commit `0739734`。
- Platform AppData/Application Support-owned catalog 支持 revision、active selection、backup 与 corrupt-data fail-closed；Rust 测试证明不会写 DSH_HOME/Profile。
- Discovery 只检查 explicit、DSH_PATH 与 PATH filesystem metadata，不执行候选、shell、package manager、安装、构建或 version probe；global discovery 明确 deferred。
- Shell UI 已支持 candidate selection、backend validation、explicit save、active Environment startup restore 与 snapshot refresh；保存不会启动 DSH。
- 本 slice 前端 TypeScript、13 项 Vitest、Vite build、Shell-only 五命令 ACL 与浏览器视觉 QA 通过；Rust fmt/check、10 项单测与 strict Clippy 通过。证据见 [REVIEW-M1-ENV-DISCOVERY-SLICE](reviews/REVIEW-M1-ENV-DISCOVERY-SLICE.yaml)。
- Maintainer 已明确接受 Environment persistence/discovery slice；接受记录见 [REVIEW-M1-ENV-DISCOVERY-SLICE-ACCEPTANCE](reviews/REVIEW-M1-ENV-DISCOVERY-SLICE-ACCEPTANCE.yaml)。`IF-ENVIRONMENT` 升为 `verified`，但该决定不关闭 `WI-M1-SHELL` 或 M1。
- AttachedHealth request/report、fixtures、ADR/lifecycle/compatibility 规则与 `AC-OWN-002` 已 contract-first 冻结；实现位于 commit `f1eb75d`，Runtime UI 位于 commit `5b9b761`。
- Attached probe 只解析 persisted fixed-loopback Environment，以 backend-owned 750 ms timeout 做一次 TCP connect；identity 永远 `unverified`、process ownership 永远 `external`、lifecycle mutation 永远 `denied`。
- 本 slice 前端 TypeScript、16 项 Vitest、Vite build、Shell-only 六命令 ACL 与浏览器视觉 QA 通过；Rust fmt/check、15 项单测与 strict Clippy 通过。证据见 [REVIEW-M1-ATTACHED-HEALTH-SLICE](reviews/REVIEW-M1-ATTACHED-HEALTH-SLICE.yaml)。
- Maintainer 已明确接受 Attached health/status slice；接受记录见 [REVIEW-M1-ATTACHED-HEALTH-SLICE-ACCEPTANCE](reviews/REVIEW-M1-ATTACHED-HEALTH-SLICE-ACCEPTANCE.yaml)。该决定不关闭 `IF-RUNTIME-STATUS`、`WI-M1-SHELL` 或 M1。
- `IF-DSH-SURFACE-POLICY` v1alpha1、4 个 Schema、5 个 fixtures、`AC-WEB-004` 与 ADR/security/migration/compatibility 规则已 contract-first 冻结；实现位于 commit `7c38238`，Shell preview 位于 commit `a0f9bd9`。
- Policy 只从 persisted fixed `http://127.0.0.1:<port>` Environment 派生；exact same-origin main-frame navigation 可通过，另一 loopback/credential/non-HTTP/popup/download/permission 默认拒绝，external HTTP(S) 只产生待 human confirmation 的 delegate decision。
- 本 slice 八命令 Shell-only ACL、Rust fmt/check/21 项测试/strict Clippy、TypeScript/18 项 Vitest/Vite build 与浏览器视觉 QA 全部通过；没有创建远程 WebView。证据见 [REVIEW-M1-DSH-SURFACE-POLICY-SLICE](reviews/REVIEW-M1-DSH-SURFACE-POLICY-SLICE.yaml)。
- Maintainer 已明确接受 DSH Surface policy slice；接受记录见 [REVIEW-M1-DSH-SURFACE-POLICY-SLICE-ACCEPTANCE](reviews/REVIEW-M1-DSH-SURFACE-POLICY-SLICE-ACCEPTANCE.yaml)。`IF-DSH-SURFACE-POLICY` 升为 `verified`，但本次批准不授权创建远程 DSH WebView，也不关闭 `WI-M1-SHELL` 或 M1。
- Managed Runtime start/status/exact-generation stop request/report、4 个 Schema、5 个 fixtures、`AC-RUN-005` 与 ADR/lifecycle/security/compatibility 规则已 contract-first 冻结；实现位于 commit `c737af9`，Shell controls 位于 commit `2aa332a`。
- P0 integrated Supervisor 只从 persisted Managed Environment 派生结构化 launch，强制 loopback 与 `--no-open`，保留 child/process-tree ownership；只有 current generation 的 exact `dsh web:` output 与 bounded TCP connect 同时成立才发布 endpoint。
- Windows Job Object child-tree cleanup、stale generation、malformed/foreign/wrong-port readiness、endpoint release 和 Drop cleanup 均有 Rust 测试；Attached mutation、PID/port ownership inference、Node override 与 source auto-build 均 fail closed。
- 本 slice 11-command Shell-only ACL、Rust check/27 项测试/strict Clippy、TypeScript/19 项 Vitest/24-module Vite build 与浏览器视觉 QA 全部通过；没有启动用户 DSH，也没有创建 remote WebView。证据见 [REVIEW-M1-MANAGED-READINESS-SLICE](reviews/REVIEW-M1-MANAGED-READINESS-SLICE.yaml)。
- Maintainer 已明确接受 Managed readiness slice；接受记录见 [REVIEW-M1-MANAGED-READINESS-SLICE-ACCEPTANCE](reviews/REVIEW-M1-MANAGED-READINESS-SLICE-ACCEPTANCE.yaml)。`IF-RUNTIME-CONTROL` 升为 `verified`，但 `WI-M1-SHELL` 与 M1 仍需完成实际 DSH Surface。
- `ADR-0011` 与 `IF-DSH-SURFACE-LIFECYCLE` 已 contract-first 冻结；实现位于 commit `a8f0b52`，TypeScript binding 位于 `1862caf`，Shell orchestration 位于 `7260099`，minimum viewport hardening 位于 `6ac3539`。
- Windows child 只消费 Supervisor current-generation verified binding；以 fixed label `dsh-surface` 从 `about:blank` 创建，在 remote navigation 前安装 WebView2 permission/autofill/password deny，并拒绝 cross-origin、popup、download、DOM injection、page eval 与 automatic external open。macOS/Linux/other 在创建前返回 `unsupported_platform`。
- Tauri capability 精确匹配 `webviews: ["shell"]`；十六个 custom commands 的 AppManifest/invoke/permission inventory 完全一致，`dsh-surface` 不匹配任何 privileged command 或 remote URL access。
- Shell 已实现可视 slot bounds、mount/status/layout/reload/unmount、rail hide、binding-loss cleanup、generation-bound retry 与 320 × 240 minimum gate；frontend 22 项 tests、Rust 30 项 tests、strict Clippy、Vite build、ACL 与 1280/420/390px visual QA 通过。证据见 [REVIEW-M1-DSH-SURFACE-NATIVE-SLICE](reviews/REVIEW-M1-DSH-SURFACE-NATIVE-SLICE.yaml)。
- 本 session 没有启动 user-owned DSH；automated 与 Shell visual QA 不替代真实 WebView2 permission/redirect/popup/download/load-failure matrix，因此当前不声明 Windows support，M1 与 `WI-M1-SHELL` 仍为 `in_progress`。
- Maintainer 已明确接受 native DSH Surface implementation slice；接受记录见 [REVIEW-M1-DSH-SURFACE-NATIVE-SLICE-ACCEPTANCE](reviews/REVIEW-M1-DSH-SURFACE-NATIVE-SLICE-ACCEPTANCE.yaml)。该决定关闭本次 session 并释放 advisory claim，但不关闭 `WI-M1-SHELL`、接口安全审查或 M1，也不构成 Windows support 声明。
- ADR-0012 Authenticated Managed Web Bootstrap 已 contract-first 冻结并实现：`nodePath` 限定 Managed Repository、结构化 Node launch、认证 candidate 解析、backend-only bootstrap URL 生命周期与 redaction 均通过 Rust 35 项、frontend 22 项、strict Clippy、TypeScript、Vite build 与 ACL 门禁；真实 DSH Windows WebView2 smoke 尚未执行。

## 当前门禁

`implementation_authorized: true` 允许在已认领工作项范围内进入实现，但不豁免 branch/session/lease、接口优先、ADR、模块安全审查、clean-room 与验证证据要求。

## 下一动作

在 [SESSION-20260828-M1-NATIVE-ACCEPTANCE](sessions/SESSION-20260828-M1-NATIVE-ACCEPTANCE.yaml) 中继续执行 Windows real-DSH native smoke/compatibility acceptance，并在 macOS/Linux host 复核 `unsupported_platform`。已接受的 handoff 不声明平台 support，也不关闭 M1。
