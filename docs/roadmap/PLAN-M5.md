# M5 Interop — Execution Plan (2026-08-30)

> 规划先行：contract-first。M5 的独特风险：两个外部基线（DSH 自身接口、dsh-std alpha）都在漂移；本阶段的核心是 **变化吸收点**（LADDER）与 **授权链**，不是实现新 surface。

## M5 退出标准（MILESTONES / WI-M5-INTEROP / ACCEPTANCE）

1. Absent/known/unknown dsh-std conformance matrix 通过（WI-M5-INTEROP acceptance）。
2. Legacy 与 optional dsh-std 两类 adapter 共存并可降级（ROADMAP）。
3. agent_automation 授权链落地：terminal（AC-TERM-001）、browser interact/take_over（AC-BRW-002）、notification/usage adapter（ADR-0016 决策 5）。
4. IF-NEGOTIATION/IF-INVOCATION 激活；IF-LEASE 从 partial 到完整（broker + wire）。

## 范围梳理（M1-M4 积累的 M5 项）

| 来源 | 内容 | 切片 |
|------|------|------|
| WI-M5 | dsh-std absent/known/unknown matrix；baseline refresh | M5-A/D |
| IF-NEGOTIATION/INVOCATION | Hello/Agreement/Invocation/Result/Event 激活（not_authorized→authorized） | M5-A/B |
| IF-LEASE | wire/shape 层补齐（broker enforcement 已有，M2） | M5-B |
| ADR-0016 决策 5 | DSH notification/usage adapter | M5-C |
| ADR-0017 决策 2 / AC-BRW-002 | browser interact/take_over + broker lease | M5-E |
| AC-TERM-001 | terminal agent_automation 模式 | M5-E |
| MOD-ADAPTER-DSH / DSH-STD | L1 Legacy + L2 optional adapters | M5-C/D |

## 切片划分

### M5-A 契约与基线冻结
- dsh-std baseline refresh：registry 坐标（latest/rc 标签）、pinned version、artifact integrity（EXTERNAL_BASELINE 更新；LADDER/DSH_STD_POLICY 已冻结原则）。
- ADR-0018 Adapter Architecture：activation ownership（每次激活单独协商，不缓存 Agreement）、conformance 声明必须绑定精确 package version + integrity + fixture、alpha type 不穿越 adapter、additive compatibility（adapter 失效不破坏低层级）。
- IF-NEGOTIATION/IF-INVOCATION 契约细化：envelope schema 现状核验 + fixtures 补齐（Hello/Agreement/Invocation/Result/Event 正负用例）。
- AC 新增：AC-ADAPT-001（absent/known/unknown 三态行为）、AC-ADAPT-002（adapter 失效降级不破坏 L0/L1）。

### M5-B capability-contracts wire/shape 层
- packages/capability-contracts：Hello/Agreement/Invocation/Result/Event 类型 + 校验器 + lease wire 形状（与 specs/protocol/*.schema.json 对齐）。
- broker 接线：IF-LEASE 完整化（grant/revoke/expire wire 形状与 M2 broker enforcement 对接）；IF-NEGOTIATION/INVOCATION implementation_status authorized。
- 测试：wire 序列化 fixture 比对（沿用 M3/M4 模式）、协商状态机（proposed→agreed→active、reject/degrade 路径）。

### M5-C Legacy adapter（MOD-ADAPTER-DSH，L1）
- 基于真实 DSH 接口调研（D:\deepseek-harness 本地仓库 + 运行行为）：usage/notification/restart hints 的可行适配点。
- adapter-dsh 实现：只消费 DSH 公开 HTTP/WS 表面；DSH-specific type 止步于 adapter 边界；降级语义（adapter 失败 → L0 baseline 不变）。
- 测试：fake-DSH fixture 驱动（tests/fake-dsh 已有基建）。

### M5-D dsh-std optional adapter（MOD-ADAPTER-DSH-STD，L2）
- adapter-dsh-std：negotiation（Hello/Agreement）、facets、conformance 声明（pinned version + integrity）。
- absent/known/unknown matrix：无 std（L0/L1 行为不变）、known std 版本（L2 能力）、unknown/漂移版本（fail-closed + 记录）。
- 测试：三态 fixture 矩阵 + degraded behavior。

### M5-E agent_automation 授权链
- broker mutation lease 全链：agent 协商 → grant → lease → terminal/browser mutation（AC-TERM-001 反转、AC-BRW-002 落地）。
- terminal agent 模式：write/resize 经授权链；browser interact（点击/输入/导航）+ take_over（human takeover 撤销 lease）。
- 安全：授权链 fail-closed（无 lease 拒绝 mutation）；每次 activation 独立协商（不缓存）。

## 执行顺序

1. M5-A 契约/基线 → 提交（外部基线 refresh 需要联网核验 dsh-std registry 与 DSH 接口现状）。
2. M5-B wire/shape + broker 接线（纯内部契约，可并行于 M5-A 的调研部分）。
3. M5-C Legacy adapter（依赖 DSH 接口调研）。
4. M5-D dsh-std adapter（依赖 M5-A 基线 + M5-B wire）。
5. M5-E 授权链（依赖 M5-B + M5-C/D 的 mutation 面）。
6. 集成门禁、独立评审、HANDOFF-M5-INTEROP、maintainer 验收、合并 main。

## 统一外源 API interface（maintainer 2026-08-30 需求）

- **需求**：Desktop 需要一个**统一的 API interface 作为外源接口**——外部工具/脚本/系统以稳定方式调用 Desktop 能力（browser/terminal/runtime/usage/notification…），而不是各 surface 各 bridge 命令零散暴露。
- **现状缺口**：tauri IPC 仅 shell webview 可调；local-transport 是内部载体（无应用层契约）；browser/terminal 命令是 surface 内联。
- **方向**（随 M5-A ADR-0018 定型）：
  - 载体：local-transport（已有认证 loopback、一次性 credential、framing/deadline）之上增加**应用层 RPC 面**（envelope 复用 capability-contracts 的 Invocation/Result/Event）。
  - 契约：M5-B 的 wire/shape 层即统一契约源；capability 独立版本化 + envelope 版本（沿用 ADR-0014/0005）。
  - 授权：复用 Capability Broker（grant/lease/dispatch）；外部调用方走显式 credential + 授权链（与 agent 授权链同构）。
  - 定位区分：Desktop 对外 API ≠ DSH 自身 /api（LADDER 变化吸收点仍成立）。
- **归属**：M5-B 扩展（wire/shape 契约包含对外 API 定义 + 一个参考实现 example：外部脚本经 local-transport 调用 list_browsers 等）；完整外部服务面与文档化 SDK 评估为 M5 收尾或 M6 项（取决于范围确认）。

## 明确不做（本阶段）

- 采用 dsh-std 未稳定 wire 作为核心依赖（L2 只表示已知版本 conformance）。
- 跳过 Legacy/L0 fallback（adapter 失效必须可降级）。
- Connection/wire carrier 标准化映射（LADDER 注明 wire 稳定后再评估）。
- Browser/Terminal 的 agent 自动化 UI（只做授权链与 adapter 面，UI 侧 M5-E 最小化）。

## 风险（2026-08-30 双调研初步结论）

### 风险 1：dsh-std alpha 漂移 —— 结论：**中低，可 pin，风险较 8-25 降一档**

- 观测（08-30）：17 个包全部可安装（tarball/integrity/types 齐全，ESM，zod@^4）；GitHub main 3df0543（08-29，+3 commits）；发布流水线激活（39 tags / 22 prerelease releases，connection rc 标签 08-29 移动）；README 仍标 early draft。
- **决策建议**：pin `@dsh-std/core@0.1.1-rc.1`（core 自 08-23 未动）+ GitHub commit 3df0543 双锁；逐包锁精确版本 + integrity，不跟浮动 latest/rc 标签。
- conformance 矩阵（absent/known/unknown）可实测判定（参考包可真实安装运行）。
- M5-D 工作量：包可用分支 → **2-4 人日** + rc 漂移跟进成本。

### 风险 2：DSH 接口无稳定契约 —— 结论：**中，restart hints 是唯一实质缺口**

- 观测（08-30，源码级）：`/api` JSON-RPC（7 namespace：session/workspace/settings/credentials/directoryPicker/fileReferences/skills）+ `/api/remote.mux` WS + `$events` 流（17 事件 allowlist）+ token/cookie 认证（ADR-0012 证实）。
- notification：✅ 可行（$events 直接消费）。usage：⚠️ 部分（session 事件内嵌用量需聚合）。**restart hints：❌ 无现成端点/事件**。
- **新增风险**：DSH /api **无版本化**（单一前缀 + exact-keys 校验，0.1.2-alpha.1 无稳定性承诺）→ adapter 对 DSH 升级脆弱，LADDER 变化吸收点会被高频触发。
- **决策建议**：M5-C 范围 = notification（完整）+ usage（事件聚合）；restart hints 降级为"事件推断（cordis dynamic-* + settings/document-updated）+ 已知缺口记录"，不阻塞 M5。

### 风险 3：M5-E 授权链

- 横跨 broker/adapter/surface 三层，安全敏感（security_review required ×N）；依赖 M5-B wire 完整性。结论：按现有 broker 机制（ADR-0014）扩展，无新技术风险；工作量中等。
