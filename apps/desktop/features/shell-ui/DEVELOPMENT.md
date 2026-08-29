# Shell UI Development Contract

## Boundary

提供 Activity Rail、Desktop layout、全局状态与 Surface 切换。 它只处理本模块列出的 ownership，不通过便利性绕过上层 policy、下层 abstraction 或 Adapter boundary。

## Data flow

```text
RuntimeStatus + Environment selection
  -> Shell UI
  -> 用户意图对应的结构化 Tauri commands
```

## State and errors

实现必须暴露可观察状态，重复操作幂等或返回 `CONFLICT`。Attached probe 只消费 backend report，不能从 TCP 可达性推断 DSH identity、进程 ownership 或 lifecycle authority。Managed UI 只呈现 backend report；restore/save 不自动 start，stop 必须二次确认并携带当前 report 的 generation。Environment 变化必须先清除旧 Managed report。未授权/未协商返回标准错误；不得抛出包含秘密、原始命令或用户数据的跨边界错误。

## Security

只允许 build-time inventory 中声明且授予精确 Shell WebView label 的窄命令。Shell 不解释、执行或记录 Environment 的 path/args；路径输入、校验与持久化均经 `IF-ENVIRONMENT` 结构化边界。Managed lifecycle request 只能传 Environment ID，stop 额外传 `expectedGeneration`；不能传 PID、port、argv 或 endpoint。Surface lifecycle request 只能增加 logical bounds/visibility，不能传 origin/URL/label/permission。Shell 只能读取 backend-derived policy/decision；不能自行构造 allowed origin 或自动打开 delegated URL。`dsh-surface` 不匹配 Shell capability。

## Compatibility

DSH/platform/std 特定差异集中在既定 adapter/provider；本模块不得使用版本号或品牌环境变量猜 capability。

## Tests

- Happy path 与所有公开 operation。
- Invalid state、unavailable、unauthorized 和 cleanup。
- 所属 M1 acceptance catalog。
- Security/reliability 边界的 negative tests。
- Active Environment bootstrap restore、explicit save 后 snapshot refresh 与 backend-unavailable degradation。
- Attached startup auto-probe、bounded manual re-probe、fixed-port error、evidence presentation 与 Start/Stop/Restart absence。
- Managed persisted-only explicit start、status evidence、two-step exact-generation stop、stale report fail closed、backend error 与 restore/save no-auto-start。
- DSH Surface policy ready/pending、verified-generation mount、layout hide/show、status polling、explicit retry/unmount、minimum viewport、IPC/automatic-open denial 与 remote child 无 DOM bridge。

## Milestone exit

实现、测试、文档、tracking、evidence 与 handoff 同时完成；没有 evidence 时状态不得高于 `review`。
