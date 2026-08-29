# Local Transport

**Module ID:** `MOD-LOCAL-TRANSPORT`
**Target milestone:** M2
**Canonical status:** [MOD-LOCAL-TRANSPORT](../../tracking/modules/MOD-LOCAL-TRANSPORT.yaml)

## Purpose

提供认证 carrier、framing 和连接监督。当前实现为 loopback TCP（一次性 ephemeral credential、u32 长度前缀 framing、64 KiB 上限、deadline/并发限制）；Named Pipe/UDS 为 Carrier trait 扩展点，留待 daemon 阶段（ADR-0007/0008）。

## Owns

- endpoint lifecycle
- ACL/mode
- ephemeral credential
- framing/reconnect

## Does not own

- capability semantics
- plugin identity proof

## Inputs

- envelope bytes
- instance/generation

## Outputs

- authenticated message stream

## Dependencies

- protocol schemas

## Interfaces

- `IF-NEGOTIATION`
- `IF-INVOCATION`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
