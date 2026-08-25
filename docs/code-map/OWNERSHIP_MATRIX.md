# Ownership Matrix

| Module | Owner role | Public surface | Security review | Milestone |
|---|---|---|---|---|
| shell-ui | UI Owner | navigation/state view | standard | M1 |
| harness-surface | UI Owner | DSH Web container | WebView | M1 |
| environment-settings | UI + Runtime | DshEnvironment | path/secret | M1 |
| supervisor | Runtime Owner | Runtime capability | high | M2 |
| process-manager | Runtime + Security | process group | critical | M2 |
| local-transport | Runtime + Security | carrier/auth | critical | M2 |
| terminal-provider | Runtime + Security | Terminal capability | critical | M3 |
| browser-provider | Browser + Security | Browser capability | critical | M4 |
| capability-contracts | Architecture Owner | Schemas/types | protocol | M0/M2 |
| adapter-dsh | Interop Owner | DSH mapping | high | M2/M5 |
| adapter-dsh-std | Interop Owner | standard mapping | protocol | M5 |
| usage-collector | Interop Owner | Usage events | privacy | M3 |

P0 Capability Broker 是 `MOD-SUPERVISOR` 的子组件，不建立第二份 module state；`MOD-PROTOCOL-SCHEMAS` 在 M0 冻结规范，`MOD-CAPABILITY-CONTRACTS` 在 M2 实现跨语言 consumer contract。两者不得合并状态或相互冒充 owner。
