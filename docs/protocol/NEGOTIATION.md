# Capability Negotiation

## 声明

Participant 声明独立 `supports` 与 `requires`。Requirement 包含 coordinate 和 required flag；optional 不可用时仍可激活 degraded facet，required 不可用时拒绝 activation。

## Agreement

Agreement 记录：

- 精确 coordinate/version。
- granted capabilities。
- unavailable capabilities 与 reason。
- activation ID、generation、lease 约束。
- provider identity 与限流/审批策略提示。

## 降级

| Backend | 能力 |
|---|---|
| 普通 DSH，无 Adapter | Web Surface、Managed lifecycle、health |
| Legacy companion | Usage、Notification、restart reason |
| dsh-std adapter | negotiated protocol/facet |
| 未知 std version | Legacy/baseline，std unavailable |

Capability availability 不允许通过 `window.__DESKTOP__`、环境变量品牌名或 Desktop 版本号猜测。
