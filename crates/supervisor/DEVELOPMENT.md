# Supervisor Development Contract

## Boundary

管理 Environment、Backend state、health、restart、recovery 与 ownership。 它只处理本模块列出的 ownership，不通过便利性绕过上层 policy、下层 abstraction 或 Adapter boundary。

## Data flow

```text
validated StartSpec + runtime requests + health/process events
  -> Supervisor
  -> RuntimeStatus + audited lifecycle events
```

## State and errors

实现必须暴露可观察状态，重复操作幂等或返回 `CONFLICT`。未授权/未协商返回标准错误；不得抛出包含秘密、原始命令或用户数据的跨边界错误。

## Security

Attached mutation hard deny；状态转换幂等；bounded recovery。P0 Broker 必须在 dispatch 前同时验证已协商 capability、Desktop grant、lease、scope、owner 与 generation；Adapter 认证或 Schema 合法均不能单独授予 native authority。Managed Web launch 必须强制 loopback 与 `--no-open`，拒绝用户参数覆盖 host/port/open policy。

## Compatibility

DSH/platform/std 特定差异集中在既定 adapter/provider；本模块不得使用版本号或品牌环境变量猜 capability。Auto port 使用当前已验证 recipe 的 `--port 0`，但 DSH 输出 URL 仅是 candidate；process identity、loopback host 与 readiness probe 全部通过后才更新 canonical endpoint/generation。

## Tests

- Happy path 与所有公开 operation。
- Invalid state、unavailable、unauthorized 和 cleanup。
- 所属 M2 acceptance catalog。
- Security/reliability 边界的 negative tests。
- Browser auto-open suppression、reserved arg collision、spoofed readiness URL 与 source checkout missing-build-artifact tests。

## Milestone exit

实现、测试、文档、tracking、evidence 与 handoff 同时完成；没有 evidence 时状态不得高于 `review`。
