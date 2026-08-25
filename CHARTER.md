# Project Charter

## 愿景

让 DeepSeek Harness 可以快速升级、重启和演化，而用户的桌面工作环境、终端、浏览器 Surface 与运行控制仍保持稳定。

## 产品定义

DSH Desktop Shell 是用户自有 DSH 的跨平台 Desktop Control Plane、Native Capability Host 与 Workbench Shell。它不是另一个 DSH 发行版。

## 目标

- 发现、验证并连接用户已有 DSH 与 `DSH_HOME`。
- 提供 Managed 与 Attached 两种不混淆的 ownership 模式。
- 在不修改上游 Web UI 的前提下提供可靠的启动、健康、重启、恢复和诊断。
- 逐步提供 Persistent Terminal、Shared Browser、Usage、Notification 等能力。
- 通过稳定内部 Capability Contract 与可替换 Adapter 隔离 DSH 和 `dsh-std` 的变化。
- 让任何新的人类或 Agent 能从仓库真源恢复状态并安全继续工作。

## 非目标

- 不分发、下载、升级或 patch DSH Core、Node 或 pnpm。
- 不直接修改用户 Profile、`DSH_HOME` 或插件依赖。
- 不 fork、复制或 DOM patch upstream DSH Web UI。
- 不创建第二个 Desktop Plugin Market。
- 不将 Browser/Terminal 的高权限能力直接注入网页。
- P0 不实现 Remote、Cloud Relay、完整 Scheduler 或完整 Plugin HMR。

## Ownership Boundary

| 对象 | 权威 Owner | Desktop 行为 |
|---|---|---|
| Desktop 配置与 UI | Desktop | 读写与展示 |
| Supervisor 和 native providers | Desktop | 完全控制 |
| Managed DSH 进程 | Desktop 临时持有生命周期 | 启停、健康、恢复 |
| Attached DSH 进程 | 外部 owner | 只连接，不强杀或重启 |
| DSH 安装、Node、pnpm | 用户 | 只发现与验证 |
| `DSH_HOME`、Profile、插件、凭据 | 用户/DSH | 只引用，不读取凭据正文 |
| Agent、Session、Scheduler 语义 | DSH | 通过 Adapter 交互 |
| DSH Web UI | upstream | 原样承载 |

## 成功标准

- DSH restart 不要求退出 Desktop。
- Managed/Attached 的所有破坏性生命周期操作都能证明 process ownership。
- DSH 或 `dsh-std` adapter 不可用时能降级到 Web + lifecycle 基线，而非整体失效。
- DSH WebView 与任意 Browser 页面均无 privileged Tauri IPC。
- 每项能力有明确版本、授权、作用域、owner、审计与撤销路径。
- 所有项目状态、决策、风险、接口和 handoff 都能从仓库恢复。
