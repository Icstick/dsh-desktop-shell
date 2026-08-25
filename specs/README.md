# Normative Specifications

本目录是跨语言配置、协议和跟踪结构的规范真源。M0 只提供 JSON Schema，不生成 Rust/TypeScript binding 或 validator。

## Config

- [DshEnvironment](config/dsh-environment.schema.json)

## Protocol

- [Protocol Coordinate](protocol/protocol-coordinate.schema.json)
- [Envelope](protocol/envelope.schema.json)
- [Capability Lease](protocol/capability-lease.schema.json)
- [Runtime](protocol/runtime-capability.schema.json)
- [Terminal](protocol/terminal-capability.schema.json)
- [Browser](protocol/browser-capability.schema.json)
- [Usage](protocol/usage-capability.schema.json)
- [Notification](protocol/notification-capability.schema.json)

## Tracking

- [Project](tracking/project.schema.json)
- [Milestone](tracking/milestone.schema.json)
- [Module](tracking/module.schema.json)
- [Interface](tracking/interface.schema.json)
- [Work Item](tracking/work-item.schema.json)
- [Risk](tracking/risk.schema.json)
- [Handoff](tracking/handoff.schema.json)
- [Session](tracking/session.schema.json)
- [Review](tracking/review.schema.json)
- [Blocker](tracking/blocker.schema.json)

Schema 变化必须伴随 ADR/CHANGELOG、fixture 与 migration note。
