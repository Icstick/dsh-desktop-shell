# Desktop Development Contract

前端只消费 versioned contracts 与 Tauri command facade，不管理 process、PTY 或 raw transport。DSH/Browser WebViews 无 privileged commands。UI 状态必须映射 backend canonical state，不能自行推断 ownership/health。

M1 前先验证三类 WebView capability、external navigation、reconnect overlay 和 first-run Setup。具体功能位于 features 子目录。
