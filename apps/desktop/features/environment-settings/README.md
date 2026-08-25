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

## Dependencies

- supervisor

## Interfaces

- `IF-ENVIRONMENT`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。
