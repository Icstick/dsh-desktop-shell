# Local Transport

**Module ID:** `MOD-LOCAL-TRANSPORT`
**Target milestone:** M2
**Canonical status:** [MOD-LOCAL-TRANSPORT](../../tracking/modules/MOD-LOCAL-TRANSPORT.yaml)

## Purpose

提供 Named Pipe/UDS 与 loopback fallback 的认证 carrier、framing 和连接监督。

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
