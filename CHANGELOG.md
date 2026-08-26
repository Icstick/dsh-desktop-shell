# Changelog

本文件记录仓库级公开契约、治理与发布变化。模块级协议变化还必须更新对应 Schema、ADR 和 compatibility 记录。

## Unreleased

### Added

- 建立文档型 clean-room 项目仓库。
- 冻结 M0 架构基线、模块地图、协议草案和跟踪系统。
- 建立跨 Agent / Session 的工作项、lease、evidence 与 handoff 规则。
- 为 Supervisor wake guarantee 增加独立 ScheduleWake Schema。
- 增加可重现的 external baseline，记录官方 commit、release、npm dist-tag 与 artifact integrity。

### Changed

- 收紧 v1alpha1 Envelope：Hello/Agreement payload 结构化，Agreement 绑定 replyTo，Invocation/Result/Event kind 字段受限，Result success/error 互斥且 error 强制 correlation ID。
- CapabilityLease 禁止空 scope；Usage period 拒绝未知字段。
- 将 M1 DSH fixtures 固定为 `0.1.1-rc.2`/`0.1.1-rc.1`，并显式区分 dsh-std 的 `latest` 与 `rc` 标签。
- Managed DSH launch 固定 loopback、`--no-open` 与可验证的 auto-port 流程；source checkout 仅接受用户预构建产物。
- 收紧 Tauri 自定义命令门禁：完整 AppManifest inventory、最小 permission、精确 Shell label，并禁止 invoke-handler-only command。
