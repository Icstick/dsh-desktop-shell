# Browser Provider

**Module ID:** `MOD-BROWSER-PROVIDER`
**Target milestone:** M4
**Canonical status:** [MOD-BROWSER-PROVIDER](../../tracking/modules/MOD-BROWSER-PROVIDER.yaml)

## Purpose

纯逻辑的共享 Browser surface provider（ADR-0017）：browser session 生命周期状态机、
URL 导航策略与审计事件。**零外部依赖**（仅 std），不依赖 wry/tauri/WebView2/CDP——
真实 webview 由宿主（apps/desktop 桥接层）实现，本 crate 只负责状态与策略。

## Owns

- browser session 生命周期（`create → loading → ready`；`close → closed`；加载失败 → `error`）
- opaque session id（`brw-<unix_ms>-<seq>`，不泄露 profile 路径/进程细节，ADR-0017 决策 5）
- URL 导航策略（仅 http/https、无 userinfo、≤ 2048 字符，ADR-0017 决策 3）
- 审计事件记录（`navigation_changed` / `load_failed` / `closed`）

## Does not own

- WebView2 / CDP 渲染与进程树（宿主实现）
- profile 路径与浏览器数据（report/日志不得出现 profile 路径）
- Agent 授权（agent_automation / interact / take_over，M5 范围，ADR-0017 决策 2）

## Inputs

- host 驱动的导航、加载完成/失败回调、页面文本快照

## Outputs

- `BrowserSession` report（id/state/currentUrl/timestamps/error）
- `BrowserEvent` 审计事件（`browser://event` 推送素材）

## Dependencies

- 无（仅 std）

## Interfaces

- `IF-BROWSER`

## Usage

```rust
use dsh_browser_provider::{BrowserProvider, BrowserError, SessionRegistry};

fn main() -> Result<(), BrowserError> {
    let mut provider = SessionRegistry::new();

    // 1. create -> state=created，id 形如 brw-<unix_ms>-<seq>
    let session = provider.create()?;

    // 2. navigate：URL 由 UrlPolicy 校验（http/https、无 userinfo、≤2048）
    //    -> state=loading，推 navigation_changed 事件
    let session = provider.navigate(&session.session_id, "https://example.com")?;

    // 3. 宿主在 WebView2 NavigationCompleted 时调用 mark_ready -> state=ready
    let session = provider.mark_ready(&session.session_id)?;

    // 4. 宿主写入页面文本快照，surface 读取
    provider.set_snapshot(&session.session_id, "page text")?;
    let text = provider.snapshot_text(&session.session_id)?;

    // 5. close -> state=closed，推 closed 事件；重复 close 返回 BrowserError::Closed
    let session = provider.close(&session.session_id)?;

    // 6. 审计事件（drain 会清空历史）
    let events = provider.drain_events();
    println!("events: {events:?}, last page: {text}");
    Ok(())
}
```

## 状态机

```text
create ──> created ──navigate──> loading ──mark_ready──> ready
                                 │
                                 └──mark_load_failed──> error ──navigate──> loading（可恢复）
close ──> closed（终态；任何操作返回 BrowserError::Closed）
```

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
