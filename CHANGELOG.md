# Changelog

本文件记录仓库级公开契约、治理与发布变化。模块级协议变化还必须更新对应 Schema、ADR 和 compatibility 记录。

## Unreleased

### Added

- 建立文档型 clean-room 项目仓库。
- 冻结 M0 架构基线、模块地图、协议草案和跟踪系统。
- 建立跨 Agent / Session 的工作项、lease、evidence 与 handoff 规则。
- 为 Supervisor wake guarantee 增加独立 ScheduleWake Schema。

### Changed

- 收紧 v1alpha1 Envelope：Hello/Agreement payload 结构化，Agreement 绑定 replyTo，Invocation/Result/Event kind 字段受限，Result success/error 互斥。
