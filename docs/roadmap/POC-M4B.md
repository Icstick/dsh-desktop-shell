# M4-B Provider PoC — 架构与验证计划 (2026-08-29)

> 目标：在写产品代码前，用最小实验验证两个 browser provider candidate 的可行性，产出可复查的对比证据，由 maintainer 拍板默认 provider（ADR-0017 决策 4：默认 WebView2 embedded，CDP 是否升级由 PoC 决定）。

## 1. PoC 目标与成功标准

每个 candidate 的 PoC 必须回答：

| # | 问题 | 成功标准 |
|---|------|----------|
| P1 | 能创建独立浏览器会话（profile 隔离） | 独立 user-data-dir 生效；两个会话不共享 cookie/localStorage（用测试站点或注入验证） |
| P2 | 能导航到 HTTP(S) URL | navigate 后页面加载；事件/状态可观察 |
| P3 | 能取文本快照 | 拿到 document.title + body.innerText（长度/内容可断言） |
| P4 | 能截图（或明确降级） | 截图为非空图像；或记录不可行原因 |
| P5 | 能关闭并清理 | 会话关闭；进程树/资源清理（PoC B 尤其关键） |
| P6 | deny 语义有证据 | file:/download/popup/permission 至少一项有实测拒绝证据（PoC 级别，完整矩阵在 M4-C） |

## 2. Candidate A：嵌入式 WebView2（默认路径）

### 2.1 架构形态（侦察确认 2026-08-29）

- **形态**：独立 crate `scripts/pocs/m4b/poc-a/`（`[workspace]` 空表隔离，不加入根 workspace），依赖 **wry =0.55.1 + tao =0.35.3 + serde_json =1.0.151**——全部命中现有 target/debug 缓存（与 Cargo.lock 一致），**必须 `--target-dir D:\DSH_workspace\development\dsh-desktop-shell\target` 共享缓存**，否则全树重编 5-15 分钟。
- **Profile 隔离**：`WebContext::new(Some(path))`（直通 CreateCoreWebView2EnvironmentWithOptions），builder 用 `new_with_web_context`。
- **文本快照**：⚠️ `evaluate_script` 返回 `Result<()>` **不返回结果**；必须 `evaluate_script_with_callback(js, Fn(String)+Send+'static)`（回调收 ExecuteScript JSON 序列化值）。
- **截图**：⚠️ wry 0.55/0.56 均无 capture API。降级：主证据 = title+innerText 长度 + url()；stretch = `WebViewExtWindows::webview()` + webview2-com 0.38.2 的 CapturePreview。
- **拦截**：`with_navigation_handler`（true=放行）、`with_on_page_load_handler`（Finished 取快照）。
- **Permission**：0.55 无 permission API；0.56 才有 `with_permission_handler`（Windows 12 种）+ `with_profile_name` → **M4-C 升级 wry 0.56.1 的明确依据**（本 PoC 用 0.55.1 因缓存命中）。
- **风险 Top3**：① eval 走 callback（线程/生命周期）② 无 capture（截图降级）③ 忘 `--target-dir` 则全量编译。

### 2.2 隔离设计
- 独立 user-data-dir：Desktop 拥有，AppData 下 `browser-profiles/<session-id>`（与 ADR-0017 决策 3 一致）
- WebView label `browser`：与 `shell`/`dsh-surface` 分权（M4-C 落实，PoC 验证 API 可行）

### 2.3 验证脚本（已实现 `scripts/pocs/m4b/poc-a/`）
- 最小程序（wry + tao 独立 crate）：create（独立 WebContext data dir）→ navigate(https://example.com) → on_page_load Finished → evaluate_script_with_callback(title/innerText) → 自动退出
- 截图降级：文本快照为主证据（wry 0.55 无 capture API；stretch = webview2-com CapturePreview）
- 隔离验证（PoC 级）：WebContext::new(Some(dir)) 的目录被创建且不共享默认 WebView2 数据；cookie 级互斥验证 M4-C

## 3. Candidate B：外部 Edge + 受管 CDP

### 3.1 架构形态（侦察确认 2026-08-29）

- **形态**：`scripts/pocs/m4b/poc-b/pocb-edge-cdp.mjs`——**零依赖 Node 24 脚本**（原生 WebSocket/fetch），复用仓库 `smoke-native.mjs` 的 CDP 会话模式（id 匹配 Map + send/evaluate）。
- **启动**：`msedge.exe`（本机 v151，C:\Program Files (x86)\Microsoft\Edge\Application\）带 `--remote-debugging-port=0 --user-data-dir=<唯一临时目录> --no-first-run --no-default-browser-check --remote-allow-origins=* --disable-features=msEdgeStartupBoost`。
- **端口发现**：读 `<user-data-dir>/DevToolsActivePort`（第一行=端口，源码级确认；port=0 由 OS 分配无竞争）；stderr 仅旁证。
- **驱动**：`/json/list` 找 page target → WS 连接 → `Page.enable/navigate` → `readyState==='complete'` 轮询 → `Runtime.evaluate` 取 innerText → `Page.captureScreenshot` 存 PNG。
- **清理**：`taskkill /PID <pid> /T /F` 杀全树（renderer/gpu/crashpad）→ 删 user-data-dir（失败重试后忽略）。
- **关键坑**：user-data-dir 必须每次唯一（复用会转发已有实例、端口永不出现）；pipe 模式 Windows 无现成客户端不用；WS 响应乱序靠 id 匹配。
- **产品迁移**：CDP 语义与最终 Rust 版 1:1 对应（tokio-tungstenite 或 chromiumoxide 在 M4-C/M5 评估）。

### 3.2 隔离设计
- 唯一 user-data-dir（每会话临时目录）；同一 dir 复用会导致 Edge 进程复用——必须唯一
- CDP 连接只存在于 provider 内部（不暴露给 WebView/Agent，ADR-0017 决策 4）

### 3.3 验证脚本（已实现 `scripts/pocs/m4b/poc-b/pocb-edge-cdp.mjs`）
- spawn Edge(唯一 user-data-dir, --remote-debugging-port=0, 加固参数) → 轮询 DevToolsActivePort（≤10s）→ /json/list → WS 连接 → Page.navigate → readyState 轮询 → Runtime.evaluate(innerText) → Page.captureScreenshot → taskkill /T /F → 删 user-data-dir → 复查无残留 msedge

## 4. 验证矩阵（PoC 报告格式）

| 验证项 | A: WebView2 | B: Edge+CDP | 证据形式 |
|--------|------------|-------------|----------|
| create 会话 | | | 运行输出 |
| navigate https | | | 状态/事件 |
| 文本快照 | | | title+innerText 断言 |
| 截图 | | | 图像文件非空 |
| profile 隔离 | | | cookie/marker 不共享 |
| deny（file:/等） | | | 拒绝记录 |
| 清理 | | | 进程树/目录 |
| 代码量/复杂度 | | | LOC + 依赖 |
| 风险点 | | | 清单 |

## 5. 实验隔离

- PoC 代码放 `scripts/pocs/m4b/`（独立于产品代码；作为 WI-M4-BROWSER 证据提交）
- PoC A 若用独立 crate：不加入根 workspace（独立 Cargo.toml）或用 example
- PoC B 用 Node 原生实现（零新依赖）优先
- 不启动用户 DSH、不创建远程 WebView、不污染 AppData 产品数据

## 6. 时间盒与风险

- PoC A ≤ 2h；PoC B ≤ 2.5h；超时则记录"未验证项"而不是无限调
- 已知风险：wry 截图 API 可能缺失（降级为文本快照）；Edge 多实例/端口竞态；CDP 版本差异
- PoC 失败 ≠ candidate 淘汰：区分"环境问题"与"结构性不可行"

## 7. 输出物

1. 本目录 PoC 运行证据（输出/截图/脚本）
2. `POC-M4B-REPORT.md`：验证矩阵 + 结论 + 推荐
3. maintainer 拍板：默认 provider（A）维持 / 升级 B / 调整
