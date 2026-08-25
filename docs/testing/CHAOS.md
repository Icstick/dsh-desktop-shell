# Chaos Scenarios

- DSH 启动前/启动中/health 前退出。
- readiness 永不成功或极慢。
- 端口已占用、启动后被抢占、停止后延迟释放。
- stale PID、PID reuse、foreign process、orphan children。
- stop/restart 重复调用、并发 start、restart during recovery。
- Adapter negotiation 中断、旧 generation response、malformed/oversized IPC。
- Browser provider crash、PTY child exit、Shell UI restart。
- broken Profile、plugin boot failure、缺失 executable、无效 DSH_HOME。
- crash-loop fuse 进入 Safe Stop 并保留诊断。

每个 scenario 记录前置状态、注入点、期望转换、资源清理、用户可见结果和证据。
