---
id: ADR-0014
status: accepted
date: 2026-08-28
owner_role: runtime-owner
---

# ADR-0014: P0 Capability Broker — Grant, Lease and Dispatch Enforcement

## 背景

M2 需要可靠运行时的授权核心（AC-LEASE-001、TM-PLG-001）。Adapter 认证或 Schema 合法均不能单独授予 native authority：capability 必须先协商（IF-NEGOTIATION Agreement），再经 Desktop grant 显式授予，dispatch 前还必须同时满足 lease、scope、owner 与 generation。当前仓库只有协议 Schema（capability-lease.schema.json、envelope.schema.json、protocol-coordinate.schema.json）与接口登记（IF-NEGOTIATION / IF-LEASE / IF-INVOCATION），没有可执行 enforcement。本 ADR 冻结 P0 Capability Broker 的语义，作为 `crates/supervisor`（MOD-SUPERVISOR）的首个可独立构建的 Rust 核心。

## 决策

1. **Dispatch 门禁**：一次 provider dispatch 只有在**同时**满足以下全部条件时才放行，且按固定顺序逐项校验（任一失败立即拒绝，返回对应标准错误）：
   - capability 已 grant（grant 只能来自已协商 capability——IF-NEGOTIATION Agreement 的 granted coordinate；Desktop grant 是 negotiated capability 之上的显式授予，两者缺一不可）；
   - 请求 owner 与 grant owner 匹配；
   - 请求 generation 与 grant generation 匹配；
   - 请求 scope 被 grant scope 覆盖；
   - 存在有效 lease：capability/owner/generation 匹配、scope 覆盖、未过期、未撤销。
2. **Grant/lease 模型**：`CapabilityGrant`（capability coordinate + version、scope、owner、generation、createdAtUnixMs）与 `Lease`（leaseId、capability、owner、generation、scope、expiresAtUnixMs、revoked?）的字段语义对齐 `capability-lease.schema.json`；capability 使用独立 `apiVersion` + `kind` 坐标（ADR-0005，对齐 `protocol-coordinate.schema.json`）。wire 层的 shape 校验（minLength/pattern/minProperties 等）属于 contract 层（capability-contracts / local-transport），Broker 只做 enforcement 语义。
3. **撤销原因集合（AC-LEASE-001）**：`disconnect`、`unload`、`expiry`、`human_takeover`、`generation_change`。撤销记录（reason + atUnixMs）不可覆盖：首个撤销记录先到先得，重复 revoke 是幂等 no-op。
4. **Generation change 语义**：对同一 capability 以新 generation 重新 grant 时，Broker 自动以 `generation_change` 撤销该 capability 现存全部 lease；stale generation 的 dispatch 返回 `STALE_GENERATION`（协议错误码）。generation 是 grant 级身份，任何请求必须携带与当前 grant 完全一致的 generation。
5. **幂等与 CONFLICT（DEVELOPMENT.md）**：完全相同的重复 grant/lease 幂等返回成功；同一 capability、同一 generation 但字段分歧的重新 grant 返回 `CONFLICT`；revoke 幂等（未知或已撤销 lease 均为 no-op，保留首条撤销记录）；已撤销 lease 的 id 不可复用，重新 lease 同 id 返回 `CONFLICT`；provider 注册重复返回 `CONFLICT`。
6. **错误语义**：`BrokerError` = UnknownCapability / UnknownProvider / NotGranted / LeaseExpired / LeaseRevoked / ScopeMismatch / GenerationMismatch / Conflict，映射到协议错误码（ERROR_MODEL）：UnknownCapability、UnknownProvider → `UNAVAILABLE`（retryable，条件变化后可重试）；NotGranted、LeaseExpired、LeaseRevoked、ScopeMismatch → `UNAUTHORIZED`（需重新授权）；GenerationMismatch → `STALE_GENERATION`；Conflict → `CONFLICT`。所有错误 message 是静态字符串，不携带 secrets、path、原始命令、用户数据或资源标识（无秘密跨边界）。
7. **Provider dispatch 骨架**：`Provider { id, capability }` 注册表；`dispatch(provider_id, invocation)` 先执行第 1 条门禁再调用 provider handler；未知 provider 返回 `UnknownProvider`；provider 声明的 capability 与 invocation 不匹配返回 `UnknownCapability`。
8. **可测试的时间**：Broker 接受可注入 `Clock`（默认系统时钟）；expiry 判定使用注入时钟，测试用 `FakeClock` 推进时间，不 sleep。
9. **范围**：本 ADR 只冻结 broker 核心（`crates/supervisor/src/broker.rs`，独立 workspace 构建）。daemon、managed_runtime 抽取、audit 事件流、lease sweep 定时器与 provider 实现属于后续切片（M2-D 抽取、M6）。

## 替代方案

- 仅验证已协商 capability，无 Desktop grant/lease：Adapter 协商成功即可调用 native provider，违反 DEVELOPMENT.md 的 P0 要求（schema 合法不能单独授予 authority），拒绝。
- 仅 grant 无 lease：无法表达激活/会话级授权、scope 收窄与撤销生命周期（AC-LEASE-001 全部原因集），拒绝。
- 错误携带调用方或资源细节（owner/scope/leaseId）：跨边界泄漏状态信息，违反无秘密边界，拒绝。
- 重复 grant 一律 CONFLICT：合法重放（幂等重试）会被误拒，破坏 DEVELOPMENT.md 的幂等要求；采用“相同则幂等、分歧则 CONFLICT”。

## 后果

- `crates/supervisor` 成为可独立 `cargo test/fmt/clippy` 的 P0 crate（package `dsh-supervisor`，edition 2024，仅依赖 serde/serde_json）；并入根 workspace 时移除其 `[workspace]` 表并加入根 members。
- IF-LEASE 的 grant/revoke/expire 与 IF-INVOCATION 的 dispatch 获得执行语义基础；后续 daemon/IPC 层复用同一 Broker 实例。
- Broker 状态可观察（grant/lease/provider 查询接口），为 diagnostics 切片（AC-LOG-001）提供证据源。
- 测试覆盖 AC-LEASE-001 全原因集合与逐项门禁；expiry 测试不依赖真实时间。

## 验证门禁

- grant→lease→dispatch 成功路径；provider handler 收到 invocation 并返回结果。
- 未知 capability / 未知 provider / capability-provider 不匹配均拒绝。
- disconnect、unload、human_takeover、generation_change、expiry 五种撤销后 dispatch 均拒绝（AC-LEASE-001）。
- 重复 grant 幂等、分歧 CONFLICT；重复 lease 幂等；revoke 幂等；已撤销 lease id 重发 CONFLICT 且错误确定可复现。
- owner / generation / scope（grant 级与 lease 级）不匹配逐项拒绝且错误可区分。
- BrokerError → 协议错误码映射与静态 message 测试（无秘密）。
- `cargo test --manifest-path crates/supervisor/Cargo.toml`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 全绿。

## Wire 与 Broker 内部模型映射

capability-lease.schema.json 定义未来 contract 层的 wire shape（leaseId、participantId、activationId、expiresAt date-time）；Broker 内部 Lease 使用 id/expires_at_unix_ms/revoked 等字段并带撤销记录。对应关系：wire leaseId ↔ broker id；wire participantId ↔ owner；wire activationId ↔ generation/instance；wire expiresAt ↔ expires_at_unix_ms；wire 撤销由 revoke(reason) 的 LeaseRevocation 表达。wire shape 校验属于 capability-contracts（M5），Broker 只做 enforcement，不做 wire 校验。

## 受影响模块

- MOD-SUPERVISOR（crates/supervisor，本 ADR 是 broker 部分）
- IF-NEGOTIATION / IF-LEASE / IF-INVOCATION（语义消费方；Schema 不变）
- capability-contracts / local-transport（后续 wire 校验与 IPC，不随本 ADR 变更）
