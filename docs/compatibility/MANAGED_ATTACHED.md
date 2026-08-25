# Managed and Attached Behavior

| Operation | Managed | Attached |
|---|---|---|
| Connect/render | allow | allow |
| Health/probe | allow | allow |
| Start | allow | not applicable |
| Graceful stop | allow | deny |
| Restart/recovery | allow | deny |
| Process-group force kill | last resort | deny |
| Adapter negotiation | optional | optional |
| Ownership handover | future protocol | future protocol |

UI 和 API 永远返回明确 ownership。Attach endpoint 丢失进入 Detached/Retry，不转换为 Managed。
