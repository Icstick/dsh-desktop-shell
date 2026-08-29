# Normative Specifications

本目录是跨语言配置、协议和跟踪结构的规范真源。M0 只提供 JSON Schema，不生成 Rust/TypeScript binding 或 validator。

## Config

- [DshEnvironment](config/dsh-environment.schema.json)
- [Environment Catalog](config/environment-catalog.schema.json)
- [Harness Discovery Request](config/harness-discovery-request.schema.json)
- [Harness Discovery Report](config/harness-discovery-report.schema.json)

Config fixtures 位于 [`config/fixtures/`](config/fixtures/)；`.valid.json` 必须通过对应 Schema，`.invalid.json` 必须被拒绝。

## Runtime Status

- [Attached Health Request](runtime/attached-health-request.schema.json)
- [Attached Health Report](runtime/attached-health-report.schema.json)
- [Managed Start Request](runtime/managed-runtime-start-request.schema.json)
- [Managed Status Request](runtime/managed-runtime-status-request.schema.json)
- [Managed Stop Request](runtime/managed-runtime-stop-request.schema.json)
- [Managed Restart Request](runtime/managed-runtime-restart-request.schema.json)
- [Managed Runtime Report](runtime/managed-runtime-report.schema.json)
- [Diagnostics Report](runtime/diagnostics-report.schema.json)

Runtime fixtures 位于 [`runtime/fixtures/`](runtime/fixtures/)。M1 Attached probe 只返回 bounded loopback TCP reachability；Managed endpoint 只有在 retained child/generation、`dsh web:` exact loopback candidate 与 bounded TCP readiness 同时成立时发布。ADR-0012 允许 legacy root 或 backend-only authenticated token root，但所有 public serialization 继续只暴露 sanitized origin。M2（ADR-0013）增加 `restart` 操作与有界恢复：RuntimeReport 携带 credential-free `recovery` 历史，预算耗尽发布 `safe_stop` 且不自动重启。

M2-B Diagnostics（AC-LOG-001）把 Supervisor、Surface、catalog 与 process 状态汇成只读 `DiagnosticsReport`：所有字段 redacted——禁止 token/query/bootstrap URL/cookie/完整 URL/PID 等秘密；runtime endpoint 只暴露 `127.0.0.1` host 与 port，recovery 只暴露 crashCount/budget/safeStop。

## DSH Surface WebView Policy

- [Policy Request](webview/dsh-surface-policy-request.schema.json)
- [Derived Policy](webview/dsh-surface-policy.schema.json)
- [Navigation Request](webview/dsh-surface-navigation-request.schema.json)
- [Navigation Decision](webview/dsh-surface-navigation-decision.schema.json)

WebView fixtures 位于 [`webview/fixtures/`](webview/fixtures/)。M1 policy slice 只冻结并实现 fail-closed evaluator；它不创建远程 WebView、不自动打开 external URL，也不授予 DSH Surface privileged IPC。

## DSH Surface Lifecycle

- [Mount Request](webview/dsh-surface-mount-request.schema.json)
- [Status Request](webview/dsh-surface-status-request.schema.json)
- [Layout Request](webview/dsh-surface-layout-request.schema.json)
- [Reload Request](webview/dsh-surface-reload-request.schema.json)
- [Unmount Request](webview/dsh-surface-unmount-request.schema.json)
- [Surface Status](webview/dsh-surface-status.schema.json)

Lifecycle request 只接受 Environment ID、expected generation 和必要的 logical bounds/visibility；endpoint、origin、URL、label、permission 与 capability 必须由 backend 固定或从 verified Managed binding 派生。M1 native implementation 由 ADR-0011 限定为 Windows；其他平台返回 `unsupported_platform` 并保持 unmounted。

## Terminal

- [Create Request](terminal/terminal-create-request.schema.json)
- [Write Request](terminal/terminal-write-request.schema.json)
- [Resize Request](terminal/terminal-resize-request.schema.json)
- [Close Request](terminal/terminal-close-request.schema.json)
- [PTY Report](terminal/terminal-report.schema.json)
- [Output Event](terminal/terminal-output-event.schema.json)

M3 只允许 human_surface 模式；agent_automation fail-closed（ADR-0015）。

## Notification

- [Request](notification/notification-request.schema.json)
- [Report](notification/notification-report.schema.json)
- [Audit Record](notification/notification-record.schema.json)

内容策略 title_only/redacted_summary/explicit_body 由 schema 强制（ADR-0016）。

## Usage

- [Usage Record](usage/usage-record.schema.json)
- [Snapshot Request](usage/usage-snapshot-request.schema.json)
- [Snapshot](usage/usage-snapshot.schema.json)

usage 本地优先、无网络外发（AC-USG-002）。

## Protocol

- [Protocol Coordinate](protocol/protocol-coordinate.schema.json)
- [Envelope](protocol/envelope.schema.json)
- [Capability Lease](protocol/capability-lease.schema.json)
- [Runtime](protocol/runtime-capability.schema.json)
- [Terminal](protocol/terminal-capability.schema.json)
- [Browser](protocol/browser-capability.schema.json)
- [Usage](protocol/usage-capability.schema.json)
- [Notification](protocol/notification-capability.schema.json)
- [Schedule Wake](protocol/schedule-wake-capability.schema.json)

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
