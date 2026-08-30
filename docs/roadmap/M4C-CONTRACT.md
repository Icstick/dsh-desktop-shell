
M4-C 实现契约（2026-08-29 冻结，子代理按此并行实现）

## crates/browser-provider（纯逻辑 crate，不依赖 wry/tauri）
- BrowserProvider trait:
  - create(&mut self) -> Result<BrowserSession, BrowserError>  // profile 由宿主决定
  - navigate(&mut self, session_id, url) -> Result<BrowserSession, BrowserError>
  - snapshot_text(&mut self, session_id) -> Result<String, BrowserError>
  - close(&mut self, session_id) -> Result<BrowserSession, BrowserError>
- UrlPolicy::validate(url) -> Result<String, UrlError>  // 必须 http(s)、无 userinfo、len<=2048
- BrowserSession { session_id: String(brw-<ms>-<seq>), state: created|loading|ready|closed|error, current_url: Option<String>, created_at_unix_ms, last_activity_unix_ms, error: Option<String> }
- SessionRegistry：HashMap 管理 + 状态迁移（create→loading→ready；close→closed；错误→error）+ 未知 session -> BrowserError::NotFound
- BrowserEvent { session_id, kind: navigation_changed|load_failed|closed, occurred_at_unix_ms, url: Option<String> }
- 单测：UrlPolicy（scheme/userinfo/长度/格式）、SessionRegistry（状态机/opaque id/NotFound）、事件序列化

## apps/desktop/src-tauri/browser.rs（桥，参考 terminal.rs 与 dsh_surface.rs 模式）
- Tauri 命令（Shell-only ACL，28 -> 33）：
  - create_browser() -> BrowserReport
  - navigate_browser(request: BrowserNavigateRequest) -> BrowserReport
  - snapshot_browser(request: BrowserSnapshotRequest) -> BrowserReport { text: String }
  - close_browser(request: BrowserCloseRequest) -> BrowserReport
  - list_browsers() -> Vec<BrowserReport>
- WebView 创建：WebviewWindowBuilder label "browser"（独立 user-data-dir/profile：tauri 2 查 with_user_data_folder 或等价 API；没有则 WebContext 方案）；窗口初始化约 960x600，隐藏 shell 窗口之外独立窗口或内嵌（先独立窗口最小可行）
- 权限：webview2-com 0.38.2 直调 ICoreWebView2 挂 PermissionRequested deny + 导航拦截（复用 dsh_surface.rs 的 handler 模式）；initialization 前安装
- 事件：browser://event 推送 BrowserEvent（tauri emit，参照 terminal://output）
- BrowserReport 与 specs/browser/browser-report.schema.json 字段一致（camelCase）

## 前端（apps/desktop/features/browser-ui/）
- BrowserPanel.tsx：URL 输入 + Open/Close/Reload 按钮 + 状态显示（currentUrl/state）+ 事件订阅（browser://event）
- ActivityRail 加 browser 入口（参照 terminal/notifications/usage 模式）
- vitest：BrowserPanel 基本交互（mock desktop-api）
- 禁止 DOM injection；面板保持可键盘操作

## 门禁
- validate-specs 59/69 全过（不新增 schema）
- validate-acl 33 commands
- cargo test workspace（新增 browser-provider 单测 + desktop 新增测试）
- vitest 新增用例
