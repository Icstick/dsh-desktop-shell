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
2. Terminal Surface 与 Automation 分权：Surface 只读/写自己的会话，不经 DSH tool/policy 授权不得执行。M3 只实现 `human_surface`，`agent_automation` fail-closed；M5-E2（ADR-0018 决策 7 授权链落地）反转：`agent_automation` create 经 capability broker 校验 grant + lease（无授权 UNAUTHORIZED），agent 会话 mutation 经 broker dispatch 门禁，human takeover 撤销 lease 后 agent 会话拒绝；human_surface 会话始终不经 broker。
3. 会话标识为 opaque id（Desktop 生成）；输出经 Tauri event 推送到 Shell WebView（只允许 `shell` label 监听）；无 privileged native bridge（沿用 ADR-0004/0011 边界）。
4. resize/write 有界：cols/rows 与单次 write/data 长度设上限；关闭幂等；Desktop 退出时 Drop 清理全部 PTY。
5. PTY 内容不进入 usage、notification、diagnostics 或 tracking（隐私边界）。

## 验证门禁

- ConPTY 创建/IO/resize/关闭有 Rust 测试（Windows）。
- AC-PTY-001 测试：Managed DSH healthy → 创建 PTY → stop/restart DSH → PTY 仍可 IO。
- M3 起 agent_automation 请求被拒；M5-E2 起无授权 agent_automation create 被拒（UNAUTHORIZED）、agent 会话 mutation 经 dispatch 门禁、takeover 后拒绝；opaque id 不泄露 pid/路径。
- output event 只发往 shell WebView。

## 受影响模块

- MOD-TERMINAL-PROVIDER（crates/terminal-provider）
- MOD-TERMINAL-UI
- MOD-SHELL-UI / IF-TERMINAL

## M8 增补：Unix 平台扩展（2026-08-31）

M8-A 将终端会话实现拆为平台分派（platform.rs → platform_unix.rs / platform_windows.rs），
Windows ConPTY 语义保持不变，Unix 新增 openpty 路径。本增补记录 Unix 侧的决策与取舍
（完整排查过程见 docs/investigations/m8-ci-terminal-integration.md）：

1. **进程模型**：fork + setsid + TIOCSCTTY + dup2 stdio + execvp（fork 后仅
   async-signal-safe 调用、无堆分配）；$SHELL 缺省 /bin/sh。
2. **reader 永不阻塞**（M6-C1 死锁结论的 Unix 对应）：poll(2) 100ms tick +
   stop flag；master 设 **O_NONBLOCK**——macOS poll 假阳性 POLLIN（kqueue 模拟）
   在数据耗尽后仍报可读，阻塞 read 会永久挂起并卡死 close 的 join；EAGAIN
   有界重试（5ms 退避）而非忙循环。
3. **write 背压**：master 非阻塞导致子进程不读 stdin 时 write 返回 EAGAIN——
   有界重试（5s deadline，EINTR 不计时）对齐 Windows 阻塞排队语义，超时才失败。
4. **teardown 顺序**：stop → terminate_io（SIGTERM → 500ms WNOHANG → SIGKILL →
   200ms WNOHANG 有界 reap，超时交 init；交互 shell 忽略 SIGTERM 是设计行为）
   → join reader → close_read（关 master；过早关闭会 fd 重用竞态）。
5. **shell 契约跨平台化**：schema 枚举 [default,cmd,powershell,pwsh,sh,bash,zsh]；
   Windows 接受 default/cmd/powershell/pwsh，Unix 接受 default/sh/bash/zsh/pwsh，
   各自拒绝对方的平台值（InvalidShell）。
6. **geometry/CLI 语义**：resize 吞 ioctl 失败返回 Ok（与 Windows Err 的分歧为
   已知取舍，best-effort 对齐）；子进程 dup2/chdir/setsid 返回值 best-effort。
