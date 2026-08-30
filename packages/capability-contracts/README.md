# Capability Contracts

**Module ID:** `MOD-CAPABILITY-CONTRACTS`
**Target milestone:** M2 (wire/shape layer lands with M5-B)
**Canonical status:** [MOD-CAPABILITY-CONTRACTS](../../tracking/modules/MOD-CAPABILITY-CONTRACTS.yaml)

## Purpose

为 UI、Rust boundary、adapters 与外部调用方（统一外源 API，ADR-0018 决策 5）提供
DSH-neutral 协议 wire/shape 层：envelope 帧校验、协商状态机、跨消息语义与 lease 映射。

零运行时依赖、TS strict、erasable-only（无 enum/namespace），源码直出
（`exports` 指向 `src/*.ts`，消费方经 Vite/esbuild 直接 import TS）。

## Owns

- coordinate/envelope/lease/error semantics
- 协商状态机（proposed → agreed → active；reject/degrade，ADR-0018 决策 1）
- 跨消息语义规则（replyTo/correlation/grant/generation/replay）
- consumer types later

## Does not own

- Cordis/DSH/std imports
- transport implementation
- 逐 method 的 payload 细化（payload 视为 opaque object）

## 库能力

| 模块 | 能力 |
|------|------|
| `validate.ts` | `validateEnvelope` 帧级校验（protocol/id/kind/participant/timestamp/generation + kind 分支的 payload 形状与禁字段）；`validateLease` 校验 broker lease |
| `negotiate.ts` | `NegotiationSession` 状态机：receiveHello / issueAgreement / activate / reject；granted⊆supports 与 granted∩unavailable=∅ 强约束；degrade 路径；Activation 记录 |
| `semantics.ts` | `validateSequence` / `SemanticValidator` 跨消息规则：replyTo 指向前序消息、Result.error.correlationId 匹配被引 Invocation、granted⊆supports、Invocation.capability∈granted、generation 单调、id 重放拒绝 |
| `lease.ts` | `constraintsToLease` / `leaseToConstraints` 映射（maxSeconds ↔ expiresAt）；approvalRequired 为 broker 侧策略，不进 wire lease（schema 禁额外字段） |

规范真源：`specs/protocol/*.schema.json`（嵌入副本在 `src/schema.ts`，由测试绑定防漂移）。

## 用法

```ts
import { validateEnvelope, NegotiationSession, validateSequence, constraintsToLease } from "@dsh-desktop/capability-contracts";

// 1. 帧级校验（与 JSON Schema 判定一致）
const frame = validateEnvelope(rawMessage);
if (!frame.ok) {
  // frame.errors: { path, message }[] — fail closed, 丢弃
}

// 2. 协商（每次激活独立协商，不缓存 Agreement）
const session = new NegotiationSession("activation-1");
session.receiveHello(frame.value);              // proposed
const agreement = session.issueAgreement({      // agreed
  activationId: "activation-1",
  granted: [terminal, browser],
  unavailable: [{ coordinate: runtime, reason: "unsupported_version" }], // degrade
});
const activation = session.activate();          // active

// 3. 跨消息语义（broker 接线处）
const seq = validateSequence([hello, agreement, invocation, result]);

// 4. lease 映射
const lease = constraintsToLease({ maxSeconds: 3600 }, { leaseId, participantId, /* ... */ });
```

## 测试

```sh
pnpm install   # 包内独立 lock（M5-B 认领工作项）
pnpm test      # vitest run
pnpm typecheck # tsc --noEmit
```

- `validate.test.ts`：22 个 protocol fixtures 交叉验证（valid 全过 / invalid 全拒）+ 嵌入 schema 与
  `specs/protocol/*.schema.json` 逐字段比对（绑定 schema 防漂移）。
- `negotiate.test.ts`：状态机全路径（happy / degrade / reject / CONFLICT / 幂等）。
- `semantics.test.ts`：9 条跨消息规则正反例。
- `lease.test.ts`：映射 roundtrip + 两端 schema 校验。

## Inputs

- normative JSON Schemas

## Outputs

- validated contract artifacts（`validateEnvelope` / `validateSequence` 结果）

## Dependencies

- specs

## Interfaces

- `IF-NEGOTIATION`
- `IF-INVOCATION`
- `IF-LEASE`

规范真源见 [specs](../../specs/README.md)；架构原因见 [ADR index](../../docs/decisions/README.md)。
