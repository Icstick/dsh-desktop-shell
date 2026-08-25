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
