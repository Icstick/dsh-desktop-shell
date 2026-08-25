# Timer UI

**Module ID:** `MOD-TIMER-UI`
**Target milestone:** M3
**Canonical status:** [MOD-TIMER-UI](../../../../tracking/modules/MOD-TIMER-UI.yaml)

## Purpose

提供 Desktop countdown、stopwatch 与 pomodoro。

## Owns

- local timer state
- notification presentation

## Does not own

- Agent Scheduler
- unattended Agent execution

## Inputs

- user timer settings

## Outputs

- local events/notifications

## Dependencies

- notification

## Interfaces

- No standalone public interface; consumed through owning module contracts.

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。
