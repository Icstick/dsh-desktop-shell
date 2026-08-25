# Language Boundaries

## Rust

Supervisor state machine、process manager、Job Object/process group、local IPC server、PTY backend、browser process launcher。

## TypeScript

React UI、Capability contracts consumer types、DSH/DSH-std adapters、Usage collector、Agent tool adapters、可选 Browser automation sidecar。

## JSON Schema

跨语言 wire/config/tracking 真源。Rust/TS 生成或 validator 属于后续实现，不在 M0 创建。

规则：Rust 不暴露任意 command；TS contract 不 import Cordis；UI 不持有 process/PTY；DSH-specific type 只在 adapter。
