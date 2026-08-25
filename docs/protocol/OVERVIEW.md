# Protocol Overview

协议分为三层：

1. Internal Capability Contract：Shell-neutral、DSH-neutral 的稳定语义。
2. Interop Adapter：Legacy DSH 或 optional dsh-std mapping。
3. Local Transport：Named Pipe、UDS 或 loopback carrier。

## Protocol Coordinate

每个 capability 使用：

```json
{"apiVersion":"runtime.dsh-desktop.local/v1alpha1","kind":"RuntimeControl"}
```

协议领域独立版本化，不存在全局 DesktopProtocolVersion。

## Message Kinds

- `Hello`：声明 participant、supports、requires 与 instance/generation。
- `Agreement`：返回 granted、unavailable、版本与限制。
- `Invocation`：调用已协商 capability method。
- `Result`：成功或结构化错误。
- `Event`：状态、progress、revocation 或 provider lifecycle。

规范见 [envelope.schema.json](../../specs/protocol/envelope.schema.json)。

## 最小规则

- Authentication 属于 transport，不写入业务 payload。
- 每条消息有 ID、participant、timestamp、generation。
- 迟到 generation 被拒绝。
- Unknown field 按 Schema fail closed。
- Accepted restart 在旧进程退出前返回。
- High-risk capability 需要 lease、scope 和 audit。
