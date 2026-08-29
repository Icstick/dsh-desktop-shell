# Environment Settings Development Contract

## Boundary

配置和验证用户已有 Harness、DSH_HOME、Profile、endpoint 与 ownership。 它只处理本模块列出的 ownership，不通过便利性绕过上层 policy、下层 abstraction 或 Adapter boundary。

## Data flow

```text
DshEnvironment schema + discovery candidates
  -> Environment Settings
  -> backend validation
  -> explicit AppData-owned catalog save
  -> active Environment restored on next Shell bootstrap
```

## State and errors

实现必须暴露可观察状态，重复操作幂等或返回 `CONFLICT`。未授权/未协商返回标准错误；不得抛出包含秘密、原始命令或用户数据的跨边界错误。

## Security

路径、命令与秘密分离；Attached 不提供 lifecycle mutation UI，并明确提示只有固定 loopback port 才可执行 health probe。Managed Web launch 的 `--host`、`--port`、`--no-open` 为 Supervisor-owned reserved args，UI 不允许用户通过 extra args 覆盖。Source checkout 缺少已构建产物时只报告 `requires_recipe`，不提供安装或构建按钮。Discovery 只检查文件系统 metadata；不得调用 candidate、shell、npm 或 pnpm。Catalog 只写平台 AppData/Application Support，不得写 DSH_HOME/Profile。

## Compatibility

DSH/platform/std 特定差异集中在既定 adapter/provider；本模块不得使用版本号或品牌环境变量猜 capability。Environment 预览必须显示最终 executable、cwd、profile、ownership、endpoint policy 与经过脱敏的 argv 分类，但不显示 credential。

## Tests

- Happy path 与所有公开 operation。
- Invalid state、unavailable、unauthorized 和 cleanup。
- 所属 M1 acceptance catalog。
- Security/reliability 边界的 negative tests。
- Reserved arg collision、Managed `--no-open`、auto port 与 missing prebuilt source checkout。
- Catalog revision、active selection、backup、corrupt-data refusal 与 DSH_HOME non-mutation。
- Explicit/DSH_PATH/PATH discovery、deduplication、request bounds 与 non-execution negative gate。

## Milestone exit

实现、测试、文档、tracking、evidence 与 handoff 同时完成；没有 evidence 时状态不得高于 `review`。
