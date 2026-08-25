# Test Strategy

## Unit

State machine、environment validation、path resolution、reload policy、negotiation、Schema validator、redaction。

## Contract

Protocol envelope、capability operations、Legacy/std adapters、fake DSH、transport carriers、error codes。

## Integration

Managed start/stop/restart、Attached no-kill、Web reconnect、Terminal/Browser survival、Usage continuity。

## E2E

First-run、Environment switching、crash recovery、diagnostics、unavailable/degraded UX、real WebViews。

## Chaos

见 [CHAOS.md](CHAOS.md)。

## Security

Unauthorized IPC、path/symlink、PID spoof、WebView bridge、Agent permission bypass、secret leakage、Browser profile isolation。

## Release Evidence

每个支持平台保留版本、runner/设备、测试清单、日志 hash、失败豁免和 reviewer。不能用单一 Linux CI 推断其他平台。
