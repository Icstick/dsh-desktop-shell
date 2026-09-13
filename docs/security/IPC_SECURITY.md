# Local IPC Security

## Native Carrier

- Windows Named Pipe 使用当前用户 ACL 与 instance-specific name（daemon 生成，带 per-process nonce）。
- Unix Domain Socket 放在 daemon 数据目录（该目录 0700）内，socket 文件 0600，同样带 per-process nonce；
  bind 只收紧**自己创建**的目录，不会改动已存在目录的权限，也不会删除同路径的非 socket 文件。
- Loopback fallback 使用 ephemeral bearer credential，仅绑定 127.0.0.1。它是**显式降级载体**：内核对端
  身份不可得，因此永远不能用于控制面判定（ADR-0022 决策 3）。
- Loopback server 不信任 Host/Origin；拒绝不允许的 browser Origin、缺失 credential 与 WebView/browser preflight，防止 DNS rebinding 与 loopback CSRF。

## Identity

- 每次 start 产生 instance ID、generation、launch identity 和 credential。Token 证明 process-level membership，不证明特定 DSH plugin 身份。
- **控制面（`ShellControl`）额外要求内核提供的对端身份**（ADR-0021 决策 2/3 + ADR-0022）：只有当连接来自
  内核身份载体（Windows named pipe / Unix domain socket），且内核报告的进程镜像路径与期望的 Shell 可执行
  路径匹配时，`dsh-desktop-shell|shell` 声明才被判为 `ShellControl`；否则一律降级为 `Participant`——
  身份缺失、镜像无法解析、路径不匹配**全部 fail-closed**。TCP 连接没有对端身份，因此永不能获得控制面。
- 判定在 Hello 时计算一次（服务端事实，不采信对端自报），并与被接受的连接一起写入审计日志
  （`connection … identity=…` / `activation … authority=…`，ADR-0021 决策 4）。
- 载体端点随凭证文件发布（schema v3：Windows `pipeName` / Unix `socketPath`），Shell 优先使用它；
  连接失败会记录原因并降级到 TCP（此时控制面不可得）。

## Required Negative Tests

- missing/invalid/replayed token。
- other-user connection。
- stale generation。
- malformed framing/schema。
- oversized payload / slow client。
- endpoint reuse/hijack。
- disconnect during invocation。
- attached process spoof。
- oversized frame、slowloris、并发/队列上限与 cancellation deadline。
