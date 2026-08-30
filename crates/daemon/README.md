# dsh-daemon（dsh-desktop-daemon）

独立 daemon 进程（ADR-0019 决策 1）：无 UI、不依赖 tauri，持有共享资源（DSH 进程树、PTY、browser 会话状态、broker 授权权威——M6-C 迁入）并通过**统一外源 API**（local-transport + envelope 服务端，ADR-0019 决策 5）对外提供服务。

## 为什么需要 daemon

M1–M5 的 supervisor 资源都住在 Tauri 进程里：Shell 一重启，DSH 进程树/PTY/browser 会话全部跟着没。M6 按 ADR-0008 的演进路径把资源所有权拆到独立进程——Shell 重启只重建 UI，资源存活（ADR-0008 验证门禁）。M5-B2 的 external-api-example 是「统一外源 API」的参考闭环；本 crate 把它升级为真实服务端，并把示例的静态 GrantPolicy 换成 broker 驱动的授权链（M5-E1）。

## 启动 / 停止

```sh
cargo run -p dsh-daemon            # 前台运行（开发）
# 或直接跑编译产物：
cargo build -p dsh-daemon
target/debug/dsh-desktop-daemon.exe
```

停止：Ctrl+C 或 taskkill（Windows）。daemon 被强杀后留下的 `daemon.lock` 是**陈旧锁**，下次启动时由端口所有权检查自动回收（见下）。

### 单实例（ADR-0019 决策 4，M6-B1 最小形式）

双重检测，全部 fail-closed：

1. **claim 端口所有权**：启动时绑定固定 loopback 端口 **37771** 并持有到进程结束。第二个实例（或任何占用该端口的进程）绑定失败 → 退出码 **3**（already running）。该端口同时是 Shell 的存活探针（TCP connect 成功 = daemon 在）。
2. **启动锁文件**：`<data-dir>/daemon.lock`（内容为 pid）。端口已归我方而锁文件仍存在 = 上次崩溃残留，自动删除重取；其他失败 → 退出码 **4**。

### 退出码

| 码 | 含义 |
|----|------|
| 0 | 正常退出 |
| 1 | 运行时错误（绑定/凭证文件写入失败） |
| 2 | 参数错误（`--help` 查看用法） |
| 3 | 已有 daemon 实例（claim 端口被占） |
| 4 | 锁文件冲突 |

## 凭证（Shell 启动时读取）

daemon 启动时签发一次性 local-transport 凭证（AC-IPC-001：一次性、TTL 内有效、首次握手即消费），写入：

```text
%APPDATA%/dev.dsh.desktop-shell/daemon-credential.json
```

```json
{
  "schemaVersion": 1,
  "daemonVersion": "0.1.0",
  "pid": 1234,
  "claimPort": 37771,
  "port": 52341,
  "credential": { "token": "lt_...", "expiresAt": "2026-08-31T09:30:00.000Z" },
  "issuedAt": "2026-08-31T08:30:00.000Z"
}
```

Shell 启动流程：读文件 → 按 `port` 连接 → 用 `credential.token` 完成握手 → Hello/Agreement 协商 → Invocation。`--data-dir <dir>` 或环境变量 `DSH_DAEMON_DATA_DIR` 可覆盖数据目录（测试/排障用）。

## 能力面

授权链：`Hello` → `Broker::broker_grant_from_negotiation`（grant + 有界 lease，owner = `component-facet`（wire 兼容的 agentId 形式，M6-C1））；`Invocation` → `Broker::enforce_dispatch`（ADR-0014 门禁：grant/owner/generation/scope/有效 lease）→ 能力处理器。协商语义沿用 ADR-0018 决策 1：新激活 supersede 旧激活（generation 变更吊销旧 lease）。

| 能力 | 方法 | 返回 | 状态 |
|------|------|------|------|
| system | ping | `{ pong, echo }` | 可用 |
| daemon | status | 版本/pid/启动时间/uptime/claimPort/port/connections/credentialsIssued/activations + `resources`（terminals 已接真实计数）+ scheduler 统计 | 可用 |
| browser | create / list / status / close | create 返回 BrowserReport；list 返回 `{ browsers }`；status 返回 `{ sessions, count }`；生命周期经 Event `browser.session-created` / `browser.session-closed` 推送 | **真实**（M6-C3：daemon 持有 browser SessionRegistry，状态权威；渲染仍在 Shell） |
| terminal | create / write / resize / close / status | create/resize 返回 TerminalReport；status 返回 `{ sessions, count }`；输出经 Event `terminal.output` 推送 | **真实**（M6-C1：daemon 持有 PTY registry） |
| runtime | status | `{ managedRuntimes: 0, endpoints: [] }` | **占位**（M6-C3） |
| scheduler | wake / cancel | IF-SCHEDULE-WAKE（定时/延迟触发，M6-D） | 可用（M6-D） |

## browser 能力（M6-C3：会话状态权威迁入，渲染留 Shell）

daemon 持有 `SessionRegistry`（crates/browser-provider，M4 状态机；ADR-0019 决策 2 方案 A：**渲染在 Shell、状态权威在 daemon**），envelope 方法为命名空间形式 `browser.create` / `browser.list` / `browser.status` / `browser.close`（M6-B1 占位方法 `list_browsers` 被命名空间形式取代，与 terminal 一致；请求/报告/事件 wire 形状对齐 `specs/browser/*.schema.json`）。

**M6-C3 范围（registry 语义）**：create 注册会话并返回 BrowserReport（opaque `brw-<ms>-<seq>` id）；list/status 暴露**存活**会话（daemon 权威视图，Shell 重启后 restore 用）；close 走状态机并广播 closed 事件。**navigate/snapshot 仍在 Shell 渲染进程内执行**——Shell 渲染事件与 daemon 状态间的同步协议（attach、navigate/snapshot 上报、Shell 重启 handover 重挂）由 M6-C4 定。

**事件回流**：会话建立/关闭 → EventRouter（sessionId → 创建连接定向路由）→ 每连接 writer 线程 → envelope Event（method `browser.session-created` / `browser.session-closed`，payload 为 browser-event 形状，kind 新增 `created`——specs/browser/browser-event.schema.json 已扩展）。事件**不跨会话、不跨连接**。

**会话所有权**：close 仅允许创建该会话的连接（其他连接 → NOT_PROCESS_OWNER，与 terminal 一致）；连接断开不杀会话（状态存活，M6-C4 handover 接管）。

## terminal 能力（M6-C1：PTY 真实迁入）

daemon 持有 PtyRegistry（ConPTY 子进程，`crates/terminal-provider`），envelope 方法为命名空间形式 `terminal.create` / `terminal.write` / `terminal.resize` / `terminal.close` / `terminal.status`（方法 pattern `^[a-z][a-z0-9._-]+$`；请求/报告/事件 wire 形状对齐 `specs/terminal/*.schema.json`）。

**授权（human/agent 分离）**：

- **human 会话**（`mode: human_surface`）：仅经本地 credential——连接已通过握手 + 协商拿到 terminal grant 即视为授权，**不走 broker**（与 M5 terminal.rs 一致）。
- **agent 会话**（`mode: agent_automation`）：create 时携带 agent facts（agentId/activationId/generation/scope），经 `Broker::enforce_dispatch`（ADR-0014/0018 决策 7）门禁；之后的每次变更（write/resize/close）按记录的 binding 再次过门禁（human takeover 吊销 lease → fail-closed，STALE_GENERATION）。

**事件回流**：PTY 输出 → daemon 桥线程 → EventRouter（sessionId → 订阅连接定向路由）→ 每连接 writer 线程 → envelope Event（method `terminal.output`，payload 为 TerminalOutputEvent）。会话建立时创建连接自动订阅；事件**不跨会话、不跨连接**。

**会话所有权**：写/改/关仅允许创建该会话的连接（其他连接 → NOT_PROCESS_OWNER）；连接断开**不杀会话**（资源存活是 daemon 存在的理由，ADR-0008），事件停止流动，M6-C4 的 Shell handover 再接管。

**broker 单属主（M6-C1 决策）**：broker 每能力只允许一个当前属主（ADR-0014）。第二个 participant 协商同一能力时，Agreement 仍在协议层授予该能力（human 路径凭 credential 即可），broker 保持现任属主不变；同一 participant 的新协商照常 supersede（generation 变更，ADR-0018 决策 1）。

协议细节（envelope 帧格式、授权三规则、correlation 校验）与 external-api-example 完全一致；本 crate 的 `envelope.rs` 是从示例逐字移植（wire 契约防漂移），示例 crate 保持独立（客户端示例）。

## 结构

| 路径 | 职责 |
|------|------|
| `src/main.rs` | 入口：参数 → 单实例守卫 → 绑定 → 凭证文件 → serve 循环（每连接一线程） |
| `src/server.rs` | envelope 服务端：serve_connection/handle_envelope/SessionState + broker 授权链（M5-E1 桥）+ 每连接事件 writer 线程 + terminal 事件桥 |
| `src/envelope.rs` | envelope wire 类型 + 帧校验（从 external-api-example 逐字移植） |
| `src/browser.rs` | BrowserHost（SessionRegistry + 连接所有权）+ browser.* 方法处理 + 生命周期事件 + 错误映射 |
| `src/terminal.rs` | TerminalHost（PtyRegistry + agent binding + 连接所有权）+ terminal.* 方法处理 + 错误映射 |
| `src/events.rs` | EventRouter/EventSubscriber：sessionId → 连接订阅路由（M6-B1 TODO⑤ 落地） |
| `src/scheduler.rs` | IF-SCHEDULE-WAKE TimerHost（wake/cancel + 统计，M6-D） |
| `src/credential.rs` | 凭证文件格式（读写、原子写入）+ 数据目录解析 |
| `src/singleton.rs` | 单实例守卫（claim 端口 + 锁文件） |
| `tests/` | daemon_integration（10）+ scheduler_wake + split_brain + terminal_integration（5，真实 PTY 经 envelope 往返）+ browser_integration（5，会话状态权威经 envelope 往返）+ 单元测试 |

## 门禁

```sh
cargo fmt -p dsh-daemon --check
cargo clippy -p dsh-daemon --all-targets -- -D warnings
cargo test -p dsh-daemon
```

## M6-C 迁移计划（TODO 清单）

1. **固定端口 envelope bind**：`dsh-local-transport` 目前只绑定随机 loopback 端口（`LocalServer::bind` 硬编码 port 0）。M6-B1 用「claim 端口 37771 + 凭证文件携带真实 port」绕过；M6-C 给 local-transport 增加 `bind_addr(addr, limits)` 变体后，envelope 服务端直接落 37771。
2. **broker provider 注册 / `Broker::dispatch`**：P0 provider handler 是无错误的 `Fn(&Invocation) -> InvocationResult`，而 envelope 协议需要对未知方法返回错误 Result，因此 M6-B1 用 `enforce_dispatch` 做门禁、daemon 分派表执行；资源迁移（terminal/browser/runtime provider）时补注册与 fallible handler 设计。
3. **资源迁移**：PTY registry **已完成（M6-C1）**（daemon 真实持有 PTY，`daemon.status` 的 `resources.terminals` 接真实计数）；browser 会话状态 **已完成（M6-C3）**（daemon 持有 SessionRegistry，`resources.browsers` 接真实计数；渲染仍在 Shell）；managed_runtime DSH 进程树待 M6-C2。
4. **连接级 lease 吊销**：连接断开时以 `LeaseRevocationReason::Disconnect` 吊销该会话 lease（当前由 TTL 兜底）。
5. **Event 订阅/路由**：**terminal.output 已完成（M6-C1）**、**browser.session-created/closed 已完成（M6-C3）**（EventRouter：会话建立时订阅、按 sessionId 定向推送、连接断开清理订阅）；runtime 事件转发待 M6-C2。
6. **Shell 侧接线**：Shell 启动 → 探测 37771 / 读凭证文件 → 连接 → 资源接管确认（DSH/PTY 已在 daemon 持有则不重启）。

## 已知边界（M6-B/C1）

- 停止 = 进程终止（Ctrl+C/taskkill）；`--shutdown` IPC 与优雅停机协议留 M6-D。
- 连接断开暂不吊销该连接的 broker lease（TTL 兜底）；连接级 lease 吊销待 M6-C。
- 固定端口 envelope bind 待 local-transport `bind_addr` 变体（M6-C）；当前 claim 端口 37771 + 凭证文件携带真实 port。
- 连接断开的会话成为「孤儿」（PTY 存活、事件停流、其他连接不可变更）；Shell handover 接管协议待 M6-C4。
- `terminal.close` 带 provider 拆除 workaround：`PtyRegistry::close` 在输出 reader 持有 pending 同步 ReadFile 且无数据在途时会死锁（Windows CloseHandle 等待 pending I/O）；daemon 先做一次同几何 resize 强制 ConPTY flush（已验证稳定）。根因修复归 `crates/terminal-provider`（先关子进程 stdin 端）。
- 每次协商独立激活（ADR-0018 决策 1）；同一 participant 的新协商 supersede 旧激活（STALE_GENERATION）。
