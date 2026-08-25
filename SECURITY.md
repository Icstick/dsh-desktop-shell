# Security Policy

## 安全目标

- Attached DSH 永不因端口探测而被误杀。
- DSH WebView 与任意网页不能获得 privileged native API。
- Agent native action 必须经过 DSH tool/policy 与 Desktop capability grant。
- IPC 默认本地、认证、最小权限，并能拒绝 replay/stale generation。
- 凭据、Authorization、token、用户路径和 session 内容不得进入默认诊断包。

## 报告漏洞

在配置 GitHub 远端后，优先使用 GitHub Private Vulnerability Reporting 或私有 Security Advisory。不要在公开 Issue 中提交 API key、凭据、完整 `DSH_HOME`、原始 session、未经脱敏的日志或可直接复现本机 RCE 的敏感材料。

远端尚未配置时，请直接联系项目维护者，通过私有渠道提交：影响版本、平台、最小复现、攻击前提、影响范围和建议缓解。不要附带与问题无关的用户数据。

## 范围

重点包括 process ownership、IPC authentication、WebView isolation、Browser profile、PTY、path validation、log redaction、capability authorization、supply chain 和 release provenance。

完整模型见 [Threat Model](docs/security/THREAT_MODEL.md)。
