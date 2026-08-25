# Shell UI

**Module ID:** `MOD-SHELL-UI`
**Target milestone:** M1
**Canonical status:** [MOD-SHELL-UI](../../../../tracking/modules/MOD-SHELL-UI.yaml)

## Purpose

提供 Activity Rail、Desktop layout、全局状态与 Surface 切换。

## Owns

- 导航与可访问性
- Desktop-only 状态展示
- Surface selection

## Does not own

- DSH 内部导航/DOM
- 进程与 resource lifecycle

## Inputs

- RuntimeStatus
- Environment selection

## Outputs

- 用户意图对应的结构化 Tauri commands

## Dependencies

- capability contracts

## Interfaces

- `IF-RUNTIME-STATUS`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。
