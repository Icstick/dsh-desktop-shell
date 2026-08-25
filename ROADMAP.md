# Roadmap

项目使用相对周次；实际起始日期在项目获批后写入 `tracking/project.yaml`。

| Milestone | 周次 | 目标 | 退出标准 |
|---|---:|---|---|
| M0 Architecture Freeze | 启动前 | 边界、协议、安全和治理冻结 | ADR、Schema、威胁模型和文档体系通过 review |
| M1 Shell MVP | 1–4 | Environment、Discovery、Managed/Attached、DSH Surface | 能指定已有 DSH、验证、启动/连接并显示原版 UI |
| M2 Reliable Runtime | 5–8 | Supervisor、health、restart、process ownership、IPC | crash/port/stale PID/ownership chaos cases 通过 |
| M3 Workbench | 9–13 | Notification、Persistent Terminal、Usage、Diagnostics | DSH restart 后 PTY 存活；Usage/通知可审计 |
| M4 Shared Browser | 14–17 | Browser provider、human takeover、权限隔离 | Browser contract、安全与隔离测试通过 |
| M5 Interop | 18–19 | Legacy 与 optional dsh-std adapter | 两类 adapter 共存并可降级 |
| M6 Daemon | 20–24 | UI/Supervisor 生命周期隔离、持久资源、wake | UI 重启不影响 DSH/PTY；调度唤醒有策略 |
| M7 Stable Candidate | 25–27 | 三平台 hardening、签名、provenance | release checklist、兼容矩阵和供应链证据完整 |

依赖关系、验收门禁和人月估算见 [Milestones](docs/roadmap/MILESTONES.md)。
