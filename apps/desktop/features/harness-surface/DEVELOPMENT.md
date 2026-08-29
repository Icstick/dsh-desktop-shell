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

实现必须暴露可观察状态，重复操作幂等或返回结构化错误。Policy pending 与 ready 必须显式；只有 backend report 同时给出 current generation、owned process tree 与 verified readiness，Shell 才能提交不含 endpoint/URL/label 的 mount request。Decision 只返回 sanitized origin，不回显 path/query/fragment/credential。未授权/未协商返回标准错误；不得抛出包含秘密、原始命令或用户数据的跨边界错误。

Native lifecycle state 固定为 `unmounted -> mounting -> loading -> ready|hidden|error`；generation mismatch 进入 `stale` 并关闭旧 child，未支持平台进入 `unsupported_platform` 且保持 unmounted。重复 mount/update/reload/unmount 必须绑定相同 Environment 与 generation；不能跨 generation 复用 child WebView。Shell 只在 slot 可视区域至少为 320 × 240 CSS pixel 时 mount，rail 切换隐藏，runtime binding 丢失或组件 teardown 时 unmount；status failure 使用 generation-scoped latch，只有显式 retry 或新 generation 才解除。

## Security

DSH WebView 零 privileged Tauri capability；其 label 不匹配任何 custom command permission 或 remote URL access。所有应用 command 必须进入 AppManifest inventory，不能用 invoke-handler-only registration 绕过 ACL。Allowed origin 只能由 backend 从 persisted fixed-loopback Environment 或 verified Managed binding 派生；external delegate 需要 user gesture 与外层 human confirmation，永不自动打开。Native lifecycle slice 只在满足 `IF-DSH-SURFACE-LIFECYCLE`、ADR-0011 与 AC-WEB-005..007 时允许 Windows backend 创建 fixed-label child；仍禁止 initialization script、page eval、DOM/renderer patch、external auto-open、caller-supplied endpoint 和任何 DSH Surface capability/permission。非 Windows backend 必须在 WebView creation 前 fail closed。

## Compatibility

DSH/platform/std 特定差异集中在既定 adapter/provider；本模块不得使用版本号或品牌环境变量猜 capability。

## Tests

- Happy path 与所有公开 operation。
- Invalid state、unavailable、unauthorized 和 cleanup。
- 所属 M1 acceptance catalog。
- Security/reliability 边界的 negative tests。
- 枚举全部 custom commands，并证明 DSH WebView 调用均被拒绝。
- Exact same-origin path/query/fragment、external gesture/no-gesture、另一 loopback host/port、IPv4 alias、bracketed IPv6 loopback、credentialed/non-HTTP URL、popup、download、permission 与 malformed/overlong URL。
- Policy ready/pending UI、Managed verified/stopped 呈现、sanitized evidence、最小 lifecycle request、rail hide、error reload、undersized gate、无 iframe/webview/script DOM 和 console-clean responsive visual QA。
- Windows WebView2 real-DSH smoke 必须验证 permission deny、redirect/cross-origin、popup、download、load failure、resize/hide/show/reload/unmount；完成前不得把 automated gates写成平台支持声明。

## Milestone exit

实现、测试、文档、tracking、evidence 与 handoff 同时完成；没有 evidence 时状态不得高于 `review`。
