# Architecture Decision Records

ADR 是 ownership、public contract、状态机、transport、trust boundary、技术栈和许可决策的原因真源。

## 状态

`proposed -> accepted -> superseded`；已接受 ADR 不原地改写结论。需要改变决策时创建新 ADR，并在旧文档中记录 superseded-by。

## 初始决策

- [ADR-0001](ADR-0001-external-core.md)
- [ADR-0002](ADR-0002-managed-attached.md)
- [ADR-0003](ADR-0003-tauri-stack.md)
- [ADR-0004](ADR-0004-unmodified-upstream-ui.md)
- [ADR-0005](ADR-0005-versioned-capabilities.md)
- [ADR-0006](ADR-0006-dsh-std-optional.md)
- [ADR-0007](ADR-0007-local-transport.md)
- [ADR-0008](ADR-0008-supervisor-evolution.md)
- [ADR-0009](ADR-0009-responsibility-exclusions.md)
- [ADR-0010](ADR-0010-license-clean-room.md)
- [ADR-0011](ADR-0011-platform-gated-native-dsh-surface.md)
- [ADR-0012](ADR-0012-authenticated-managed-web-bootstrap.md)
- [ADR-0013](ADR-0013-supervisor-restart-recovery.md)
- [ADR-0014](ADR-0014-capability-broker-grant-lease.md)
- [ADR-0021](ADR-0021-shell-daemon-identity-binding.md) — accepted（Shell–Daemon 身份绑定 option C；H-2 未关闭，见 ADR-0022 预留；本索引 0015-0020 未登记，属既有漂移）
- [ADR-0022](ADR-0022-peer-identity.md) — accepted（local-transport peer identity；B2 方向，spike 已完成，实现分片推进中；落地后 H-2 才可标「已关闭」）
- [ADR-0023](ADR-0023-user-gesture-gate.md) — accepted（lease `approval_required` 接成用户手势闸门；v0.1.0 之后实施）
