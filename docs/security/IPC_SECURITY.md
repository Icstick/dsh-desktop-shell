# Local IPC Security

## Native Carrier

- Windows Named Pipe 使用当前用户 ACL 与 instance-specific name。
- Unix Domain Socket 放在 user runtime directory，限制 filesystem mode。
- Loopback fallback 使用随机端口与 ephemeral bearer credential，仅绑定 127.0.0.1。

## Identity

每次 start 产生 instance ID、generation、launch identity 和 credential。Token 证明 process-level membership，不证明特定 DSH plugin 身份。

## Required Negative Tests

- missing/invalid/replayed token。
- other-user connection。
- stale generation。
- malformed framing/schema。
- oversized payload / slow client。
- endpoint reuse/hijack。
- disconnect during invocation。
- attached process spoof。
