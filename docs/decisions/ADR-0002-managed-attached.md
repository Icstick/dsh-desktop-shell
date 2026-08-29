---
id: ADR-0002
status: accepted
date: 2026-08-25
owner_role: architecture-owner
---

# ADR-0002: Managed and Attached Ownership

## 背景

发现 loopback endpoint 不能证明 Desktop 创建或拥有对应进程。错误接管可能终止用户外部任务。

## 决策

Environment 必须声明 ownership=managed|attached。Managed 由 Supervisor 创建并可 stop/restart/recover；Attached 只 connect/probe，默认拒绝所有 destructive lifecycle。

## 替代方案

- 自动接管端口：拒绝，无法证明 owner。
- Attach-only：无法实现产品核心 restart/recovery。
- 显式 handover：可作为未来独立协议，不属于 P0。

## 后果

UI、状态机、错误码与测试需要双分支；换来可证明的安全边界。

## 验证门禁

- Attached restart 返回 NOT_PROCESS_OWNER。
- PID reuse、stale PID、foreign port negative tests。
- UI 始终显示 ownership。
- Attached health probe 只能访问持久化 Environment 的固定 loopback endpoint，并受 backend-owned deadline 限制。
- TCP reachability 只能进入 identity=`unverified`、processOwnership=`external`；不得据此推导 DSH identity 或 Desktop ownership。
- Managed ownership 只来自 Supervisor 当前保留的 child/process-tree handle、opaque instance ID 与 generation；不得从 PID、端口、catalog 或启动输出单独重建。
- Managed endpoint publication 必须同时满足：当前 owned generation 仍存活、该 child 的 bounded output 出现冻结 baseline 的 `dsh web:` marker、candidate 是 exact `http://127.0.0.1:<port>` root URL、固定端口与 Environment 一致且 bounded TCP readiness 成功。Legacy credential-free 与 backend-owned authenticated bootstrap 的精确边界由 ADR-0012 收紧；完整 bootstrap credential 永不进入公开 endpoint。
- Managed stop 必须携带 expected generation，并且只能操作 retained process-tree handle；旧 generation 返回 `STALE_GENERATION`。M1 不实现自动 recovery 或 ownership handover。

## 受影响模块

supervisor、process-manager、harness-surface、runtime-diagnostics
