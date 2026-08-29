# Acceptance Catalog

验收项使用稳定 ID 并链接 milestone/work item。

- AC-OWN-001：Attached restart 必须返回 NOT_PROCESS_OWNER。
- AC-OWN-002：Attached health 只执行 bounded fixed-loopback reachability probe；不得发送 application payload、扫描端口、触发 lifecycle mutation，且 reachability 不得提升 identity/process ownership。
- AC-RUN-001：Managed restart 更新 generation 并恢复 DSH Surface。
- AC-RUN-002：强制停止后 process group 和 endpoint 都释放。
- AC-RUN-003：Managed current-DSH launch 强制 loopback 与 `--no-open`；auto port 使用 `--port 0`，输出 endpoint 经 process identity、host、port 和 readiness 验证后才发布。
- AC-RUN-004：source checkout 缺少已构建产物时返回 UNAVAILABLE，Desktop 不执行 install、build 或 bootstrap。
- AC-RUN-005：Managed endpoint 只有在 current retained process-tree handle、generation/instance、owned `dsh web:` exact loopback candidate 与 bounded TCP readiness 同时成立时发布；只允许 legacy credential-free root 或 backend-owned exact token root，公开 report 必须删除 bootstrap credential；stale generation、foreign output、caller/畸形 credential、另一端口/host、超时或 early exit 必须拒绝并清理 process tree。
- AC-RUN-006：Windows user-prebuilt repository 只能通过 persisted absolute `nodePath + harness.path` 结构化启动；不得调用 shell/package manager/install/build，非 Managed Repository 的 Node override 必须拒绝。
- AC-REC-001：恢复预算耗尽进入 Safe Stop。
- AC-WEB-001：DSH WebView 无 privileged Tauri capability。
- AC-WEB-002：hostile browser Origin、DNS rebinding 与无 credential loopback request 被拒绝。
- AC-WEB-003：每个 custom Tauri command 都登记到 AppManifest 并映射最小 permission；DSH/Browser WebView 对全部 command 的负向调用均被拒绝。
- AC-WEB-004：DSH Surface policy 只从 persisted fixed `http://127.0.0.1:<port>` 派生；exact same-origin main-frame navigation 可通过，另一 loopback origin、credentialed/non-HTTP URL、popup、download、permission 与无 user gesture external navigation 必须拒绝；external delegate 不得自动打开。
- AC-WEB-005：native DSH Surface 只接受 Supervisor 当前 retained process tree、current generation 与 verified endpoint 同时成立的 Managed binding；caller-supplied endpoint/origin/URL/label、Attached、stale/unready/unowned binding 全部拒绝。
- AC-WEB-006：Windows child WebView 在 remote document load 前安装全 `PermissionRequested` deny；只允许 verified exact-origin navigation，cross-origin、popup/new-window、download、permission、initialization script 与 page eval 均拒绝。
- AC-WEB-007：macOS、Linux 和 other 在没有等价 permission-deny 证据时返回 `unsupported_platform`、保持 unmounted 且不创建 WebView。
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

## M3 Workbench acceptance additions

- AC-NOT-001：Notification content policy 强制：title_only/redacted_summary 不得携带 body；explicit_body 才允许 body。
- AC-NOT-002：dedupeKey 在 TTL 内折叠重复通知；审计记录可复查（id/event/title/policy/时间/source，无秘密）。
- AC-USG-001：usage snapshot 可审计（来源/周期/token 估算/是否 estimate），且绝不包含终端或通知内容。
- AC-USG-002：usage 记录本地优先，无网络外发。
- AC-TERM-001：agent_automation 终端模式在 M5 adapter 授权链落地前 fail-closed 拒绝（human_surface 仅限）。
- AC-TERM-002：PTY 会话 id 为 opaque（不泄露 pid/路径）；输出事件只发往 shell WebView。

## M1 native Surface evidence state

- `AC-WEB-005`：automated implementation evidence passed。Rust binding tests、request Schema/negative fixture 与 frontend tests 证明 caller 不能提交 endpoint/origin/URL/label，只有 verified Managed generation 会触发 mount。
- `AC-WEB-006`：source/static/unit gates passed，real WebView2 smoke pending。Windows implementation 在 remote navigation 前安装 permission/autofill deny，并拒绝 cross-origin/new-window/download；仍需真实 DSH 页面验证 permission、redirect、popup、download、load failure 和 reload/unmount。
- `AC-WEB-007`：code path/unit evidence passed，target-platform execution pending。非 Windows compile/runtime matrix 尚未在 macOS/Linux host 执行，因此只记录预期 fail-closed 行为，不记录平台通过。

## M1 authenticated bootstrap evidence state

- `AC-RUN-005/006`：2026-08-28 real user-owned DSH source checkout 首次 smoke 证明当前 output 使用 authenticated root，旧 credential-free parser按设计 fail closed。ADR-0012、Schema/fixture 与实现验证完成前不得发布 endpoint或宣称兼容。
- `AC-WEB-006`：authenticated bootstrap 必须由 native backend直接导航，token exchange 后只允许 same-origin clean-root redirect；Runtime/Surface report、Shell IPC、日志、诊断与 tracking 均不得出现 token/query。
