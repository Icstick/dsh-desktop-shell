# Non-functional Requirements

## Reliability

- Supervisor 状态转换必须可观察、幂等并处理重复请求。
- 进程停止遵循 graceful -> soft termination -> force process-group termination -> endpoint verification。
- crash recovery 有预算、backoff 和 Safe Stop。
- Shell failure 不应破坏用户 DSH 数据；P2 后 Shell restart 不影响 Supervisor。

## Security

- Fail closed、least privilege、local-only by default。
- Attached lifecycle mutation 被硬拒绝。
- WebView 无 privilege；Browser/Terminal mutation 需要独立 grant。
- 诊断、日志、IPC 和 tracking 中不存凭据正文。

## Compatibility

- Baseline、Enhanced、Standard-aware 三梯度。
- DSH 与 dsh-std 变化集中在 Adapter。
- 未知版本给出 unsupported/degraded，不猜测兼容。

## Portability

- Windows、macOS、Linux 分别验证 process tree、WebView、path、IPC 与权限。
- Tauri system WebView 差异不得由单平台 CI 推断。

## Maintainability

- 公开契约有 Schema、版本、owner、fixture 和 migration policy。
- 一个模块一个明确职责；跨模块依赖保持单向。
- 状态、接口、决策分别只有一个真源。

## Observability

- 结构化日志包含 instance、environment、generation、state、operation、correlation ID。
- 默认日志脱敏；诊断包说明被移除字段。
- 所有 restart、grant、revoke、ownership change 可审计。
