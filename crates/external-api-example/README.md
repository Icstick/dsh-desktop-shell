# dsh-external-api-example

统一外源 API 参考闭环（ADR-0018 决策 5，PLAN-M5 "统一外源 API interface"）：
**local-transport 载体 + envelope 契约 + 最小授权** 的可运行参考实现。

本 crate 是独立 bin crate（lib + bin），**不依赖 tauri、不依赖 desktop shell**，
作为"外部工具如何调用 Desktop 能力"的可运行规范。默认策略只授予
`system.ping`；`browser.list_browsers` 是静态分派示例（策略授予后即可用）。

## 结构

| 路径 | 职责 |
|------|------|
| `src/envelope.rs` | envelope wire 类型 + 帧校验（TS `validate.ts` 语义移植：protocol const、id 8..=128、按 kind 的必填/禁用字段、Result payload/error oneOf、method 与坐标 pattern、RFC3339 时间戳、`additionalProperties:false`） |
| `src/server.rs` | local-transport server 收 envelope 帧 → 校验 → Hello 协商（granted/unavailable）→ Invocation 分派 → Result 回发（error.correlationId 恒等于被应答 Invocation 的 id） |
| `src/client.rs` | 协商（Hello→Agreement）→ Invocation → Result；Result 关联校验（replyTo 与 error.correlationId 必须匹配 Invocation id，不匹配拒绝） |
| `src/catalog.rs` | 示例能力坐标（system.ping / browser.list_browsers） |
| `src/main.rs` | 自包含 demo：起 server、协商、ping 成功 + list_browsers 被拒 |
| `tests/closed_loop.rs` | 7 个集成测试，全部走真实 local-transport 线路 |

## 授权模型（最小授权，fail-closed）

| 场景 | 结果 |
|------|------|
| 无 Agreement 直接 Invocation（缺 `participant.activationId` 或未协商） | Result error `UNAUTHORIZED` |
| 已协商但能力不在 `granted` 中（如默认策略下的 `browser.list_browsers`） | Result error `UNAUTHORIZED` |
| Invocation 的 `replyTo`/error.`correlationId` 与待应答 Invocation 不符 | 客户端拒绝（`ClientError::CorrelationMismatch`） |
| 帧校验失败（协议版本、未知字段、缺必填、generation 为负等） | Result error `MALFORMED_MESSAGE`（能关联则回发） |

每次激活独立协商（ADR-0018 决策 1：不缓存 Agreement）；未授予的能力进
`unavailable[].reason = policy_denied`（降级路径）。

## 外部工具接入步骤

1. **拿凭据**：Desktop 侧（broker/transport 所有者）签发一次性凭据，TTL 内有效，
   一次握手即消费（AC-IPC-001）。本示例：`server.issue_credential(ttl)`。
2. **连接**：`LocalClient::connect(addr, &credential, &limits)`，完成
   local-transport 握手（帧长上限、超时、并发上限由 `Limits` 控制，AC-IPC-002）。
3. **协商**：发 `Hello`（`payload.supports` = 想使用的能力列表，
   `instanceId` 标识调用方实例），收 `Agreement`：`granted` = 被授予的子集，
   `unavailable` = 被拒能力及原因；记下 `payload.activationId`。
4. **调用**：发 `Invocation`（`participant.activationId` = 协商所得，
   `capability`/`method`/`payload`），收 `Result`。
5. **校验**：`Result.replyTo` 必须等于 Invocation id；错误分支
   `error.correlationId` 也必须等于 Invocation id；不匹配即拒绝。
6. **Event**：异步事件（`kind: Event`）不带 error，可随时到达，按
   `capability.method` 分派。

### 线格式示例

```json
// Hello（客户端 → 服务端）
{ "protocol": "interop.dsh-desktop.local/v1alpha1", "id": "msg-...", "kind": "Hello",
  "participant": { "component": "external-tool", "facet": "example-client" },
  "timestamp": "2026-08-31T09:30:00.000Z", "generation": 0,
  "payload": { "instanceId": "ext-tool-...", "supports": [
    { "apiVersion": "system.dsh-desktop.local/v1alpha1", "kind": "System" },
    { "apiVersion": "browser.dsh-desktop.local/v1alpha1", "kind": "Browser" } ],
    "requires": [] } }

// Agreement（服务端 → 客户端，granted ⊆ Hello.supports）
{ "protocol": "interop.dsh-desktop.local/v1alpha1", "id": "msg-...", "kind": "Agreement",
  "participant": { "component": "dsh-desktop-shell", "facet": "external-api-example",
    "activationId": "act-..." },
  "timestamp": "...", "generation": 0, "replyTo": "msg-...",
  "payload": { "activationId": "act-...",
    "granted": [{ "apiVersion": "system.dsh-desktop.local/v1alpha1", "kind": "System" }],
    "unavailable": [{ "coordinate": { "apiVersion": "browser.dsh-desktop.local/v1alpha1",
      "kind": "Browser" }, "reason": "policy_denied" }] } }

// Invocation（客户端 → 服务端）
{ "protocol": "interop.dsh-desktop.local/v1alpha1", "id": "msg-...", "kind": "Invocation",
  "participant": { "component": "external-tool", "facet": "example-client",
    "activationId": "act-..." },
  "timestamp": "...", "generation": 1,
  "capability": { "apiVersion": "system.dsh-desktop.local/v1alpha1", "kind": "System" },
  "method": "ping", "payload": { "message": "hello" } }

// Result（服务端 → 客户端）
{ "protocol": "interop.dsh-desktop.local/v1alpha1", "id": "msg-...", "kind": "Result",
  "participant": { "component": "dsh-desktop-shell", "facet": "external-api-example" },
  "timestamp": "...", "generation": 1, "replyTo": "msg-...",
  "capability": { "apiVersion": "system.dsh-desktop.local/v1alpha1", "kind": "System" },
  "method": "ping", "payload": { "pong": true, "echo": { "message": "hello" } } }
```

## 与 TS capability-contracts 的关系

- **契约源**：`packages/capability-contracts`（TS）是 wire/shape 层的权威实现
  （类型 + `validateEnvelope` 帧校验 + 协商状态机 + `semantics.ts` 跨消息规则），
  并被 22 个 `specs/protocol/fixtures` 交叉验证。
- **本 crate 的角色**：Rust 侧消费端参考。TS 负责**生成/校验信封**（外部工具可用
  capability-contracts 构造并预校验 Hello/Invocation），Rust 侧（本 server）负责
  **消费信封**（解析、帧校验、协商、授权分派）。两侧对同一份
  `envelope.schema.json` 实现同一语义，是本闭环的"TS 生成、Rust 消费"分工。
- **语义对应**：
  | TS（capability-contracts） | Rust（本 crate） |
  |---|---|
  | `validateEnvelope` | `envelope::validate_envelope` |
  | `NegotiationSession`（proposed→agreed→active） | `server::handle_hello` + `SessionState` |
  | `semantics.ts` correlation-match / result-target | `client::verify_result` |
  | `semantics.ts` invocation-granted | `server::handle_invocation` 授权检查 |
  | `coordinatesEqual` | `ProtocolCoordinate` 的 `PartialEq` |
- **漂移防护**：TS 侧测试直接交叉验证 22 个 fixtures；Rust 侧单元测试镜像
  fixture 形状（Hello/Agreement 正例、负 generation、未知字段、oneOf 等）。
  已知缺口：Rust 侧没有自动化 fixture 全量比对（M6 项，见下）。

## 门禁

```sh
cargo fmt -p dsh-external-api-example --check
cargo clippy -p dsh-external-api-example --all-targets -- -D warnings
cargo test -p dsh-external-api-example
cargo run -p dsh-external-api-example   # demo
```

## M6 真实服务端接线缺口

1. **broker 接入**：本示例的 `GrantPolicy` 是静态策略；真实服务端应由
   Capability Broker（ADR-0014）的 grant/lease 驱动，激活写入 broker 登记
   （ADR-0018 决策 1），并支持 leaseConstraints/lease 到期。
2. **fixture 全量交叉验证**：把 22 个 `specs/protocol/fixtures` 引入 Rust 测试
   （`include_str!` + 正负断言），与 TS 侧形成双端防漂移。
3. **Event 通道**：示例只校验 Event 帧；真实服务端需要能力事件订阅/路由
   （如 terminal.output、browser 事件）。
4. **generation 单调性**：示例按连接维护 generation；真实服务端应按
   participant 流（component|facet|activationId）维护并拒绝回退
   （semantics.ts generation-monotonic）。
5. **多连接/会话语义**：真实服务端需按连接隔离协商状态（当前即每连接
   `SessionState`，但 broker 登记与 lease 生命周期尚未接）。
