# Desktop Development Contract

前端只消费 versioned contracts 与 Tauri command facade，不管理 process、PTY 或 raw transport。DSH/Browser WebViews 无 privileged commands。所有 custom Tauri commands 必须登记到 `tauri_build::AppManifest::commands`，再通过最小 permission 和精确 Shell label 授权；禁止 invoke-handler-only command。UI 状态必须映射 backend canonical state，不能自行推断 ownership/health。

M1 前先验证三类 WebView capability、AppManifest command inventory、最终权限合并、external navigation、reconnect overlay 和 first-run Setup。具体功能位于 features 子目录。
