# ADR-0018: Adapter Architecture and External API Interface (M5)

- Status: accepted
- Date: 2026-08-30
- Milestone: M5 Interop
- Owner: interop-and-security-owner

## Context

M5 需要：① Legacy 与 optional dsh-std 两类 adapter 共存并可降级（ROADMAP M5）；② 激活 IF-NEGOTIATION/IF-INVOCATION（Hello/Agreement/Invocation/Result/Event）；③ agent_automation 授权链落地（AC-TERM-001、AC-BRW-002）；④ maintainer 需求：统一的对外 API interface（外源接口）。两个外部基线经 2026-08-30 双调研：dsh-std 17 包可安装（rc 生态，pin 可行）；DSH /api 无版本化（notification 可行、usage 部分、restart hints 无现成）。LADDER 定义了 L0-L3 兼容阶梯与变化吸收点；DSH_STD_POLICY 规定 conformance 绑定精确版本。

## Decisions

### 决策 1：Activation ownership —— 每次激活单独协商，不缓存 Agreement
- 每次 capability activation（外部调用方/agent/adapter 连接）独立完成 Hello→Agreement；前一次的 Agreement 不得缓存为新 generation 的事实（DSH_STD_POLICY 原文要求）。
- 协商状态机（proposed→agreed→active；reject/degrade 路径）由 capability-contracts 实现并在 broker 登记。

### 决策 2：Conformance 声明绑定精确坐标
- dsh-std conformance 声明必须同时绑定：精确 package version（如 core@0.1.1-rc.1）+ GitHub commit（3df0543）+ artifact integrity；禁止只写 latest/rc 标签（connection rc 标签 08-29 已移动的教训）。
- 三态语义（absent/known/unknown）：absent = 域协议未实现（L0/L1 行为不变）；known = 声明坐标与本地 fixture 校验通过（L2 能力）；unknown = 坐标漂移/校验失败（fail-closed + 记录，不自动降级承诺）。

### 决策 3：alpha type 不穿越 adapter 边界；adapter 是唯一变化吸收点
- DSH 自身类型与 dsh-std alpha 类型都止步于各自 adapter 内部；Desktop 内部一律使用 capability-contracts 的 wire 类型。
- LADDER 变化吸收点映射：DSH launch/internal API → adapter-dsh；dsh-std alpha → adapter-dsh-std；transport 标准 → local-transport adapter；Web router → Surface URL/route restore（不用 DOM）。

### 决策 4：Additive compatibility —— adapter 失效不得破坏低层级
- L2/L1 adapter 任何失败路径都必须可降级到 L0 baseline（DSH process + HTTP Web UI 的 Surface/health/Managed lifecycle）。
- 降级测试进 conformance matrix：known 版本适配失败 → 记录 + 保持 L0，不 panic 不阻断。

### 决策 5：统一外源 API interface（maintainer 需求）
- 外部工具/脚本/系统访问 Desktop 能力的统一面 = **local-transport 载体 + capability-contracts envelope（Invocation/Result/Event）+ Capability Broker 授权**。
- 认证：local-transport 一次性 credential（已有，AC-IPC-001/002）+ broker grant/lease；外部调用方与 agent 走同构授权链（M5-E）。
- 契约源：M5-B 的 wire/shape 层（specs/protocol/*.schema.json + packages/capability-contracts 类型）即统一 API 契约；capability 独立版本化（ADR-0005）+ envelope 版本。
- 定位区分：Desktop 对外 API 是 Desktop 能力面；DSH 自身 /api 由 adapter-dsh 吸收，两者不混。
- M5 范围：M5-B 提供 wire/shape + 一个参考 example（外部脚本经 local-transport 调用 list_browsers）；完整外部 SDK/文档化服务面评估为 M6 项。

### 决策 6：M5-C Legacy adapter 范围（风险裁剪）
- notification：完整实现（$events WS 流消费，mux 协议 + cookie 认证）。
- usage：部分实现（session 事件内嵌用量聚合；无专用端点）。
- restart hints：**降级为事件推断**（cordis dynamic-package/retract + settings/document-updated → 提示"配置已变化"），明确标注非 DSH 原生语义；上游新增端点前不承诺。

### 决策 7：agent_automation 授权链（M5-E）
- terminal agent 模式（AC-TERM-001 反转）与 browser interact/take_over（AC-BRW-002）经统一授权链：agent 协商 → broker grant → lease → mutation；human takeover 撤销 lease。
- 每次 mutation 校验 dispatch 门禁（ADR-0014 固定顺序：capability+grant+owner+generation+scope+lease）。

## Consequences

- specs/protocol/ 增加/修订 negotiation/invocation fixtures（M5-A 细化清单）；IF-NEGOTIATION/IF-INVOCATION implementation_status → authorized（M5-B）。
- packages/capability-contracts 实现 wire/shape + 协商状态机；broker 接线 IF-LEASE 完整化。
- packages/adapter-dsh（L1）与 packages/adapter-dsh-std（L2）按决策 3/6 实现；conformance matrix 测试（三态）。
- 统一 API example（M5-B 产物）验证 local-transport 外部调用路径。
- M5-E 授权链完成后，AC-TERM-001/AC-BRW-002 转 verified。
- dsh-std 依赖逐包 pin 精确版本 + integrity（EXTERNAL_BASELINE 已刷新至 3df0543/core rc.1）。
