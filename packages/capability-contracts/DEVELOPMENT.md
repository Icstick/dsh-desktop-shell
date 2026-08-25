# Capability Contracts Development Contract

## Boundary

为 UI、Rust boundary 与 adapters 提供 DSH-neutral 协议模型。 它只处理本模块列出的 ownership，不通过便利性绕过上层 policy、下层 abstraction 或 Adapter boundary。

## Data flow

```text
normative JSON Schemas
  -> Capability Contracts
  -> validated contract artifacts
```

## State and errors

实现必须暴露可观察状态，重复操作幂等或返回 `CONFLICT`。未授权/未协商返回标准错误；不得抛出包含秘密、原始命令或用户数据的跨边界错误。

## Security

unknown fields/version fail closed；契约不等于授权。

## Compatibility

DSH/platform/std 特定差异集中在既定 adapter/provider；本模块不得使用版本号或品牌环境变量猜 capability。

## Tests

- Happy path 与所有公开 operation。
- Invalid state、unavailable、unauthorized 和 cleanup。
- 所属 M2 acceptance catalog。
- Security/reliability 边界的 negative tests。

## Milestone exit

实现、测试、文档、tracking、evidence 与 handoff 同时完成；没有 evidence 时状态不得高于 `review`。
