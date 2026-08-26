# Permission Matrix

| Capability | Human UI | Agent | 默认 |
|---|---|---|---|
| Runtime status | allow | observe through DSH | allow |
| Managed start/stop | allow | policy-gated | human only |
| Attached stop/restart | deny | deny | deny |
| Terminal view/write | allow | separate tool grant | agent deny |
| Browser snapshot | allow | scoped grant | agent optional |
| Browser navigation/mutation | allow | domain/action grant | agent deny |
| Password/autofill | human only | deny | deny |
| File dialog | user gesture | only selected result | gesture required |
| Clipboard read | explicit | deny unless scoped | deny |
| Notification | allow | semantic event only | redact content |
| Arbitrary PID kill/raw IPC | deny | deny | deny |

Grant 必须绑定 participant、activation、resource、scope、generation 和可选 expiry。

Schema 合法、transport 已认证或 capability 已协商都不等于授权。Broker 在每次 mutation dispatch 前重新验证 Desktop grant 与 lease；disconnect、unload、human takeover、expiry 和 generation change 都必须撤销相关 authority。

Tauri 应用命令还必须进入 `tauri_build::AppManifest::commands`，再由最小 permission 和精确 capability label 授权。仅在 `invoke_handler` 注册的自定义 command 禁止合入，因为这类 command 默认不受预期 capability ACL 约束。DSH/Browser WebView 必须同时满足：不匹配 privileged capability、不匹配 command permission、没有 remote URL access。
