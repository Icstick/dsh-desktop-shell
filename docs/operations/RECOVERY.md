# Recovery

## Managed Core

异常退出 -> 记录 cause -> backoff -> 在 budget 内 restart -> health -> reconnect。预算耗尽进入 Safe Stop。用户可查看日志、修改 Environment、手动重试或切换 Attached。

## Broken Profile

Desktop 不自动修 Profile。仍允许进入 Settings、Diagnostics 和选择其他 Environment。可提供打开 DSH 官方工具/文档的导航。

## Provider

PTY child exit 只影响对应 session；Browser provider crash 尝试恢复 provider，不重启 DSH；Shell crash 在 P2 由 daemon 保持资源。

## Data

Desktop state 使用原子写/备份策略；不把 recovery metadata 写入 DSH_HOME。
