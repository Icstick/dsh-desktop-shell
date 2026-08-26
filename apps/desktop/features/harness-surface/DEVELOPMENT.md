# Harness Surface Development Contract

## Boundary

承载原版 DSH Web UI，并提供 loading、error、reconnect 与 route hint 恢复。 它只处理本模块列出的 ownership，不通过便利性绕过上层 policy、下层 abstraction 或 Adapter boundary。

## Data flow

```text
healthy endpoint + generation + route hint
  -> Harness Surface
  -> surface state and user-visible diagnostics
```

## State and errors

实现必须暴露可观察状态，重复操作幂等或返回 `CONFLICT`。未授权/未协商返回标准错误；不得抛出包含秘密、原始命令或用户数据的跨边界错误。

## Security

DSH WebView 零 privileged Tauri capability；其 label 不匹配任何 custom command permission 或 remote URL access。所有应用 command 必须进入 AppManifest inventory，不能用 invoke-handler-only registration 绕过 ACL。

## Compatibility

DSH/platform/std 特定差异集中在既定 adapter/provider；本模块不得使用版本号或品牌环境变量猜 capability。

## Tests

- Happy path 与所有公开 operation。
- Invalid state、unavailable、unauthorized 和 cleanup。
- 所属 M1 acceptance catalog。
- Security/reliability 边界的 negative tests。
- 枚举全部 custom commands，并证明 DSH WebView 调用均被拒绝。

## Milestone exit

实现、测试、文档、tracking、evidence 与 handoff 同时完成；没有 evidence 时状态不得高于 `review`。
