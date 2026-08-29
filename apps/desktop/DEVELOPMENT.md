# Desktop Development Contract

前端只消费 versioned contracts 与 Tauri command facade，不管理 process、PTY 或 raw transport。DSH/Browser WebViews 无 privileged commands。所有 custom Tauri commands 必须登记到 `tauri_build::AppManifest::commands`，再通过最小 permission 和精确 Shell label 授权；禁止 invoke-handler-only command。UI 状态必须映射 backend canonical state，不能自行推断 ownership/health。

M1 前先验证三类 WebView capability、AppManifest command inventory、最终权限合并、external navigation、reconnect overlay 和 first-run Setup。具体功能位于 features 子目录。

## Implemented M1 slice gates

- `scripts/validate-acl.mjs` 必须证明 build-time command inventory、invoke handler 和 capability permission 集合一致。
- `tauri.conf.json` 只能向精确的 `shell` label 分配 capability；远程 URL 和额外 WebView 不得获得权限。
- `DshEnvironment` 在前后端均做结构校验；Supervisor-owned 参数不得从 user args 穿透。
- 前端测试必须覆盖默认草稿、保留参数拒绝、Shell 导航与 Setup 验证交互。
- Environment catalog 与 discovery 必须保持 AppData-owned、non-executing、non-installing 和 DSH_HOME/Profile non-mutation。
- Attached health 只接受 persisted fixed-loopback Environment，并以 backend-owned 750 ms timeout 做一次 TCP connect；UI 必须同时显示 identity `unverified`、process ownership `external` 和 lifecycle mutation `denied`。
- DSH Surface policy 只从 persisted fixed `http://127.0.0.1:<port>` 派生；caller 不得提供 allowed origin/label/grant。Same-origin main-frame 之外默认拒绝，external HTTP(S) 只返回待 human confirmation 的 delegate decision，不能自动打开。
- `dsh-surface` 不得出现在 Tauri window/webview target 或 remote capability；policy evaluator source 不得创建 WebView、注入脚本、调用 opener 或执行页面代码。
- Managed lifecycle request 只接受 persisted Environment ID；caller 不得提供 executable、argv、cwd、host、port、endpoint、instance 或 ownership。
- Managed start 必须使用结构化 argv 固定 loopback 与 `--no-open`，由 retained child/process-tree handle 证明 ownership；仅在当前 generation 的 owned `dsh web:` output 与 bounded TCP connect 同时成立后发布 endpoint。
- Managed stop 必须携带精确 `expectedGeneration`，并只作用于 retained process tree；不得依据 PID、port 或进程名推断 ownership。Environment 切换时 UI 必须立即丢弃旧 report。
- Windows Job Object tree cleanup 与 endpoint release 已有测试证据；Unix process-group 路径仍需真实平台验证。自动 restart/recovery 与 Safe Stop 已在 M2 实现（ADR-0013：bounded auto-restart、crash-loop fuse、recovery 报告）；daemon 与健康策略细粒度调参仍不属于当前范围。
