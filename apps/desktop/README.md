# Desktop Application

Tauri 2 + React/TypeScript Shell，承载 Activity Rail、Desktop-owned views 和 unmodified DSH Web Surface。Rust backend 只暴露结构化 commands，并通过 crates 中的模块管理 native lifecycle。

## Current implementation

M1 已建立可编译的桌面壳层、Environment 基线、Attached health、Managed readiness 与 native DSH Surface foothold：

- `shell-ui`：Activity Rail、Desktop layout、runtime 状态、Attached endpoint evidence、Managed start/status/generation-bound stop，以及 generation-bound native Surface mount/hide/reload/unmount 协调。
- `environment-settings`：`DshEnvironment` 草稿、前后端校验、catalog 持久化和 non-executing discovery。
- `harness-surface`：未配置、Attached read-only、Managed native lifecycle、viewport gate、sanitized error/retry 与 platform unsupported 状态。
- Rust backend：Environment catalog、Harness discovery、固定 750 ms 的 Attached loopback TCP reachability probe，以及 P0 integrated Managed Supervisor。Managed 仅启动 persisted Environment，使用结构化 argv、保留进程树 ownership，并在 owned `dsh web:` output 与 bounded TCP readiness 同时成立后发布 endpoint。
- DSH Surface：Windows 只从 Supervisor 的 current-generation verified Managed binding 创建 fixed-label child WebView；remote load 前安装 permission deny 与 autofill/password deny，cross-origin、popup、download、DOM injection、page eval 和 automatic external open 均拒绝。macOS/Linux/other 返回 `unsupported_platform` 且不创建 WebView。
- Tauri ACL：十六个 custom commands 全部进入 AppManifest 与最小 permission；capability 只匹配 `shell` WebView，`dsh-surface` 没有 privileged command 或 remote URL access。

Setup 可以显式保存 Environment，但不会安装、构建或自动启动 DSH。Managed start 必须由 Runtime Surface 显式触发；stop 必须二次确认并携带当前 generation。Windows native implementation 已通过静态、单元、ACL、前端和布局门禁，但真实 user-owned DSH + WebView2 smoke/negative matrix 尚未完成，因此当前不形成 Windows 支持声明。自动恢复属于 M2，P2 daemon 属于 M6；Attached 永远不提供 lifecycle mutation。

## Local verification

Node.js 24+、pnpm 11.19.0 与 Rust 1.98.0：

```powershell
Set-Location apps/desktop
pnpm install --frozen-lockfile
pnpm run check:acl
pnpm run check
pnpm run test
pnpm run build

Set-Location ../..
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

在 Windows UNC checkout 中，Node worker 和包管理器可能无法稳定继承临时盘符。应优先使用本地 checkout；CI 也必须在本地文件系统执行。

- [Features](features/README.md)
- [Local Agent Rules](AGENTS.md)
- [Development Contract](DEVELOPMENT.md)
