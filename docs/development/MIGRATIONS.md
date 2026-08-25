# Migration Policy

任何持久配置或 public protocol 变化必须：

1. 标识 source/target version。
2. 定义 forward 与 rollback。
3. 保留用户原数据并先备份。
4. 提供 dry-run/validation。
5. 更新 Schema、compat matrix、changelog、support。
6. 明确 Desktop state 与 DSH_HOME 分离。

P0 不迁移 DSH_HOME/Profile。未来 daemonization、transport 与 Environment schema 迁移需独立 ADR。
