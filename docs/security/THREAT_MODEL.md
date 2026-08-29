---
id: DOC-THREAT-MODEL
status: review
owner_role: security-owner
---

# Threat Model

## 资产

- 用户 DSH/Node 进程、文件系统和 shell 权限。
- DSH_HOME、Profile、插件、凭据和 session。
- Browser 登录态、下载、剪贴板和 autofill。
- PTY session、cwd、输入输出和子进程。
- Supervisor lifecycle、transport credential 和 process identity。
- 诊断、日志、release signing 与更新链。

## 信任边界

Z0 Supervisor/Broker/native providers；Z1 Shell UI；Z2 DSH Core/plugins；Z3 upstream DSH WebView；Z4 arbitrary browser pages。Z2 不是 sandbox。

## 威胁与控制

| ID | Threat | Control | Owner | Verification |
|---|---|---|---|---|
| TM-OWN-001 | 误杀外部 DSH / PID reuse | explicit ownership + retained process-tree handle + instance/generation；stop 不从 PID/port 重建 authority | MOD-SUPERVISOR、MOD-PROCESS-MANAGER | AC-OWN-001、AC-RUN-005、chaos |
| TM-IPC-001 | IPC spoof/replay/stale generation | ACL/mode + ephemeral credential + generation | MOD-LOCAL-TRANSPORT | AC-IPC-001 |
| TM-IPC-002 | malformed/oversized/slow client 导致资源耗尽 | framing limits + deadline + bounded concurrency + cancellation | MOD-LOCAL-TRANSPORT | AC-IPC-002 |
| TM-WEB-001 | DSH/Browser WebView privilege escalation | zero privileged capability + exact window/webview allowlist | MOD-HARNESS-SURFACE、MOD-SHELL-UI | AC-WEB-001、AC-BRW-001 |
| TM-WEB-002 | hostile page 通过 DNS rebinding/loopback CSRF 触达 fallback | credential 不进入 Web、Origin policy、127.0.0.1 only | MOD-LOCAL-TRANSPORT、MOD-HARNESS-SURFACE | AC-WEB-002 |
| TM-WEB-003 | invoke-handler-only custom command 绕过预期 capability ACL | complete AppManifest command inventory + minimal permission + exact label + no remote URL | MOD-SHELL-UI、MOD-HARNESS-SURFACE | AC-WEB-003 |
| TM-WEB-004 | remote DSH page 获得 camera/microphone/geolocation/notification 等 native permission | Windows WebView2 load 前安装全拒绝 handler；无可复查 deny hook 的平台 fail closed，不创建 WebView | MOD-HARNESS-SURFACE | AC-WEB-006、AC-WEB-007 |
| TM-PLG-001 | Plugin/Agent 越权 | DSH policy + Desktop grant/lease/scope/generation | MOD-CAPABILITY-CONTRACTS、MOD-SUPERVISOR | adversarial adapter fixture、AC-LEASE-001 |
| TM-CMD-001 | Harness path/args/cwd 触发 shell injection | structured executable + argv；禁止 shell parsing；secret 不进入 argv | MOD-ENVIRONMENT-SETTINGS、MOD-PROCESS-MANAGER | AC-CMD-001 |
| TM-PATH-001 | path/symlink/TOCTOU escape | canonicalization、scope、open handle/final identity recheck | MOD-ENVIRONMENT-SETTINGS、native providers | AC-PATH-001 |
| TM-BRW-001 | Browser profile/account/download 泄漏 | isolated profile + human-only secrets + scoped download | MOD-BROWSER-PROVIDER | AC-BRW-002、profile isolation |
| TM-PTY-001 | Agent arbitrary PTY / Human session takeover | Surface/Automation split + opaque resource + grant | MOD-TERMINAL-PROVIDER | AC-PTY-001、permission tests |
| TM-LOG-001 | log/diagnostic 泄漏 secret、path、argv 或 content | structured allowlist + double redaction | MOD-RUNTIME-DIAGNOSTICS、MOD-USAGE-COLLECTOR | AC-LOG-001 |
| TM-SUP-001 | dependency/build/update 执行未审计代码 | no DSH package management + pinned dependency/source review | release/compliance owner | release audit |

## Review Rule

任何列为 Owner 的模块都必须在实现前保持 `security_review: required`，直到具备实现级证据后才可改为 `passed`。M0 对威胁模型的接受不代表未来实现已经通过安全审查。

## 未接受风险

- 给 DSH WebView 注入 `exec`、`readFile`、raw CDP 或 raw IPC。
- 固定全局 token、监听 0.0.0.0、仅凭 PID/port 判断 owner。
- Agent 默认继承 Human Terminal/Browser 权限。
- 诊断包包含 credential、完整环境变量或完整 session。
