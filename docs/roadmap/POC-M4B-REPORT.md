# POC-M4B-REPORT — Provider Candidate 对比报告 (2026-08-29)

> 依据 POC-M4B.md 验证矩阵，由两个独立 PoC 实验产出。结论供 maintainer 拍板默认 provider（ADR-0017 决策 4）。

## 验证矩阵

| 验证项 | A: WebView2 (wry 0.55.1) | B: Edge + CDP | 证据位置 |
|--------|--------------------------|---------------|----------|
| P1 create + profile 隔离 | ✅ 独立 WebContext data dir（198 项 EBWebView 结构） | ✅ 唯一 user-data-dir（DevToolsActivePort 就位） | evidence/poc-a/run-output.txt |
| P2 navigate https | ✅ [nav] + [load finished] | ✅ readyState complete | 同上 |
| P3 文本快照 | ✅ title=Example Domain len=129（双重 JSON 编码已修） | ✅ Runtime.evaluate innerText=129 | 同上 |
| P4 截图 | ⚠️ 降级：wry 无 capture API（M4-C 需 webview2-com CapturePreview 或升 wry） | ✅ Page.captureScreenshot PNG（25998 B） | evidence/poc-b/pocb-screenshot.png |
| P5 关闭/清理 | ✅ 自动退出 exit 0（profile 保留可复查） | ✅ taskkill /T /F + profile 删除 + 全机无残留 msedge | evidence/poc-b/README.md |
| P6 deny 语义 | ◐ nav handler 拦截点就位（放行逻辑演示） | ◐ 未专项验证（M4-C 完整矩阵） | — |
| 依赖/复杂度 | wry+tao（命中缓存，零新依赖）；~150 行 Rust | 零依赖 Node 24（复用 smoke-native 模式）；~90 行 JS | — |
| 编译/运行成本 | 首编 ~10min（缓存命中 <2min），增量 1-2s | 无需编译；Edge 启动 + 全流程 1.5s | — |
| 弹窗行为 | WebView2 窗口（预期 human_surface 行为） | Edge 窗口（预期） | — |

## 结论与推荐

- **Candidate A（WebView2 embedded）维持默认**：profile 隔离 API 原生（WebContext）、拦截点齐全（navigation/load handler）、无进程管理负担；截图需在 M4-C 补 CapturePreview（wry 无 capture API）或评估升级 wry 0.56.1（附带 permission handler）。
- **Candidate B（Edge + CDP）验证可行**：零依赖驱动全流程跑通（启动/端口发现/导航/文本/截图/清理），适合作为 human-takeover 高级模式或 M6 的受管浏览器路径；产品化需 Rust CDP 栈（tokio-tungstenite/chromiumoxide）与进程树管理（复用 M2 supervisor 模式），成本高于 A。
- **维护选项**：M4-C 实现 A；B 保留 PoC 证据，是否升级为正式 provider 由 maintainer 决定（建议：暂不升级，M6 daemon 阶段 revisit）。

## 风险与 M4-C 注意事项

1. wry 0.55 无 permission handler → M4-C 升级 wry 0.56.1（API 差异：with_permission_handler、with_profile_name）或维持 0.55 + webview2-com 直接调 ICoreWebView2。
2. WebView2 ExecuteScript 回调双重 JSON 编码（PoC A 已踩）→ 快照解析需两级反序列化。
3. Edge CDP 的 user-data-dir 复用陷阱（转发旧实例）→ 产品化时必须唯一目录。
4. PoC 运行会弹窗（WebView2 窗口 / Edge 窗口）——真实 human_surface 的预期行为，与 headless 自动化无关。

## 决策请求（maintainer）

- [ ] 默认 provider = Candidate A（WebView2 embedded）？
- [ ] Candidate B 升级/搁置？（建议搁置，M6 revisit）
- [ ] M4-C 的 wry 版本策略：升 0.56.1（有 permission handler）vs 0.55.1 + webview2-com 直调？
