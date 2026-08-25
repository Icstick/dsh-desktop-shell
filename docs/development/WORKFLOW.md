# Development Workflow

1. 从 tracking 选择 ready 工作项。
2. 读取 module docs、ADR、Schema、acceptance。
3. 认领 24 小时 advisory lease。
4. Contracts first：公开行为先写/改 Schema 与 fixture。
5. 在模块 boundary 内实现；跨 boundary 先 ADR。
6. 执行 unit/contract/security 和相关 platform tests。
7. 更新 evidence、module/interface state、changelog、handoff。
8. PR 经 CODEOWNER/security gate，squash merge。

M0 期间步骤 5 仅允许文档/规范，不允许源码。
