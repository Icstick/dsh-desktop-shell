# Trust Boundaries

| Zone | 内容 | 信任级别 | 规则 |
|---|---|---|---|
| Z0 | Supervisor、Broker、native providers | 最高 | 结构化参数、least privilege、审计 |
| Z1 | Shell UI | 高但受限 | 窄 Tauri allowlist |
| Z2 | DSH Core + third-party plugins | 受控信任 | 仅经 authenticated adapter，非 sandbox |
| Z3 | Upstream DSH WebView | 不可信 Web | 无 privileged IPC |
| Z4 | Arbitrary Browser pages | 不可信 Web | 无 privileged IPC、独立 profile/policy |

## 关键攻击路径

- External port 被误认为 owned process。
- DSH plugin 读取 process-level token 并越权调用。
- Browser page 获取 Desktop IPC。
- Agent 绕过 DSH policy 直接连 raw IPC。
- stale PID 或 PID reuse 导致错误终止。
- 诊断包泄漏 path、token、Authorization 或 session。
- Browser profile 混用导致账户数据跨 Environment 泄漏。
- PTY automation 与 human session 混权。

缓解与验证见 [Threat Model](../security/THREAT_MODEL.md)。
