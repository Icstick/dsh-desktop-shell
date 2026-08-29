# Rust Crates

M0 建立时为文档型契约镜像；M2 起 crates/local-transport 与 crates/supervisor 已实现并纳入根 Cargo workspace。

- crates/local-transport（MOD-LOCAL-TRANSPORT）：认证 loopback carrier、framing/limits（AC-IPC-001/002）。
- crates/supervisor（MOD-SUPERVISOR）：P0 Capability Broker grant/lease/dispatch（ADR-0014，AC-LEASE-001）。
- crates/process-manager 与 crates/browser-provider、crates/terminal-provider 仍为契约镜像（M2 后续/相应里程碑实现）。

依赖方向：Supervisor 可依赖 process/transport/providers；底层 crates 不依赖 Tauri UI 或 DSH internals。
