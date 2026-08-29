---
id: ADR-0015
status: accepted
date: 2026-08-28
owner_role: runtime-owner
---

# ADR-0015: Desktop-Owned Persistent Terminal (Windows ConPTY)

## 背景

M3 需要 Workbench 终端（AC-PTY-001：DSH restart 不终止 Desktop-owned PTY）。终端进程若挂在 DSH process tree 下，DSH 重启会杀死会话；若挂在 Shell 下，窗口关闭即丢失。既有的 terminal-capability schema 定义 create/write/resize/close 与 human_surface/agent_automation 两种 mode。

## 决策

1. PTY 会话由 Desktop-owned：terminal-provider 直接 spawn 用户 shell（Windows ConPTY），进程树挂在本进程（Desktop）下，与 Managed DSH process tree 完全独立。DSH stop/restart/crash 不影响 PTY 存活（AC-PTY-001）。
2. M3 只实现 `human_surface` 模式；`agent_automation` 请求 fail-closed 拒绝（需要 M5 adapter 授权链）。Terminal Surface 与 Automation 分权：Surface 只读/写自己的会话，不经 DSH tool/policy 授权不得执行。
3. 会话标识为 opaque id（Desktop 生成）；输出经 Tauri event 推送到 Shell WebView（只允许 `shell` label 监听）；无 privileged native bridge（沿用 ADR-0004/0011 边界）。
4. resize/write 有界：cols/rows 与单次 write/data 长度设上限；关闭幂等；Desktop 退出时 Drop 清理全部 PTY。
5. PTY 内容不进入 usage、notification、diagnostics 或 tracking（隐私边界）。

## 验证门禁

- ConPTY 创建/IO/resize/关闭有 Rust 测试（Windows）。
- AC-PTY-001 测试：Managed DSH healthy → 创建 PTY → stop/restart DSH → PTY 仍可 IO。
- agent_automation 请求被拒；opaque id 不泄露 pid/路径。
- output event 只发往 shell WebView。

## 受影响模块

- MOD-TERMINAL-PROVIDER（crates/terminal-provider）
- MOD-TERMINAL-UI
- MOD-SHELL-UI / IF-TERMINAL
