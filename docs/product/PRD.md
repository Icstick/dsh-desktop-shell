---
id: DOC-PRD
status: review
owner_role: product-owner
verified_on: 2026-08-25
---

# Product Requirements Document

## 问题

用户已经拥有快速变化的 DeepSeek Harness、`DSH_HOME`、Profile 和插件，但缺少一个不会随 DSH restart、插件操作或上游升级一起消失的稳定桌面工作空间。普通 WebView wrapper 不能提供可靠进程管理、持久 PTY、Shared Browser、系统通知和长期后台运行。

## 产品目标

DSH Desktop Shell 在不拥有 DSH 发行的前提下，提供：

1. External DSH discovery、validation 与 Environment 管理。
2. Managed/Attached 两种明确 ownership。
3. 原版 DSH Web UI 的无侵入 Desktop Surface。
4. start、stop、health、restart、crash recovery 与 diagnostics。
5. 版本化 Capability Broker 与可替换 DSH/dsh-std Adapter。
6. 后续 Persistent Terminal、Shared Browser、Usage、Notification 和 wake guarantee。

## 目标用户

- 使用 npm/global DSH 的日常用户。
- 使用源码 checkout 开发 DSH 或插件的工程师。
- 维护多个 stable/dev/experimental Environment 的高级用户。
- 需要可审计、可恢复本地 Agent runtime 的团队。

## P0 范围

- First-run Setup：Harness、`DSH_HOME`、Profile、Node override、Managed/Attached、host/port。
- Environment discovery 与 validation，不写入 DSH。
- DSH Surface、loading/error/reconnect overlay。
- Supervisor、process ownership、health、restart、crash-loop fuse。
- Local authenticated transport 与最小 Runtime capability。
- Compatibility fixtures、diagnostics 与安全基线。

## 后续范围

- P1：Notification、Usage、Persistent Terminal、Shared Browser、optional dsh-std adapter。
- P2：独立 daemon、persistent provider ownership、Scheduler wake、hardening 与 stable release。

## 明确不做

- Bundled Core、Node/pnpm/Core updater、Desktop Plugin Market。
- DOM injection、renderer patch、unrestricted native bridge。
- Desktop 内重新实现 Agent/Session/Scheduler/plugin lifecycle。
- 在未协商和授权时向 Agent 暴露 terminal/browser mutation。

## 体验原则

- 首次配置发生在应用内，不塞进安装器。
- Attach 必须明确显示“externally managed”。
- 任何高权限能力默认不可用并解释原因。
- Adapter 故障时保留 Web + lifecycle 基线。
- DSH restart 只让 DSH Surface 短暂重连，Shell 与 Desktop-owned resources 保持。

## 成功指标类别

具体数值在 M1/M2 PoC 后冻结；M0 先定义测量面：

- Setup 成功率与诊断完整度。
- Managed start/restart/recovery 成功率。
- Attached ownership 零误杀。
- Crash-loop 到 safe-stop 的可预测性。
- DSH restart 后 route、Terminal、Browser resource 的保留率。
- 兼容版本覆盖与 degraded-mode 可解释率。
- 诊断包 secret redaction 通过率。
