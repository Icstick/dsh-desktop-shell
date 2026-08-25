# Migration Policy

任何持久配置或 public protocol 变化必须：

1. 标识 source/target version。
2. 定义 forward 与 rollback。
3. 保留用户原数据并先备份。
4. 提供 dry-run/validation。
5. 更新 Schema、compat matrix、changelog、support。
6. 明确 Desktop state 与 DSH_HOME 分离。

P0 不迁移 DSH_HOME/Profile。未来 daemonization、transport 与 Environment schema 迁移需独立 ADR。

## M0 v1alpha1 Pre-implementation Correction

- Source：初始 permissive Envelope draft；Target：kind-specific Hello/Agreement/Invocation/Result/Event constraints。
- Forward：Agreement 增加 `replyTo`；Result success payload 与 error 改为互斥；ScheduleWake 改用独立 Schema。
- Rollback：M0 尚无 implementation/persisted message，不提供运行时 rollback；旧 fixture 必须显式标记 superseded。
- Validation：按 `protocol/fixtures/README.md` 的 valid/invalid matrix 校验，且不迁移 `DSH_HOME`。
