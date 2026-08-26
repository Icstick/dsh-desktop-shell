# WebView Isolation

## WebViews

- Shell UI：仅获结构化、窄范围 Tauri commands。
- DSH Web：无 privileged Tauri capability；只访问 DSH HTTP/WS。
- Browser page：无 privileged capability；独立 session/profile 与 navigation policy。

Tauri capability 合并会扩大权限，因此一个 WebView 不得同时落入宽泛 capability 集合。配置 review 必须列出每个 window/webview 的最终合并权限。

## Custom Command ACL

“未给 DSH/Browser WebView 配置 capability”本身不是充分条件。Tauri 2 中，仅通过 `invoke_handler` 注册、但没有进入应用 ACL manifest 的自定义 command 默认对所有 window/webview 可调用。所有应用自定义 command 必须同时：

1. 登记到 `tauri_build::AppManifest::commands`，不得存在 invoke-handler-only command。
2. 生成或声明最小 permission，并只分配给精确 Shell window/webview label。
3. 不配置 remote URL access；DSH 和 Browser label 不匹配任何 privileged command permission。
4. 在 review 中对照 command inventory、AppManifest、permission 与 capability 的最终合并结果；任一遗漏或多余映射均失败。

因此“零 privileged capability”表示 capability、permission 和 AppManifest command inventory 三层同时闭合，而不只是缺少某个 capability 文件。

Transport credential、Adapter token、Named Pipe/UDS name 与 raw loopback endpoint 不得注入任何 WebView。WebView 不能以“同机页面”为由绕过 Origin、navigation 或 capability grant。

## 禁止 API

`exec(command)`、`spawn(anything)`、`readFile(path)`、raw shell/fs scope、raw CDP socket、raw local transport handle。

## 导航

DSH Surface 仅允许配置的 loopback origin 与必要资源。External navigation 交给 Browser/OpenExternal policy。Browser 默认 HTTP(S)，`file:`、custom scheme、download、popup 和 permission prompt 单独处理。
