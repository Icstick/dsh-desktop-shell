---
id: ADR-0013
status: accepted
date: 2026-08-28
owner_role: runtime-owner
---

# ADR-0013: Supervisor Restart and Bounded Recovery

## 背景

M1 Supervisor 支持 explicit start/status/exact-generation stop，并保留 owned process-tree、generation 与 endpoint readiness 语义。M2 需要可靠运行时的两个缺失能力：Managed restart（AC-RUN-001：restart 更新 generation 并恢复 DSH Surface）与 crash 恢复（AC-REC-001：恢复预算耗尽进入 Safe Stop）。DSH 可能随时退出（崩溃、端口被占、readiness 失败、外部 kill），无界自动重启会放大故障并掩盖真实错误。

## 决策

1. `restart` 操作：对同一持久化 Environment，先以 exact generation 停止 retained process tree，再以相同结构化 launch recipe 启动新 generation（端口语义不变：auto port 使用 `--port 0`，fixed port 保持 fail-closed）。旧 generation 的 Surface binding 立即失效；新 generation 的 binding 只有在 retained process-tree、output 与 bounded TCP readiness 同时成立时才可发布。restart 请求只携带 schemaVersion、environmentId 与 expectedGeneration；executable/argv/cwd/host/port 一律由 backend 从持久化 Environment 解析。
2. 意外退出（Healthy/Starting 期间 retained child 退出，而非 explicit stop）视为 crash。当且仅当 Environment policy 显式启用 `autoRestartOnCrash` 时，Supervisor 进行有界自动重启：单窗口内最多 `RECOVERY_BUDGET`（3）次 crash，窗口 `RECOVERY_WINDOW`（60 秒）；每次尝试都是新 generation，使用同一 retained launch spec，restart 间隔有界（bounded backoff）。
3. 预算耗尽（窗口内 crash 数达到上限且仍失败）进入 `safe_stop`：停止自动重启、释放 endpoint 与 bootstrap credential、保留可审计的 recovery evidence；显式 start 或 restart 重置 recovery 窗口与 generation。
4. Recovery 状态进入公开 RuntimeReport 的 `recovery` 字段（crashCount、windowStartUnixMs、budget、safeStop、lastCrashAtUnixMs），不包含任何秘密、token、query、bootstrap URL、PID 或用户数据。
5. crash 计数只针对 owned generation 的 retained process-tree；stale generation、foreign process、Attached 或 caller-supplied endpoint 永远不进入 recovery 状态（保持 fail-closed）。
6. `autoRestartOnCrash` 未启用时，crash 与 M1 一致：转换到 `crashed` 并等待显式 start/restart；不产生自动进程。

## 替代方案

- 无界自动重启：crash-loop 会耗尽资源并掩盖故障，拒绝。
- restart 复用 stop+start 两次请求：中间窗口暴露旧 generation 失效与新 generation 未就绪的状态，且无法保证原子 generation 语义，拒绝。
- 仅靠 PID/port 恢复：违反 retained-handle 原则，无法区分 PID reuse/foreign process，拒绝。
- 把 recovery 预算写入 catalog 持久化：预算属于进程生命周期，重启桌面应重置，拒绝。

## 后果

- RuntimeReport 增加 `safe_stop` 状态与 `recovery` 字段；IF-RUNTIME-CONTROL 增加 `restart` 操作。
- Shell 前端增加 restart 入口与 safe_stop/recovery 展示；Surface 在 generation 变化后按既有逻辑重新挂载。
- 崩溃恢复是 backend-owned 与 generation-bound 的；chaos 测试覆盖 crash-loop fuse、crash-after-ready 自动重启、重复/并发 start/stop/restart、stale restart 与端口冲突。
- macOS/Linux 与交互式 WebView2 smoke 仍按 M1 handoff 搁置；本 ADR 只改变 Supervisor 行为，不改变平台 gate。

## 验证门禁

- restart 更新 generation，旧 generation binding 拒绝，新 generation binding 可发布（AC-RUN-001）。
- crash-loop（autoRestartOnCrash）在预算内自动重启、预算耗尽进入 safe_stop 且不再启动（AC-REC-001）。
- 重复 stop/restart 幂等；并发 start/restart 返回 CONFLICT；restart 期间 stop 按 generation 语义拒绝。
- RuntimeReport 序列化测试证明 recovery 字段不含秘密；fixture 校验 safe_stop/recovery 组合。
- 未启用 autoRestartOnCrash 时 crash 只转 crashed，不自动重启。

## 受影响模块

- MOD-SUPERVISOR（apps/desktop/src-tauri/src/managed_runtime.rs，后续抽取至 crates/supervisor）
- MOD-SHELL-UI
- IF-RUNTIME-CONTROL / IF-RUNTIME-STATUS
