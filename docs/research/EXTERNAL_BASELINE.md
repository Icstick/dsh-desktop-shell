---
id: DOC-RESEARCH-EXTERNAL-BASELINE
status: verified
verified_on: 2026-08-28
verified_at: 2026-08-28T12:57:55Z
---

# External Baseline

本文件是外部版本与事实的可读快照；机器可读真源位于 [SOURCE_REGISTER.yaml](../compliance/SOURCE_REGISTER.yaml)。默认分支名称不是版本标识，所有结论绑定到精确 commit、release 或 registry artifact。npm 的 `dist-tag` 单独记录，不能用语义版本排序替代。

## 取证范围与方法

- GitHub REST API：默认分支、HEAD commit、commit 时间、release 与许可证元数据。
- immutable GitHub permalink：绑定 README、Tauri capability 文档和附加许可条款的精确 blob。
- npm registry：绑定 package version、发布时间、SHA-1 `shasum` 与 SHA-512 `integrity`。
- 只读取公开元数据和规范事实；未复制任何第三方代码、资产或产品文案。

## 已冻结坐标

| Source | Repository coordinate | Distribution coordinate | Verified conclusion |
|---|---|---|---|
| DeepSeek Harness | `master@cd5ef8148158c3a752a658978873241fdf8e2bbc` | `@deepseek-ai/dsh@0.1.1-rc.2` (`latest`) | 仍为 Developer Preview；advisory source 已采用 authenticated Web bootstrap，registry latest 仍是独立 fixture |
| dsh-std | `main@bb194ad53a72f4fa7da1286c88dcebb488b43eb9` | `@dsh-std/core`: `latest=0.1.0-rc1`, `rc=0.1.1-rc.1` | 代码与提案仍为 early drafts；版本选择必须显式绑定 dist-tag/版本 |
| Tauri | `tauri-docs/v2@1eb8f13f5961301ee46e8376e0b31c23fa927e81` | `tauri-v2.11.5` | capability 按 window/webview 作用，多 capability 权限合并；remote IPC 必须显式开启 |
| Desktop reference | `main@2a06026018fc498e4b2b52cd7e7bfdaae610ba10` | `v0.8.2` | 仅作非规范观察；MIT 顶层文本另附限制商业二次开发的条款 |
| Apache-2.0 | official license text | SHA-256 `CFC7749B...BC523D30` | 本仓库 LICENSE 与官方完整文本在忽略外围空白后匹配 |

## Compatibility 影响

1. DSH 初始 matrix 的 `latest` fixture 固定为 `0.1.1-rc.2`，`N-1` fixture 固定为 `0.1.1-rc.1`；`master@cd5...` 仅作 advisory，不得自动宣称支持或推导未发布 `0.1.2-alpha.1` artifact。
2. npm metadata 没有为上述 DSH/dsh-std artifact 提供 `gitHead`，因此不得推导 package tarball 与 repository HEAD 的一一对应关系。
3. dsh-std 的 `rc` dist-tag 比 `latest` 指向更新版本；M5 必须明确选择版本并运行 conformance，不能把“最大 semver”当成 registry policy。
4. Tauri 事实继续支持 DSH WebView/Browser WebView 无 privileged IPC、精确 label allowlist 和最终权限合并审查。
5. Desktop reference 的发行速度、renderer patch 或产品实现不构成本项目需求；附加条款进一步要求保持 clean-room、禁止复制。

## Current DSH Launch Contract

在冻结的 DSH advisory revision 中，Web profile 明确接受 `--host`、`--port`、`--trusted-host` 与 `--no-open`；`--port 0` 表示由 OS 分配空闲端口，`--host 0.0.0.0` 被上游以安全原因为由拒绝。本地启动默认会打开系统浏览器，因此 Desktop Managed launch 必须显式提供 `--no-open`。当前 readiness URL 是 exact loopback root 加 43-character base64url process token；根请求交换 authority-bound signed cookie 后重定向到 clean root。来源见机器可读 Source Register 中的 `launch_contract`、`launch_implementation` 与 `browser_auth_contract` evidence。

从 source checkout 启动时，上游要求先构建产物；本项目只接受用户已准备好的 launch recipe，不运行安装或构建。npm artifact 没有 `gitHead`，所以 published-package 与 source-checkout fixture 继续分开验证。

## 2026-08-28 Authenticated bootstrap scoped verification

- GitHub 官方 `master@cd5ef8148158c3a752a658978873241fdf8e2bbc` 仍声明 Developer Preview，并要求 source checkout 先由用户 build；Windows 实测预构建 `apps/cli/lib/bin.js` 需要 Node 启动。
- 官方 Web App 文档说明每个 process 生成 fresh token；`dsh web:` 打印 token root，浏览器取得 signed cookie 后重定向到 clean root。官方 Browser Auth 文档进一步限定 token 只在 `GET /` 交换，缺失/错误/authority mismatch 在 RPC 前返回 401。
- 2026-08-28 npm registry 的 `latest`/`next` 仍指向 `0.1.1-rc.2`，不存在 `0.1.2-alpha.1` 发布元数据；因此 current source behavior只作为 advisory coordinate，不能冒充 registry release fixture。
- 真实启动的完整 bootstrap URL含 credential，未写入 repository、tracking 或 review；证据只记录 sanitized shape 和 fail-closed outcome。

结论冻结于 `ADR-0012`：允许 current-generation backend-only DSH bootstrap URL直接进入 unprivileged native Surface，禁止进入 Shell IPC、公开 report、日志、诊断或 tracking；source checkout 使用无 shell的 explicit Node recipe。

## 2026-08-28 Native WebView scoped verification

本次只刷新 M1 native Surface 直接依赖的 API/security seam，不重写 DSH/dsh-std 坐标：

- 仓库锁定的 `tauri=2.11.5` 中，child `WebviewBuilder` 属于 `unstable` feature；官方 API 提供 `on_navigation`、`on_new_window`、`on_download` 与 `on_page_load`，navigation callback 返回 `false` 可取消导航。[Tauri 2.11.5 WebviewBuilder](https://docs.rs/tauri/2.11.5/tauri/webview/struct.WebviewBuilder.html)
- 锁定的 `wry=0.55.1` Windows backend 只在 clipboard feature 开启时安装特定 clipboard allow handler，没有统一 builder-level permission deny hook；Tauri `with_webview` 仍允许 host 取得 native handle并使用锁定的 `webview2-com=0.38.2` 安装全拒绝 handler。[Wry v0.55.1 WebView2 source](https://github.com/tauri-apps/wry/blob/wry-v0.55.1/src/webview2/mod.rs)
- 同一 Wry tag 的 WKWebView UI delegate 在没有 permission handler 时对 media capture 采用 grant；锁定的 Tauri builder 没有足以证明已安装等价 handler 的公开方法。[Wry v0.55.1 WKWebView source](https://github.com/tauri-apps/wry/blob/wry-v0.55.1/src/wkwebview/class/wry_web_view_ui_delegate.rs)
- Tauri capability 仍按 window/webview 匹配，多个 capability 权限合并，remote access 必须显式配置；因此 `dsh-surface` 继续不匹配任何 privileged capability。[Tauri Capabilities](https://v2.tauri.app/security/capabilities/)

结论冻结于 `ADR-0011`：M1 native Surface 只在 Windows 形成实现 foothold，并安装 WebView2 全 permission deny；macOS/Linux/other fail closed。该结论不推导未来版本能力，Tauri/Wry 升级必须重新核验。

## 架构结论

没有发现需要替代 ADR-0001、ADR-0003、ADR-0004 或 ADR-0006 的证据。User-owned External Core、optional dsh-std adapter、Tauri WebView 隔离和 clean-room boundary 继续有效；`implementation_authorized` 不因 baseline 刷新而改变。

## 下次刷新触发器

- `@deepseek-ai/dsh` 的 `latest` dist-tag、官方 Developer Preview 声明或 CLI/Web/discovery 行为变化。
- `@dsh-std/core` dist-tag、README status、connection/wire/conformance contract 变化。
- Tauri capability、remote access、multi-WebView 或 permission merge 语义变化。
- Desktop reference 的许可证/附加条款变化；即使变为宽松许可，也必须经过单独 copy review。
