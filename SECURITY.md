# Security Policy

## 安全目标

- Attached DSH 永不因端口探测而被误杀。
- DSH WebView 与任意网页不能获得 privileged native API。
- Agent native action 必须经过 DSH tool/policy 与 Desktop capability grant。
- IPC 默认本地、认证、最小权限，并能拒绝 replay/stale generation。
- 凭据、Authorization、token、用户路径和 session 内容不得进入默认诊断包。

## 报告漏洞

本仓库已有 private GitHub 远端。若仓库启用了 Private Vulnerability Reporting，使用该入口；否则先通过维护者认可的私有渠道请求 Security Advisory 协作，不要把漏洞细节写入普通 Issue。任何渠道都不得附带与问题无关的用户数据。

不要在公开或普通协作区提交 API key、凭据、完整 `DSH_HOME`、原始 session、未经脱敏的日志或可直接复现本机 RCE 的敏感材料。

## 范围

重点包括 process ownership、IPC authentication、WebView isolation、Browser profile、PTY、path validation、log redaction、capability authorization、supply chain 和 release provenance。

完整模型见 [Threat Model](docs/security/THREAT_MODEL.md)。
