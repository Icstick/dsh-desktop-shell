# Error Model

| Code | 含义 | Retry |
|---|---|---|
| UNAVAILABLE | provider/capability 当前不可用 | 条件变化后 |
| UNAUTHORIZED | 未授权、lease 无效或 scope 不符 | 重新授权 |
| UNSUPPORTED_VERSION | coordinate/version 不兼容 | 升降级 adapter |
| NOT_PROCESS_OWNER | Attached 或 identity 不匹配 | 不可自动重试 |
| USER_GESTURE_REQUIRED | 必须由用户直接触发 | 用户操作后 |
| USER_DENIED | 用户拒绝 | 不自动重试 |
| STALE_GENERATION | 消息来自旧 DSH generation | 新连接重试 |
| MALFORMED_MESSAGE | Schema/framing 无效 | 修复调用方 |
| CONFLICT | state transition 或 resource 冲突 | 查询状态后 |
| TIMEOUT | provider/readiness 超时 | 依 policy |
| SAFE_STOP | recovery budget 耗尽 | 用户干预 |

错误必须包含 machine code、最小安全 message、retryable 和 correlation ID；内部 path、credential、raw command 不进入外部 message。
