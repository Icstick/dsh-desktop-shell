# Repository Map

```text
dsh-desktop-shell/
├─ apps/desktop/                  M1 Tauri/React shell implementation and module docs
├─ crates/                        Future Rust control-plane modules
├─ packages/                      Future TypeScript contracts and adapters
├─ protocol/                      Protocol-facing module documentation
├─ specs/                         Normative JSON Schemas
├─ tests/                         Future test-area contracts
├─ docs/                          Narrative architecture and governance
├─ tracking/                      Canonical project execution state
└─ .github/                       Contribution templates; executable CI not yet added
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

## Current M1 implementation paths

```text
apps/desktop/
├─ src/                           frontend entry, shared contracts, Tauri facade
├─ features/shell-ui/src/         Activity Rail and Desktop layout
├─ features/environment-settings/src/
│  └─                             Environment draft, form, validation tests
├─ features/harness-surface/src/  native Surface slot, lifecycle and fail-closed UI
├─ scripts/validate-acl.mjs       machine-checkable command/capability inventory gate
└─ src-tauri/
   ├─ build.rs                    AppManifest command allowlist
   ├─ capabilities/shell.json     exact Shell-window permissions
   └─ src/                        Tauri commands and Environment validation
```

`crates/`、`packages/`、`protocol/` 与 `tests/` 仍是模块契约镜像；后续实现不得绕过其中的 ownership 与接口边界。
