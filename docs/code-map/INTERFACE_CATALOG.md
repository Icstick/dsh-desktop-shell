# Interface Catalog

规范真源位于 `specs/`，状态真源位于 `tracking/interfaces/`。

| ID | Capability / Contract | Owner | 初始阶段 |
|---|---|---|---|
| IF-ENVIRONMENT | DshEnvironment | supervisor | M1 |
| IF-RUNTIME-STATUS | RuntimeStatus | supervisor | M2 |
| IF-RUNTIME-CONTROL | start/stop/restart | supervisor | M2 |
| IF-NEGOTIATION | Hello/Agreement | capability-contracts | M2 |
| IF-INVOCATION | Invocation/Result/Event | capability-contracts | M2 |
| IF-LEASE | CapabilityLease | capability-contracts | M2 |
| IF-NOTIFICATION | Native Notification | shell/runtime | M3 |
| IF-USAGE | UsageTelemetry | usage-collector | M3 |
| IF-TERMINAL | Terminal Surface/Automation | terminal-provider | M3 |
| IF-BROWSER | Browser Surface/Automation | browser-provider | M4 |
| IF-SCHEDULE-WAKE | Supervisor wake guarantee | supervisor | M6 |

函数级实现状态不追踪任意私有 helper，只追踪稳定公开操作，以避免符号重构造成无意义的状态漂移。
