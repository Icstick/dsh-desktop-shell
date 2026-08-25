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

| Threat | Control | Verification |
|---|---|---|
| 误杀外部 DSH | explicit ownership + process identity | Attached negative tests |
| IPC spoof/replay | ACL/mode + ephemeral credential + generation | security contract tests |
| WebView privilege escalation | zero privileged capability | Tauri config audit/E2E |
| Plugin 越权 | DSH policy + Desktop grant/lease | adversarial adapter fixture |
| Browser account leakage | isolated profile + human-only secret surfaces | profile isolation tests |
| Agent arbitrary PTY | Surface/Automation split + approval | permission tests |
| PID reuse | handle/start-time/launch identity | chaos test |
| log secret leakage | structured allowlist + double redaction | golden corpus |
| path/symlink escape | canonicalization + scope | platform path tests |
| supply-chain code execution | no package management + allowlist policy | release audit |

## 未接受风险

- 给 DSH WebView 注入 `exec`、`readFile`、raw CDP 或 raw IPC。
- 固定全局 token、监听 0.0.0.0、仅凭 PID/port 判断 owner。
- Agent 默认继承 Human Terminal/Browser 权限。
- 诊断包包含 credential、完整环境变量或完整 session。
