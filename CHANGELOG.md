# Changelog

本文件记录仓库级公开契约、治理与发布变化。模块级协议变化还必须更新对应 Schema、ADR 和 compatibility 记录。

## Unreleased

### Added

- 建立文档型 clean-room 项目仓库。
- 冻结 M0 架构基线、模块地图、协议草案和跟踪系统。
- 建立跨 Agent / Session 的工作项、lease、evidence 与 handoff 规则。
- 为 Supervisor wake guarantee 增加独立 ScheduleWake Schema。
- 增加可重现的 external baseline，记录官方 commit、release、npm dist-tag 与 artifact integrity。
- 启动 M1 Shell MVP：加入 Tauri 2 + React/TypeScript + Rust workspace、Activity Rail、Environment validation、DSH placeholder Surface 与项目自有平台图标。
- 加入 Tauri custom-command inventory / capability ACL 机器门禁，以及前后端 Environment 验证测试。
- 冻结 `EnvironmentCatalog` v1 与 non-executing Harness discovery request/report Schema、正负 fixtures 和迁移规则。
- 实现 AppData/Application Support-owned Environment catalog：revision、active selection、backup 与损坏拒绝均有 Rust 测试覆盖，且不写入 DSH_HOME/Profile。
- 实现不执行候选的 Harness discovery、Shell-only Tauri command ACL，以及发现、校验、显式保存与启动恢复 UI。
- 冻结 AttachedHealth request/report：只允许 persisted Environment 的 fixed-loopback bounded TCP reachability，identity 永远 unverified、process ownership 永远 external、lifecycle mutation 永远 denied。
- 实现 Shell-only Attached health probe 与 Runtime evidence UI，覆盖自动/手动探测、固定端口错误、无 lifecycle controls、ACL、Rust/React tests 和浏览器视觉验收。
- 冻结 `IF-DSH-SURFACE-POLICY` v1alpha1：policy 只从 persisted fixed-loopback Environment 派生，exact same-origin main-frame navigation 可留在 Surface，其余 loopback/credential/scheme/popup/download/permission 默认拒绝，external HTTP(S) 只产生待 human confirmation 的 delegate decision。
- 实现 Rust DSH Surface policy evaluator、八命令 Shell-only ACL 与只读 policy preview；21 项 Rust 测试覆盖 IPv4/IPv6 loopback、credential/scheme 和外链委派，18 项前端测试及浏览器视觉验收证明尚未创建远程 WebView。
- 冻结 M1 Managed Runtime v1alpha1：start/status/generation-bound stop、retained process-tree ownership，以及 owned `dsh web:` output 与 bounded TCP readiness 同时成立后才发布 endpoint。
- 实现 P0 integrated Managed Supervisor：结构化 launch、Windows Job Object/Unix process group、exact-generation stop、endpoint release、11-command Shell-only ACL，以及 Managed Runtime evidence UI；remote DSH WebView、自动恢复与 daemon 仍保持未实现。
- 接受 ADR-0011：M1 native DSH Surface 采用 Windows WebView2 permission-deny foothold，macOS/Linux/other 在具备等价可验证 hook 前显式 fail closed。
- 冻结并实现 `IF-DSH-SURFACE-LIFECYCLE`：Shell 只提交 Environment ID、expected generation、logical bounds 与 visibility；backend 从 Supervisor verified binding 派生 URL，提供 mount/status/layout/reload/unmount 并拒绝 caller endpoint/origin/URL/label。
- Windows `dsh-surface` child 在 remote load 前安装 WebView2 permission、password autosave 与 autofill deny，exact-origin navigation 以外的 navigation、popup、download 全部拒绝；capability 精确限定 `webviews: ["shell"]`，remote child 无 privileged Tauri IPC。macOS/Linux/other 显式 `unsupported_platform`。
- Shell UI 接入 native lifecycle、rail hide、generation-bound retry、binding-loss unmount 与 320 × 240 visible-bounds gate；22 项 frontend tests、30 项 Rust tests、strict Clippy、Vite build、16-command ACL 与 1280/420/390px visual QA 通过。真实 user-owned DSH/WebView2 smoke 仍待独立验收。
- 冻结 `IF-DSH-SURFACE-LIFECYCLE` v1alpha1：generation-bound mount/status/layout/reload/unmount 请求不接受 caller endpoint/origin/label，状态显式区分 Windows ready 与其他平台 unsupported。
- 接受 ADR-0012：Managed Repository 可使用 persisted absolute `nodePath + built harness.path` 结构化启动；current DSH authenticated Web bootstrap credential 只保留在 Supervisor generation 并直接交给 unprivileged native Surface，public report/IPC/log/tracking 继续 credential-free。

### Changed

- Maintainer 已接受 native DSH Surface implementation slice；本次接受关闭原 session 并释放 advisory claim，但不关闭 `WI-M1-SHELL`、安全审查或 M1，也不构成 Windows support 声明。
- 收紧 v1alpha1 Envelope：Hello/Agreement payload 结构化，Agreement 绑定 replyTo，Invocation/Result/Event kind 字段受限，Result success/error 互斥且 error 强制 correlation ID。
- CapabilityLease 禁止空 scope；Usage period 拒绝未知字段。
- 将 M1 DSH fixtures 固定为 `0.1.1-rc.2`/`0.1.1-rc.1`，并显式区分 dsh-std 的 `latest` 与 `rc` 标签。
- Managed DSH launch 固定 loopback、`--no-open` 与可验证的 auto-port 流程；source checkout 仅接受用户预构建产物。
- Managed readiness candidate 从仅 credential-free root 扩展为 legacy root 或 exact backend-owned token root；其他 query/credential 仍 fail closed。该变化不扩大 caller request、公开 endpoint 或 WebView privileged capability。
- 收紧 Tauri 自定义命令门禁：完整 AppManifest inventory、最小 permission、精确 Shell label，并禁止 invoke-handler-only command。
