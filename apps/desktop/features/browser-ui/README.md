# Browser UI

**Module ID:** `MOD-BROWSER-UI`
**Target milestone:** M4
**Canonical status:** [MOD-BROWSER-UI](../../../../tracking/modules/MOD-BROWSER-UI.yaml)

## Purpose

展示 Browser sessions、导航、owner 状态和 human takeover。

## Owns

- browser tabs/chrome
- takeover UX
- permission indicators

## Does not own

- raw CDP
- credential/autofill exposure

## Inputs

- Browser session/events

## Outputs

- human navigation/action/takeover intents

## Dependencies

- browser-provider

## Interfaces

- `IF-BROWSER`

规范真源见 [specs](../../../../specs/README.md)；架构原因见 [ADR index](../../../../docs/decisions/README.md)。
