# Log Redaction

日志采用 allowlist fields：timestamp、level、component、operation、state、environment ID、instance ID、generation、correlation ID、safe error code、duration。

默认删除或哈希：

- Authorization、cookie、token、API key、credential。
- 完整环境变量、command line secrets。
- Harness argv、Node override 参数和进程命令行；默认只记录 executable category 与参数数量。
- 用户绝对路径；必要时保留 path category 或稳定 hash。
- DSH settings/session body、terminal content、browser form value。
- Query string 中的敏感参数。

诊断导出执行第二次 redaction，并输出 manifest 说明包含/排除类别。Golden corpus 必须覆盖常见 provider key、Bearer、Windows/Unix path、URL credential 和 multiline secret。
