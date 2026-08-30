# M4-D Live Desktop QA — Shared Browser (2026-08-30)

## 方法

- 临时给 shell window 配置 `additionalBrowserArgs="--remote-debugging-port=9333"`（QA 专用，验证后已还原；生产无调试端口）。
- `tauri build --debug --no-bundle`（嵌入前端资源）后启动真实 app。
- `scripts/qa/live-browser-qa.mjs`：零依赖 Node 24，CDP 驱动 shell UI —— rail 点击 Browser → URL 输入 → Open → 面板状态读取 → API snapshot/非法导航/close → profile 目录与窗口验证。

## 结果（全部通过）

| 验证项 | 结果 | 证据 |
|--------|------|------|
| Browser rail 入口可点击 | PASS | [rail-click] CLICKED |
| BrowserPanel 挂载（Open/Reload/Close 按钮） | PASS | buttons 列表含 Open/Reload/Close |
| URL 输入 + Open 创建会话 | PASS | 面板状态：brw-1788055973732-1 |
| 真实导航 example.com | PASS | CURRENT URL https://example.com / STATE ready |
| snapshot_browser API | PASS | SNAP_OK 完整 report（state ready） |
| 非法导航拒绝（file:///） | PASS | BAD_NAV_REJECTED |
| profile 隔离 | PASS | browser-profiles/<session-id> 每会话独立目录 |
| browser 窗口创建 | PASS | 窗口标题 "DSH Browser" |
| close_browser | PASS | CLOSED |

## 运行时证据覆盖

- AC-BRW-003 导航策略：file:// 实测拒绝 ✓（on_navigation + 命令层双保险）
- AC-BRW-004 profile 隔离：独立 user-data-dir 目录实测创建 ✓
- AC-BRW-001 三层闭合：live 运行下 browser 窗口/标签不匹配 shell capability（单测 + ACL 门禁 + live 窗口创建无特权报错）✓

## 说明

- 首次 QA 运行在 snapshot 前因脚本正则转义问题中断（app 残留一个 profile 目录 brw-1788055950653-1），修复后重跑全部通过；残留目录为预期产物（profile 按 session 保留）。
- 完整输出：live-qa-output.txt
