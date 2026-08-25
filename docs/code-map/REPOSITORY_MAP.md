# Repository Map

```text
dsh-desktop-shell/
├─ apps/desktop/                  Shell UI and Tauri integration docs
├─ crates/                        Future Rust control-plane modules
├─ packages/                      Future TypeScript contracts and adapters
├─ protocol/                      Protocol-facing module documentation
├─ specs/                         Normative JSON Schemas
├─ tests/                         Future test-area contracts
├─ docs/                          Narrative architecture and governance
├─ tracking/                      Canonical project execution state
└─ .github/                       Contribution templates, no workflows in M0
```

## Desktop Features

`apps/desktop/features/`：

- shell-ui
- harness-surface
- environment-settings
- runtime-diagnostics
- terminal-ui
- browser-ui
- usage-ui
- timer-ui

## Rust Control Plane

- `crates/supervisor`
- `crates/process-manager`
- `crates/local-transport`
- `crates/terminal-provider`
- `crates/browser-provider`

## TypeScript Interop

- `packages/capability-contracts`
- `packages/adapter-dsh`
- `packages/adapter-dsh-std`
- `packages/usage-collector`
- `packages/browser-agent-adapter`
- `packages/terminal-agent-adapter`

每个叶模块必须有 README、DEVELOPMENT、AGENTS；状态不写死在模块文档，链接到对应 `tracking/modules/MOD-*.yaml`。
