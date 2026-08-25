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

## 受影响模块

supervisor、process-manager、harness-surface、runtime-diagnostics
