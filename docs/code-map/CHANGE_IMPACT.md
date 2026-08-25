# Change Impact Guide

| 变更 | 必须同步 |
|---|---|
| Environment 字段 | Schema、ADR、Setup UX、migration、compat fixture |
| Backend state | lifecycle doc、Supervisor module、UI state、chaos cases |
| Protocol envelope | Schema、versioning、adapter fixtures、CHANGELOG |
| Capability method | interface tracking、provider/adapter docs、security matrix |
| Ownership 规则 | ADR、threat model、negative tests、support diagnostics |
| Transport/auth | ADR、threat model、deployment、conformance fixtures |
| DSH compatibility | source baseline、adapter matrix、degradation UX |
| Milestone exit | milestone YAML、CURRENT、work items、review evidence |

任何“只改一处”的 public contract 变更都视为不完整。
