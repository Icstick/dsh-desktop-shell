# M4-B PoC A 验证证据：嵌入式 WebView2（wry 0.55.1）

- 验证日期：2026-02-21（session 内实跑）
- 验证人：dsh-desktop-shell 实现验证子代理
- 对应工作项：WI-M4-BROWSER（M4B PoC A：嵌入式 WebView2 surface 可行性）
- 代码位置：`scripts/pocs/m4b/poc-a/`（Cargo.toml + src/main.rs）
- 完整运行输出：`run-output.txt`；编译输出：`build-output.txt`

## 1. 运行环境

| 项 | 值 |
|---|---|
| OS | Windows（本机 GUI 桌面，运行时弹出 WebView2 窗口属正常现象） |
| rustc / cargo | 1.98.0（C:\Users\Administrator\.cargo\bin，需手动加 PATH） |
| wry | =0.55.1（源码核对：registry\src\...\wry-0.55.1） |
| tao | =0.35.3 |
| serde_json | =1.0.151 |
| target 缓存 | 共享根仓库 target：`--target-dir D:\DSH_workspace\development\dsh-desktop-shell\target`（避免全树重编） |
| WebView2 Runtime | 系统安装的 Evergreen WebView2（wry 经 webview2-com 绑定加载） |

## 2. 命令

```powershell
$env:PATH = 'C:\Users\Administrator\.cargo\bin;' + $env:PATH
cd scripts\pocs\m4b\poc-a
cargo build --target-dir D:\DSH_workspace\development\dsh-desktop-shell\target
cargo run   --target-dir D:\DSH_workspace\development\dsh-desktop-shell\target
```

程序行为：create（WebContext + tao 窗口）→ navigate https://example.com → PageLoadEvent::Finished → evaluate_script_with_callback 取 title + innerText 长度 → 打印后自动退出（exit 0）；30 秒无 Finished 则超时退出（exit 2）。

## 3. 验证结果

| 验证点 | 判定 | 证据（run-output.txt） |
|---|---|---|
| P1 profile 隔离（user-data-dir 生效） | ✅ PASS | `[profile] C:\Users\Administrator\AppData\Local\Temp\m4b-poc-a-profile`；目录已创建，含标准 WebView2 结构：EBWebView/{Default, Crashpad, ShaderCache, GPUPersistentCache, WidevineCdm, Local State, Last Version, ...} 共 198 项——独立 user data folder 直通 CreateCoreWebView2EnvironmentWithOptions 的 userDataFolder |
| P2 navigate（https 导航记录） | ✅ PASS | `[nav] https://example.com/`（navigation handler 记录，放行）→ `[load finished] https://example.com/`（PageLoadEvent::Finished） |
| P3 文本快照（title + innerText 长度） | ✅ PASS | `[snapshot] title=Example Domain innerText_len=129`；raw 回调确认页面文本为 "Example Domain … Learn more"（129 字符） |
| P6 自动退出（快照后） | ✅ PASS | `[exit] done. profile kept at %TEMP%\m4b-poc-a-profile`，进程 exit code 0（cargo run RUN_EXIT=0），无挂起 |
| 截图能力 | ⏭️ 降级 | wry 0.55/0.56 无 capture API（源码确认），按计划降级为 title + innerText + url() 文本快照 |

附加观察：`libpng warning: iCCP: cHRM chunk does not match sRGB` 来自 WebView2 内部图标资源加载，非本程序错误，不影响验证。

## 4. 遇到的问题与修复

### 4.1 骨架文件位置错误（首次编译失败，BUILD_EXIT=101）
- 现象：`error: no targets specified in the manifest`
- 原因：`main.rs` 放在 poc-a 根目录，Cargo 要求 `src/main.rs`
- 修复：`Move-Item main.rs src\main.rs`（纯骨架修正，未改代码）

### 4.2 wry 0.55.1 API：with_web_context → new_with_web_context
- 现象：骨架使用 `WebViewBuilder::new().with_web_context(&mut ctx)`
- 源码核对（wry-0.55.1/src/lib.rs）：0.55.1 **没有** `with_web_context` builder 方法；正确 API 是构造器 `WebViewBuilder::new_with_web_context(&'a mut WebContext)`（line 882）
- 修复：本次更新后的骨架已直接使用 `new_with_web_context`（未触发该错误，但核对确认 API 正确）
- 另确认：`with_navigation_handler(Fn(String)->bool)`、`with_on_page_load_handler(Fn(PageLoadEvent, String))`、`evaluate_script_with_callback(&self, js, Fn(String)+Send+'static)` 签名均与骨架一致

### 4.3 tao 0.35.3 API：EventLoop::with_user_event 已移除
- 现象：`error[E0599]: no associated function or constant named 'with_user_event' found for struct 'EventLoop<UserEvent>'`
- 源码核对（tao-0.35.3/src/event_loop.rs）：泛型用户事件必须走 `EventLoopBuilder::<T>::with_user_event().build()`；`EventLoop` 本身只有 `new() -> EventLoop<()>`
- 修复：`EventLoopBuilder::<UserEvent>::with_user_event().build()`（import 增加 EventLoopBuilder）

### 4.4 错误类型不匹配
- 现象：`error[E0277]`——`WindowBuilder::build` 返回 `tao::error::OsError`，`?` 无法转为 `wry::Error`（wry::Error 无 From<OsError>）
- 修复：`fn main() -> Result<(), Box<dyn std::error::Error>>`（两个 `?` 均兼容）

### 4.5 字符串转义错误
- 现象：`error: unknown character escape: 'm'`——`"%TEMP%\m4b-poc-a-profile"` 中 `\m` 非法转义
- 修复：改 raw string `r"[exit] done. profile kept at %TEMP%\m4b-poc-a-profile"`

### 4.6 EventLoopProxy 移动语义
- 现象：`error[E0382]` + `error[E0507]`——`proxy` 被 `with_on_page_load_handler` 闭包和 `event_loop.run` 闭包（及内层回调闭包）多次消费
- 修复：为每个闭包 clone 一份 proxy（`proxy_page` / `proxy_exit`）

### 4.7 快照 JSON 双重编码（验证逻辑 bug，运行期发现）
- 现象：`[snapshot raw]` 正确（"`{\"t\":\"Example Domain\",...}`"），但解析后 `title=? innerText_len=-1`
- 原因：WebView2 ExecuteScript 回调收到的是 JSON 序列化字符串（**外层带引号的字符串**，内层才是对象），需先解外层 String 再解析内层对象；骨架只解析了一层
- 修复：两级 serde_json 解析（先 `outer.as_str()` 再 `from_str::<Value>`）
- 修复后：`[snapshot] title=Example Domain innerText_len=129` ✅

## 5. 结论

PoC A 全部验证点通过：WebView2 嵌入式 surface 在本机可用，独立 profile 隔离生效（EBWebView 完整目录树），https 导航 + 加载完成事件 + JS 文本快照 + 自动退出闭环成立。M4-B 人类侧 surface 可行性确认。
