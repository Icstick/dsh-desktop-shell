---
id: DOC-RESEARCH-EXTERNAL-BASELINE
status: verified
verified_on: 2026-08-25
verified_at: 2026-08-25T12:19:55Z
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
| DeepSeek Harness | `master@b150a551b8d465e31e418e1b2eaf5e79bbb7d28e` | `@deepseek-ai/dsh@0.1.1-rc.2` (`latest`) | 仍为 Developer Preview，官方明确预期 breaking changes |
| dsh-std | `main@bb194ad53a72f4fa7da1286c88dcebb488b43eb9` | `@dsh-std/core`: `latest=0.1.0-rc1`, `rc=0.1.1-rc.1` | 代码与提案仍为 early drafts；版本选择必须显式绑定 dist-tag/版本 |
| Tauri | `tauri-docs/v2@1eb8f13f5961301ee46e8376e0b31c23fa927e81` | `tauri-v2.11.5` | capability 按 window/webview 作用，多 capability 权限合并；remote IPC 必须显式开启 |
| Desktop reference | `main@2a06026018fc498e4b2b52cd7e7bfdaae610ba10` | `v0.8.2` | 仅作非规范观察；MIT 顶层文本另附限制商业二次开发的条款 |
| Apache-2.0 | official license text | SHA-256 `CFC7749B...BC523D30` | 本仓库 LICENSE 与官方完整文本在忽略外围空白后匹配 |

## Compatibility 影响

1. DSH 初始 matrix 的 `latest` fixture 固定为 `0.1.1-rc.2`，`N-1` fixture 固定为 `0.1.1-rc.1`；`master@b150...` 仅作 advisory，不得自动宣称支持。
2. npm metadata 没有为上述 DSH/dsh-std artifact 提供 `gitHead`，因此不得推导 package tarball 与 repository HEAD 的一一对应关系。
3. dsh-std 的 `rc` dist-tag 比 `latest` 指向更新版本；M5 必须明确选择版本并运行 conformance，不能把“最大 semver”当成 registry policy。
4. Tauri 事实继续支持 DSH WebView/Browser WebView 无 privileged IPC、精确 label allowlist 和最终权限合并审查。
5. Desktop reference 的发行速度、renderer patch 或产品实现不构成本项目需求；附加条款进一步要求保持 clean-room、禁止复制。

## 架构结论

没有发现需要替代 ADR-0001、ADR-0003、ADR-0004 或 ADR-0006 的证据。User-owned External Core、optional dsh-std adapter、Tauri WebView 隔离和 clean-room boundary 继续有效；`implementation_authorized` 不因 baseline 刷新而改变。

## 下次刷新触发器

- `@deepseek-ai/dsh` 的 `latest` dist-tag、官方 Developer Preview 声明或 CLI/Web/discovery 行为变化。
- `@dsh-std/core` dist-tag、README status、connection/wire/conformance contract 变化。
- Tauri capability、remote access、multi-WebView 或 permission merge 语义变化。
- Desktop reference 的许可证/附加条款变化；即使变为宽松许可，也必须经过单独 copy review。
