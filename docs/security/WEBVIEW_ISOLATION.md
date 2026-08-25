# WebView Isolation

## WebViews

- Shell UI：仅获结构化、窄范围 Tauri commands。
- DSH Web：无 privileged Tauri capability；只访问 DSH HTTP/WS。
- Browser page：无 privileged capability；独立 session/profile 与 navigation policy。

Tauri capability 合并会扩大权限，因此一个 WebView 不得同时落入宽泛 capability 集合。配置 review 必须列出每个 window/webview 的最终合并权限。

## 禁止 API

`exec(command)`、`spawn(anything)`、`readFile(path)`、raw shell/fs scope、raw CDP socket、raw local transport handle。

## 导航

DSH Surface 仅允许配置的 loopback origin 与必要资源。External navigation 交给 Browser/OpenExternal policy。Browser 默认 HTTP(S)，`file:`、custom scheme、download、popup 和 permission prompt 单独处理。
