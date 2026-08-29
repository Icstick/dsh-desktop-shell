# DSH Surface Policy Fixture Matrix

Schema-valid fixtures freeze serialization. Evaluator tests must additionally cover this semantic matrix:

| Candidate | Kind / gesture | Decision |
|---|---|---|
| exact `http://127.0.0.1:<port>` origin with any path/query/fragment | main frame / either | allow in Surface |
| external HTTP(S) origin | main frame / explicit user gesture | delegate for separate human confirmation; never auto-open |
| external HTTP(S) origin | main frame / no gesture | deny |
| another loopback host or port, including `localhost`, `127/8` aliases and `::1` | main frame / either | deny origin mismatch |
| URL containing username/password | any | deny without echoing credentials |
| `file:`, `data:`, `javascript:`, `blob:` or custom scheme | any | deny |
| popup/new window, download or permission request | any | deny in DSH Surface |
| malformed or overlong URL | any | deny with sanitized evidence |

This matrix does not create a WebView and is not evidence that upstream DSH routing is compatible on any platform.

## Native lifecycle fixtures

- `dsh-surface-{mount,status,layout,reload,unmount}-request.valid.json` 冻结 Shell-only command 输入。
- `dsh-surface-mount-request.invalid.json` 故意加入 caller-supplied `endpoint`，必须因 `additionalProperties: false` 被拒绝。
- `dsh-surface-status.valid.json` 冻结 Windows ready state；`dsh-surface-status.unsupported.valid.json` 冻结非 Windows fail-closed state。

这些 fixtures 证明 serialization 与 contract boundary，不等于 native smoke evidence。Windows 支持仍需 WebView2 permission、navigation、popup/download 与 lifecycle 负向验证。
