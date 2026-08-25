# Acceptance Catalog

验收项使用稳定 ID 并链接 milestone/work item。

- AC-OWN-001：Attached restart 必须返回 NOT_PROCESS_OWNER。
- AC-RUN-001：Managed restart 更新 generation 并恢复 DSH Surface。
- AC-RUN-002：强制停止后 process group 和 endpoint 都释放。
- AC-REC-001：恢复预算耗尽进入 Safe Stop。
- AC-WEB-001：DSH WebView 无 privileged Tauri capability。
- AC-WEB-002：hostile browser Origin、DNS rebinding 与无 credential loopback request 被拒绝。
- AC-IPC-001：invalid/replay/stale credential/message 被拒绝。
- AC-IPC-002：oversized/slow client 受 frame、deadline、concurrency 限制且可清理。
- AC-CMD-001：Harness executable/argv 不经 shell parsing，shell metachar 不产生额外进程。
- AC-PATH-001：symlink/TOCTOU 不能逃逸已授权 executable、cwd、workspace 或 download scope。
- AC-LEASE-001：disconnect、unload、expiry、human takeover 与 generation change 撤销 lease。
- AC-PTY-001：DSH restart 不终止 Desktop-owned PTY。
- AC-BRW-001：Browser page 无 Desktop IPC。
- AC-BRW-002：Human takeover 撤销 Agent mutation lease。
- AC-COMP-001：Adapter 不兼容时 baseline 仍可用。
- AC-LOG-001：诊断 golden corpus 不泄漏 secret。
