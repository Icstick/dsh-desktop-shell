# Components

| Component | 语言基线 | 职责 | 不负责 |
|---|---|---|---|
| Shell UI | React/TypeScript | Activity Rail、Setup、状态、Desktop surfaces | DSH 业务语义 |
| Tauri Backend | Rust | 窄 commands、state integration、window/webview | arbitrary exec/fs bridge |
| Supervisor | Rust | Environment、process、health、restart、ownership | Core 更新、Profile mutation |
| Process Manager | Rust | process group、signals、identity、endpoint release | DSH protocol |
| Local Transport | Rust | local carrier、auth、framing、reconnect | capability semantics |
| Capability Contracts | TS + JSON Schema | stable internal API | Cordis type |
| Legacy DSH Adapter | TypeScript | DSH-specific mapping | Desktop implementation |
| dsh-std Adapter | TypeScript | standard mapping与conformance | core dependency |
| Terminal Provider | Rust | PTY session lifecycle | Agent permission decision |
| Browser Provider | Rust launcher + optional TS sidecar | process/profile/session/CDP | unrestricted credential exposure |
| Usage Collector | TypeScript/DSH | normalized telemetry | Desktop rendering |
