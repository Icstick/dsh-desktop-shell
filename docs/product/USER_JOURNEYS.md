# User Journeys

## 首次配置

```text
Install Shell
  -> First-run Setup
  -> select/discover Harness
  -> select DSH_HOME and Profile
  -> choose Managed or Attached
  -> validate launch/endpoint
  -> save Desktop-owned Environment reference
  -> open DSH Surface
```

验证失败时必须给出失败阶段、执行方式、路径、建议修复和可脱敏复制的诊断；不得自动安装 Node、修复 Profile 或修改 `DSH_HOME`。

## Managed 日常启动

Desktop 解析 Environment，生成 resolved launch plan，创建 process identity 与 generation，启动 DSH，等待 readiness，加载原版 Web UI。用户 stop/restart 时先 graceful，超时后才处理 process group，并确认 endpoint 释放。

## Attached 连接

用户提供 endpoint 或选择已知实例。Desktop 只 probe 并标记 external ownership。所有 stop/restart/kill 请求返回 `NOT_PROCESS_OWNER`，除非未来完成显式 ownership handover。

## 插件升级要求重启

DSH Adapter 经已协商 Runtime capability 请求 `core_restart`；Supervisor 在旧进程退出前返回 Accepted，保存 route hint，完成 stop/start/health，更新 generation，再让 Web Surface reconnect。Terminal/Browser 不随 DSH 退出。

## Adapter 不兼容

Adapter negotiation 失败或版本未知时：

- Web Surface、health、Managed lifecycle 继续可用。
- Usage/Notification/Agent Browser 等增强能力显示 unavailable/degraded。
- Diagnostics 说明 adapter、DSH 版本与失败原因。
- 不使用 DOM 或日志猜测补偿。

## Crash loop

在恢复预算内自动 backoff/retry；预算耗尽进入 Safe Stop，保留 Shell、设置与日志入口，要求用户显式干预，不无限重启。
