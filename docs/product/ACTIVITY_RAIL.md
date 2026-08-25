# Activity Rail

Activity Rail 是 Desktop-owned 的窄导航，不重做 DSH 内部 Sidebar。

| Surface | Owner | P0/P1/P2 | 说明 |
|---|---|---:|---|
| DSH | Upstream DSH + Shell container | P0 | 原版 DSH Web UI |
| Browser | Desktop provider | P1 | Human Surface 与 Agent Automation 分权 |
| Terminal | Desktop Supervisor | P1 | Persistent PTY |
| Usage | DSH collector + Desktop view | P1 | 数据源和估算方式可见 |
| Timer | Desktop | P1 | countdown/pomodoro，不是 Agent Scheduler |
| Runtime | Desktop | P0 | Environment、health、logs、restart |
| Settings | Desktop | P0 | 只保存引用与策略 |

## UI 边界

- Shell UI 可以调用窄 Tauri command allowlist。
- DSH WebView 与 Browser WebView不能调用 native command。
- Desktop 不向 upstream 页面注入导航、CSS、React component 或全局对象。
- External URL 必须进入合适的 Browser/OpenExternal policy，不在 DSH Surface 中任意导航。
