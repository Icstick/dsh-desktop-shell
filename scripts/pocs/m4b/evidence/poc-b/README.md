# M4-B PoC B — 受管 Edge + CDP 验证记录 (2026-08-29)

## 运行

```powershell
cd scripts\pocs\m4b\poc-b
& 'D:\Program Files\nodejs\node.exe' pocb-edge-cdp.mjs
```

零依赖（Node 24 原生 WebSocket/fetch）。耗时约 1.5s（Edge 启动 + 导航 + 截图 + 清理），exit 0，无 stderr。

## 验证结果（POC-M4B.md 矩阵）

| 验证点 | 判定 | 证据 |
|--------|------|------|
| P1 唯一 profile 目录 | PASS | mkdtemp `dsh-pocb-<rand>`（前缀隔离，复用会转发旧实例） |
| P2 动态 debug 端口 | PASS | DevToolsActivePort → 随机端口（如 11771） |
| P3 page target | PASS | /json/list → about:blank + ws:// URL |
| P4 navigate | PASS | readyState=complete（example.com） |
| P5 文本快照 | PASS | title=Example Domain，innerText 长度 129 |
| P6 截图 | PASS | pocb-screenshot.png 25998 字节 |
| 进程树清理 | PASS | taskkill /T /F + 无残留 msedge（全机复查 0 个） |
| profile 目录清理 | PASS | 等待文件锁释放后删除，%TEMP% 零残留 |

## 关键实现事实（M4-C/产品化备忘）

1. 启动参数：`--remote-debugging-port=0 --user-data-dir=<唯一目录> --no-first-run --no-default-browser-check --remote-allow-origins=* --disable-features=msEdgeStartupBoost`（StartupBoost 防被杀后复活）。
2. 端口发现：读 `<user-data-dir>/DevToolsActivePort` 第一行（比解析 stderr 可靠）。
3. user-data-dir 必须每次唯一：相同目录 + Edge 已运行 = 新进程转发旧实例后退出，端口永不出现。
4. CDP 会话：id 匹配 Map 处理响应乱序，忽略无 id 事件（复用 scripts/smoke-native.mjs 模式）。
5. 清理：taskkill /PID <pid> /T /F 杀全树（renderer/gpu/crashpad），等待 ~800ms 释放文件锁后删除 profile 目录。

## 问题修复记录

| # | 问题 | 修复 |
|---|------|------|
| 1 | 截图证据路径相对脚本目录解析，与仓库 `m4b/evidence/` 约定不一致 | 改为 `../evidence/poc-b/`（相对 poc-b 目录上一级） |
| 2 | cleanup() 未删除临时 profile 目录（%TEMP% 泄漏） | cleanup 在 taskkill 后等待文件锁释放并 rmSync（失败重试一次） |
| 3 | 初版 README/截图落在 `poc-b/evidence/` 错位路径 | 已删除错位副本，规范路径为 `m4b/evidence/poc-b/` |
