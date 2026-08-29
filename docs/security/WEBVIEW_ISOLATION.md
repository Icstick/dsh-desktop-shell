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

Desktop transport credential、Adapter token、Named Pipe/UDS name 与 raw local-transport handle 不得注入任何 WebView。ADR-0012 只允许 Supervisor 把 owned DSH process 自己生成的 single-generation Web bootstrap URL用于 `dsh-surface` 首次 native navigation；该 URL不得经过 Shell IPC、DOM injection、page eval、日志或 tracking。WebView 不能以“同机页面”为由绕过 Origin、navigation 或 capability grant。

## 禁止 API

`exec(command)`、`spawn(anything)`、`readFile(path)`、raw shell/fs scope、raw CDP socket、raw local transport handle。

## 导航

DSH Surface 仅允许配置的 loopback origin 与必要资源。External navigation 交给 Browser/OpenExternal policy。Browser 默认 HTTP(S)，`file:`、custom scheme、download、popup 和 permission prompt 单独处理。

M1 的精确规则由 `IF-DSH-SURFACE-POLICY` 冻结：allowed origin 只能从 persisted fixed `http://127.0.0.1:<port>` Environment 派生；same-origin main-frame navigation 可留在 Surface。另一 loopback origin、credentialed URL、non-HTTP scheme、popup、download 和 permission request fail closed。External HTTP(S) 只有在存在明确 user gesture 时才能返回待外层确认的 delegate decision，任何 decision 都不得自动打开 URL 或回显 path/query/fragment/credential。

## M1 Native Surface platform gate

`ADR-0011` 将 native child WebView 收紧为 Windows-only foothold：mount URL 只能来自 Supervisor 的 verified current-generation Managed binding，caller 不得提供 endpoint/origin/label。Windows 必须在 remote document load 前通过 WebView2 安装全 permission deny handler；macOS、Linux 和其他平台在具备等价可复查证据前保持 unmounted 并返回 `unsupported_platform`。

Native navigation hook 不提供可信 user-gesture provenance，因此本切片比 policy evaluator 更严格：只允许 verified exact-origin，所有 cross-origin、popup、download 与 permission 都拒绝。不得通过 initialization script、page eval、iframe 或自动 external opener 弥补该限制。

当前 capability 使用 `webviews: ["shell"]` 精确匹配发起 lifecycle command 的 Shell WebView，而不是按 parent window 广泛匹配；child label `dsh-surface` 不匹配任何 custom permission。ACL validator 同时枚举 AppManifest、invoke handler、permission 文件与 capability，要求十六个 command 完全一致、无 remote URL access、无 caller-controlled endpoint/origin/URL/label 字段。

Windows child 先以 `about:blank` 创建，再安装 WebView2 `PermissionRequested` deny、password autosave/general autofill deny 与 load completion handler，之后才导航至 backend-derived URL。`on_navigation` 只允许 bootstrap 和 exact `http://127.0.0.1:<verified-port>` origin；因此 ADR-0012 的 token root exchange 和 clean-root redirect可通过，而另一 origin 仍拒绝。new window 与 download 由 native callback 直接拒绝。上述源码与 automated tests 只构成实现审查证据；真实 DSH/WebView2 permission、redirect 与 failure smoke 未完成前，`RISK-WEBVIEW-PERMISSION-HOOK` 保持 mitigating。
