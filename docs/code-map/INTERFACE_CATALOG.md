# Interface Catalog

规范真源位于 `specs/`，状态真源位于 `tracking/interfaces/`。

| ID | Capability / Contract | Owner | 初始阶段 |
|---|---|---|---|
| IF-ENVIRONMENT | DshEnvironment + EnvironmentCatalog + HarnessDiscovery | MOD-SUPERVISOR | M1 |
| IF-RUNTIME-STATUS | RuntimeStatus + AttachedHealth | MOD-SUPERVISOR | M1/M2 |
| IF-DSH-SURFACE-POLICY | DshSurfacePolicy + NavigationDecision | MOD-HARNESS-SURFACE | M1 |
| IF-DSH-SURFACE-LIFECYCLE | generation-bound mount/status/layout/reload/unmount + DshSurfaceStatus | MOD-HARNESS-SURFACE | M1 |
| IF-RUNTIME-CONTROL | M1 start/status/generation-bound stop；M2 restart/recovery | MOD-SUPERVISOR | M1/M2 |
| IF-NEGOTIATION | Hello/Agreement | MOD-CAPABILITY-CONTRACTS | M2 |
| IF-INVOCATION | Invocation/Result/Event | MOD-CAPABILITY-CONTRACTS | M2 |
| IF-LEASE | CapabilityLease | MOD-CAPABILITY-CONTRACTS | M2 |
| IF-NOTIFICATION | Native Notification | MOD-SHELL-UI | M3 |
| IF-USAGE | UsageTelemetry | MOD-USAGE-COLLECTOR | M3 |
| IF-TERMINAL | Terminal Surface/Automation | MOD-TERMINAL-PROVIDER | M3 |
| IF-BROWSER | Browser Surface/Automation | MOD-BROWSER-PROVIDER | M4 |
| IF-SCHEDULE-WAKE | Supervisor wake guarantee | MOD-SUPERVISOR | M6 |

函数级实现状态不追踪任意私有 helper，只追踪稳定公开操作，以避免符号重构造成无意义的状态漂移。
