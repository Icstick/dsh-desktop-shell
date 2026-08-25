# Diagnostics

诊断页应显示 Desktop/DSH/Adapter/provider 版本、Environment ID、ownership、resolved launch category、backend state、instance/generation、endpoint category、最近安全错误和 redacted logs。

## Export

导出前双重 redaction；manifest 说明 included/excluded fields、生成时间、版本和 hash。默认不包含 plugin list、完整命令、绝对用户路径、环境变量、credentials、settings、session 或 terminal/browser content。

## Triage

1. Config/Discovery
2. Process ownership/start
3. Health/Web endpoint
4. Adapter negotiation
5. Provider
6. UI Surface

每层单独给出 unavailable/degraded，不用一个“启动失败”覆盖所有原因。
