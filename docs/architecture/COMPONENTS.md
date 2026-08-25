# Components

| Component | 语言基线 | 职责 | 不负责 |
|---|---|---|---|
| Shell UI | React/TypeScript | Activity Rail、Setup、状态、Desktop surfaces | DSH 业务语义 |
| Tauri Backend | Rust | 窄 commands、state integration、window/webview | arbitrary exec/fs bridge |
| Supervisor | Rust | Environment、process、health、restart、ownership | Core 更新、Profile mutation |
| Capability Broker | Rust；P0 位于 Supervisor boundary | Agreement、Desktop grant、lease、scope、generation、provider dispatch | DSH tool/policy、Adapter mapping |
| Process Manager | Rust | process group、signals、identity、endpoint release | DSH protocol |
| Local Transport | Rust | local carrier、auth、framing、reconnect | capability semantics |
| Capability Contracts | TS + JSON Schema | stable internal API | Cordis type |
| Legacy DSH Adapter | TypeScript；DSH integration boundary | DSH-specific mapping | Desktop implementation、permission bypass |
| dsh-std Adapter | TypeScript；DSH integration boundary | standard mapping与conformance | core dependency、Desktop grant |
| Terminal Provider | Rust | PTY session lifecycle | Agent permission decision |
| Browser Provider | Rust launcher + optional TS sidecar | process/profile/session/CDP | unrestricted credential exposure |
| Usage Collector | TypeScript/DSH | normalized telemetry | Desktop rendering |

Capability Broker 在 P0 不是独立顶层模块：规范由 `MOD-CAPABILITY-CONTRACTS` / `specs/` 提供，受信任 dispatch 与 authorization enforcement 归 `MOD-SUPERVISOR`。若 P2 daemon 拆分改变该 ownership，必须由 M6 ADR 明确迁移。
