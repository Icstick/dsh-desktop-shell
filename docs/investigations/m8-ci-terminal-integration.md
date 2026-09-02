# M8-B CI 排查：terminal_integration 在 macOS/Ubuntu 失败（Windows 通过）

> 2026-08-30 ~ 2026-08-31。分支 codex/wi-m8-stable。状态：**已解决**（2026-08-31）。

## 症状

- ubuntu/macos 的 `cargo test -p dsh-daemon --test terminal_integration` 稳定失败：
  agent_sessions_gate_through_the_broker 等（每测试约 30s 后失败）。
- Windows CI + 本机 Windows 全部通过。
- 失败模式：invoke 时 `Result: connection closed`（common/mod.rs:345 recv_expect）。

## 根因链（8 个真实问题，按发现顺序）

1. **异步 Event 插入协商**：daemon writer 线程独立推送 Event，可能先于 Agreement
   到达（PTY 输出时序平台相关）。TestClient::negotiate 与真实 Shell 客户端
   connect_transport 都改为缓冲 Event 直到 Agreement（产品级隐患，不只是测试）。
2. **Unix fd 重用竞态**：reader join 前关闭 master → fd 号被复用 → 旧 reader poll
   读到新 PTY 数据（事件错乱）。改为 terminate_io 只杀子进程，master 在
   close_read（join 后）关闭。
3. **FD_CLOEXEC 缺失**：master/slave 泄漏给 exec 的 shell → EOF 永不达。
4. **交互 shell 忽略 SIGTERM → waitpid 永久阻塞**（bash/dash/zsh 设计：terminal
   teardown 不杀前台 job）。修复：SIGTERM → 500ms WNOHANG 有界等待 → SIGKILL →
   阻塞 waitpid 保证 reap。（本轮主根因之一，ubuntu/macOS 通用）
5. **scheduler 测试固定日期过期**：valid_wake 的 requestedAt 用固定 2026-08-31
   时间（时间炸弹，当天恰好过期，macOS 暴露）。
6. **测试 fixture 平台路径**：4 处用 Windows 绝对路径（`C:/...`），unix 上
   `PathBuf::is_absolute()` 返回 false（commands/diagnostics/managed-runtime）。
7. **diagnostics catalog 父目录 root 所有**：/tmp 归 root，restrict_directory
   chmod 0700 失败。改为自建子目录（temp_dir 下的 dsh-diag-...）。
8. **geometry 测试把 "bash" 当不支持 shell**：unix 上 bash 合法，改 "not-a-shell"。

## macOS 专有根因（两块拼图）

### 拼图 1：poll(2) 假阳性 POLLIN + 阻塞 read → reader 永久挂起

**poll(2) 假阳性 POLLIN + 阻塞 read → reader 永久挂起 → close 卡死。**

- 10 次 `srv recv`（间隔 ~20ms，test 同步 invoke 节奏）→ 3c~3f 前半全部通过
  （agent session 的 echo 输出正常到达，wait_for_output 立即返回）。
- 17.245 最后一次 `pty poll ready=1 revents=0x1`（POLLIN）后 **reader 再无日志**。
- 之后持续的 ready=0 poll 日志来自 **agent session 的 reader**（未 close）。
- 第 10 个消息是 close human（17.221）→ terminate_io + **join reader** →
  human session 的 reader 卡在 17.245 那次 read()（macOS kqueue 模拟 poll 在
  数据已耗尽后仍报一次 POLLIN，阻塞 read 永远挂起）→ close 的 Result 永不发出。
- server worker 空闲 30s（read_deadline）→ 主动关连接（EndReason::Timeout，
  47.195 = 最后消息 + 30.0s 精确吻合）→ test 的 recv_expect panic
  `Result: connection closed`。

Ubuntu 的 poll(2) 语义正确（POLLIN 时数据必然可读），所以不触发——平台差异。

### 修复（commit 7316d55）

platform_unix.rs spawn_reader：
- master fd 设 **O_NONBLOCK**（fcntl F_GETFL/F_SETFL）。
- read 返回 **EAGAIN/WouldBlock** → sleep 5ms → continue（假 POLLIN 降级为
  可重试，stop flag 保持响应，join 必然返回）。
- read < 0 其他错误 / read == 0（EOF）→ break。

### 拼图 2：SIGKILL 后的阻塞 waitpid 在 macOS 上不返回（flaky ~50%）

O_NONBLOCK 修复后仍 flaky（两轮里一轮挂）。focused job 的
DSH_PTY_DEBUG 时间线（commit 37a2392）定位：

- reader 正常退出（"reader exit: stop"）→ 拼图 1 修复有效；
- terminate_io 走 SIGKILL fallback（zsh 忽略 SIGTERM，500ms 后升级）；
- **阻塞 `waitpid(pid, &status, 0)` 永不返回** → close 的 Result 不发 →
  30s 空闲超时 → 连接关闭（与第一轮完全同款的失败表象）。

修复（commit ac7d705）：**所有 waitpid 一律 WNOHANG + 有界等待**——
SIGKILL 后 200ms 循环 reap，超时放弃（zombie 由 init 回收）。daemon 主
循环在任何平台、任何 waitpid 行为下都不会阻塞。日志在确认后移除
（commit b23c247，DSH_PTY_DEBUG 门控痕迹全清）。

### 附带修复（live-qa 首跑暴露）

- live-qa 需显式构建 daemon bin（tauri build 只编 app；`cargo build -p
  dsh-daemon --bin dsh-desktop-daemon`）。
- live-m7-qa.mjs 硬编码本机路径 `D:/DSH_workspace/...` → 改为从脚本
  位置推导 ROOT（与 live-daemon-qa.mjs 一致）。

## 排查方法回顾（教训）

- **对比日志时序**：srv recv 间隔 ~20ms 均匀 = test 同步 invoke；第 10 个消息
  后 30s 精确静默 = server 空闲超时（read_deadline），不是 test 的问题。
- **多 reader 并存**：close 后的 poll 日志可能来自其他 session 的 reader，
  用"谁还在打日志"区分卡住的是哪个。
- **Windows 时序掩盖**：Windows 上所有竞态都因为平台调度差异不触发，
  必须靠 unix CI + 日志探针迭代（每轮 ~3 分钟）。
- 调试日志用 eprintln + --nocapture 直接看；问题解决后必须清理。

## 已完成的 M8 部分（不受影响）

- M8-A: terminal-provider 平台拆分（Windows 6/6 + unix 编译过）
- M8-A: named mutex 决策关闭、browser 降级策略（ADR）
- M8-B: CI 矩阵（三平台全绿目标，此文档解决最后一块）
- M8-C/D: bundle 配置、自签指南、deny.toml、sbom.mjs
