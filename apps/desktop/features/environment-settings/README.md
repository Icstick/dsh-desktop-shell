# Environment Settings

**Module ID:** `MOD-ENVIRONMENT-SETTINGS`
**Target milestone:** M1
**Canonical status:** [MOD-ENVIRONMENT-SETTINGS](../../../../tracking/modules/MOD-ENVIRONMENT-SETTINGS.yaml)

## Purpose

配置和验证用户已有 Harness、DSH_HOME、Profile、endpoint 与 ownership。

## Owns

- first-run/setup forms
- Environment selection
- validation presentation

## Does not own

- 安装 Node/DSH
- 写 Profile/DSH_HOME

## Inputs

- DshEnvironment schema
- discovery candidates

## Outputs

- validated Environment intent
- canonical Environment catalog revision and active selection
- structured, non-executing Harness discovery evidence

## Dependencies

- supervisor

## Interfaces

- `IF-ENVIRONMENT`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。

## Implementation entry

- [`environment-draft.ts`](src/environment-draft.ts)：默认草稿、持久化记录恢复与前端结构校验。
- [`EnvironmentSetup.tsx`](src/EnvironmentSetup.tsx)：discovery、validation 与 explicit save 交互；保存不启动 DSH，并提示 Attached health 需要固定 loopback port。
- [`environment-draft.test.ts`](src/environment-draft.test.ts)：默认值、端口、literal args、恢复和 Supervisor-owned args 测试。
- [`environment_store.rs`](../../src-tauri/src/environment_store.rs)：AppData/Application Support catalog、revision、backup 与完整性拒绝。
- [`discovery.rs`](../../src-tauri/src/discovery.rs)：explicit/DSH_PATH/PATH 候选检查；不执行候选、不安装、不构建。
