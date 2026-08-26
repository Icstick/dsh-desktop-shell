# Current Project State

- Phase：`shell-mvp`
- Milestone：M1 Shell MVP
- Status：`ready`
- Implementation authorized：`true`
- External baseline verified：2026-08-25
- Last updated：2026-08-25T13:12:46Z

## 当前状态

- Maintainer 已接受 [HANDOFF-M0-REVIEW](handoffs/HANDOFF-M0-REVIEW.yaml)，M0 与 [WI-M0-REVIEW](work-items/WI-M0-REVIEW.yaml) 已完成。
- M1 与 [WI-M1-SHELL](work-items/WI-M1-SHELL.yaml) 已进入 `ready`；当前没有 Agent claim，也尚未开始代码实现。

## 已完成

- 文档型仓库、Charter、架构、代码地图和治理基线。
- 10 个初始 ADR。
- 协议/config/tracking JSON Schema。
- M0–M7 路线、模块与接口登记。
- Threat model、compatibility、test、operations 和 clean-room policy。
- 结构化质量门禁通过；证据见 [REVIEW-M0-STRUCTURE](reviews/REVIEW-M0-STRUCTURE.yaml)。
- Architecture、Security、Interop 语义审查通过；证据分别见 [REVIEW-M0-ARCHITECTURE](reviews/REVIEW-M0-ARCHITECTURE.yaml)、[REVIEW-M0-SECURITY](reviews/REVIEW-M0-SECURITY.yaml)、[REVIEW-M0-INTEROP](reviews/REVIEW-M0-INTEROP.yaml)。
- 全仓最终门禁通过；证据见 [REVIEW-M0-FINAL-GATE](reviews/REVIEW-M0-FINAL-GATE.yaml)。
- 外部 baseline 已刷新并固定 repository revision、registry artifact、Tauri security 语义与许可边界；证据见 [REVIEW-M0-EXTERNAL-BASELINE](reviews/REVIEW-M0-EXTERNAL-BASELINE.yaml)。
- Baseline-aware M0 复审已修正 DSH Managed launch 与 Tauri custom command ACL 门禁，且未发现需要替代的 ADR；证据见 [REVIEW-M0-BASELINE-REASSESSMENT](reviews/REVIEW-M0-BASELINE-REASSESSMENT.yaml)。
- Maintainer 已显式批准实现授权；证据见 [REVIEW-M0-AUTHORIZATION](reviews/REVIEW-M0-AUTHORIZATION.yaml)。

## 当前门禁

`implementation_authorized: true` 允许在已认领工作项范围内进入实现，但不豁免 branch/session/lease、接口优先、ADR、模块安全审查、clean-room 与验证证据要求。

## 下一动作

另开 M1 branch/worktree 与 session，认领 [WI-M1-SHELL](work-items/WI-M1-SHELL.yaml)，依次读取 Shell UI、Environment Settings、Harness Surface 模块文档及相关 ADR 后开始实现。
