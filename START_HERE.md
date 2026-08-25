# START HERE

本文件是任何新的人类开发者或 Agent 的统一接手入口。不要依赖聊天记录、模型记忆或未提交分支恢复项目状态。

## 5 分钟：确认项目状态

依次读取：

1. [tracking/project.yaml](tracking/project.yaml)
2. [tracking/CURRENT.md](tracking/CURRENT.md)
3. 当前里程碑文件，例如 [tracking/milestones/M0.yaml](tracking/milestones/M0.yaml)
4. [Architecture Invariants](docs/architecture/INVARIANTS.md)

如果 `implementation_authorized` 为 `false`，禁止创建源码、构建清单、锁文件或可执行 CI workflow。

## 新贡献者路径

1. [CHARTER.md](CHARTER.md)
2. [PRD](docs/product/PRD.md)
3. [Architecture Overview](docs/architecture/OVERVIEW.md)
4. [ADR Index](docs/decisions/README.md)
5. [ROADMAP.md](ROADMAP.md)
6. [CONTRIBUTING.md](CONTRIBUTING.md)

## 模块开发者路径

1. [Repository Map](docs/code-map/REPOSITORY_MAP.md)
2. [Module Dependencies](docs/code-map/MODULE_DEPENDENCIES.md)
3. 目标模块的 `README.md`、`DEVELOPMENT.md`、`AGENTS.md`
4. [Specifications](specs/README.md) 与目标接口 Schema
5. 对应 `tracking/modules/MOD-*.yaml` 和 `tracking/interfaces/IF-*.yaml`
6. 认领或创建 `tracking/work-items/WI-*.yaml`

## 故障接手路径

1. [tracking/CURRENT.md](tracking/CURRENT.md)
2. 最近的 `tracking/handoffs/HANDOFF-*.md`
3. 对应 `tracking/blockers/`、`tracking/risks/`
4. [Diagnostics](docs/operations/DIAGNOSTICS.md)
5. [Recovery](docs/operations/RECOVERY.md)
6. [Chaos Strategy](docs/testing/CHAOS.md)

## Session 开始

- 选择单一 `WI-*` 作为主工作项。
- 记录 `claimed_by`、session、branch/worktree、`claim_expires_at`。
- 核对依赖与 acceptance criteria。
- 接口、状态机或 trust boundary 变化必须先更新 ADR/规范。

## Session 结束

- 更新工作项、模块状态、证据、风险与下一动作。
- 创建 `HANDOFF-*`；即便任务完成也要说明验证证据和残余风险。
- 不得把“代码已写”或“测试应该通过”当作完成证据。
