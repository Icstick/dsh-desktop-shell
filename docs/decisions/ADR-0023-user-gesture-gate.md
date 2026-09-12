---
id: ADR-0023
status: accepted
date: 2026-09-11
owner_role: runtime-and-security-owner
---

> **接受记录**：2026-09-11 由项目所有者接受。决策内容保持提出时的原文——按本目录约定，已接受的 ADR 不原地改写结论。
> **排期**：排在 v0.1.0 发布之后；实施前认领 `WI-M11-USER-GESTURE-GATE` 并走 security review。

# ADR-0023: lease 的 `approval_required` 接线为真正的用户手势闸门

> 编号说明：ADR-0021 已把 **ADR-0022** 预留给 local-transport peer identity，本 ADR 取 0023 以免占号。

## 背景

协议里早就写好了一个语义，但**没有任何实现方**：

1. **字段**：`Agreement.payload.leaseConstraints.approval_required`（`crates/daemon/src/envelope.rs:152-157`，schema `agreementPayload.leaseConstraints`）。协议粒度是**整个 activation**，没有 per-capability 粒度。
2. **错误码**：`USER_GESTURE_REQUIRED` 已在三处定义且在错误模型里有明确语义——
   - `crates/daemon/src/envelope.rs:69`（`Self::UserGestureRequired => "USER_GESTURE_REQUIRED"`）与 `crates/external-api-example/src/envelope.rs:69`
   - `packages/capability-contracts/src/types.ts:34`、`packages/capability-contracts/src/schema.ts:246`、`specs/protocol/envelope.schema.json:207`
   - `docs/protocol/ERROR_MODEL.md:9`：`USER_GESTURE_REQUIRED | 必须由用户直接触发 | Retry = 用户操作后`；紧邻的 `USER_DENIED | 用户拒绝 | 不自动重试`
3. **分工已经写明**：`crates/supervisor/src/broker/agent.rs:83-86` 的字段注释原文——「Surface policy flag (e.g. require a user gesture before a mutation). Carried for observability; **enforcement belongs to the surface layer (USER_GESTURE_REQUIRED), not the broker.**」

现状是一个**没有生产者的契约**：

- daemon 生成 Agreement 时把约束写死为无审批：`crates/daemon/src/server.rs:570` 的 `lease_constraints: Some(AgentLeaseConstraints::new(LEASE_MAX_SECONDS))`，`approval_required` 恒为 `None`（`crates/supervisor/src/broker/agent.rs:93`、`crates/adapter-dsh-std/src/negotiate.rs:210` 同样构造 `None`）。
- dispatch 管线（`crates/daemon/src/server.rs` 步骤 1→2→3：capability 已授予 → broker `enforce_dispatch`）**没有任何一项**与用户手势有关。
- 全仓 `USER_GESTURE_REQUIRED` 的引用只有定义处，**没有一处产生它**。

风险不是「功能缺失」，而是**契约说谎**：一个对端若按协议置位 `approval_required: true`，它会以为自己获得了「改动前须经用户点头」的保证，实际拿到的是**无任何审批的直通**。这与我们已经否决过的 tauri「`CommandScope` allow 为空即放行」属同一类错误——**安全相关的空集默认必须是拒绝**。

## 决策

1. **闸门对象**：一个 activation 的 `leaseConstraints.approval_required == true` 时，该 activation 的**全部** capability Invocation 在 dispatch 前都需要一次有效的用户手势记录；协议没有更细粒度，本 ADR 不发明更细粒度。
2. **arming 由用户侧决定，不由对端自报决定**。`approval_required` 目前由本端 daemon 在生成 Agreement 时填写，因此它的值必须来自**用户的本地策略**（哪些 capability 属于需要手势的敏感操作），而不是来自对端 `Hello` 的声明。**对端不能通过声明让自己免除闸门**（对端只能声明「我支持这个能力」，不能声明「我不需要审批」）。
3. **手势的定义**：一次由 **`shell_control` authority 的连接**（ADR-0021 决策 1 已把「人类 Shell 面」定义为一等语义）显式提交的批准记录，作用域 `(activation_id, capability coordinate)`，**一次性 + 限时**（默认 TTL 建议 ≤ 60s，未消费即失效），消费即作废。
4. **dispatch 行为是 fail-closed 且不阻塞**：没有有效手势记录时，该次 Invocation 立即返回 `USER_GESTURE_REQUIRED`，`retryable: true`，语义严格按错误模型表＝「用户操作后重试」。**不采用挂起等待手势**（备选方案 B）。
5. **用户拒绝用 `USER_DENIED`**（`retryable: false`），与「还没点头」区分开：前者是终态，后者是可重试态。
6. **审计**：手势记录的签发、消费、拒绝、以及每次因缺手势被拦截的 dispatch，都必须留下带 correlation id 的结构化记录（不含凭据、完整 URL、用户数据），对齐 ADR-0016 与 ADR-0021 决策 4。
7. **broker 不参与**：闸门留在 daemon（surface 层），维持 ADR-0014 的 DSH-neutral broker 边界。

## 与 DSH 内部审批链的关系（两层，不要混）

DSH 侧已存在 `approval/request` waterfall：`crates/adapter-dsh/src/events.rs:169` 将其列入 allowlist，`crates/adapter-dsh/src/notify.rs:9-10` 把它映射为桌面 `ApprovalRequired` 通知（TitleOnly，正文留在 UI）。

| | DSH 内部审批 | 本 ADR 的手势闸门 |
|---|---|---|
| 问的问题 | 「agent 这一次工具调用要不要放行」 | 「这个 activation 的 capability 调用要不要用户点头」 |
| 裁定方 | DSH 插件/策略（在 DSH 进程内） | daemon（在协议层，**不接受任何对端自报的批准**） |
| 载体 | `approval/request` waterfall | 协议错误码 `USER_GESTURE_REQUIRED` |

两者可叠加：**DSH 内部审批通过 ≠ 协议层手势已给**。可复用的是**通知通道与 UI 面**（同一块桌面区域），不是裁定逻辑。

## 备选方案（与推荐对比）

- **A. 一次性限时记录 + dispatch fail-closed（推荐）**：不占连接、语义与错误模型表一致、可审计、可测试。
- **B. 阻塞等待手势**（Invocation 挂起直到用户操作）：**否决**。协议模型明确把「等待用户」表达为**重试**而不是挂起；挂起会引入超时预算、连接占用与「谁来取消」的额外状态机。
- **C. 闸门放进 broker**：**否决**。与 ADR-0014 的 DSH-neutral broker 边界冲突，且 broker 注释已明确拒绝该职责。
- **D. 只把字段删掉/继续忽略**：**否决**。删掉等于承认协议里那个错误码永远不会出现；忽略等于继续让契约说谎。

## 影响、兼容性与风险

- **行为不变性**：落地后 `approval_required` 仍默认为 `None`（`server.rs:570`），除非用户策略显式 arming。**这意味着该改动可安全合入而无需等待发布窗口**——但它是 trust boundary 变化，仍需 security review（仓库变更协议）。
- **协议兼容**：不新增 envelope 方法、不改现有 capability 版本；只新增「手势记录」的**本地**提交路径。若后续要把手势跨进程暴露，需另立 ADR 并更新 schema/changelog/fixture（变更协议要求）。
- **残余风险**：手势裁定在 daemon 内，而 daemon 对「谁在控制面」的判定受 ADR-0021 的 H-2 限制——在 peer identity 落地前，手势提交者身份的上限仍是**连接级凭据**。本 ADR 不得对外声称「用户手势不可伪造」。
- **实施依赖**：本 ADR 的决策 3 需要一个本地提交路径（UI → daemon）。该路径的形式（复用现有 envelope 方法 vs 新增受控方法）是**实施期决策**，留待 WI 认领后与 security review 一并定。

## 待用户决策

1. 是否接受本 ADR（接受后才创建可认领的 WI）。
2. 排期：当前 M8 处于**发版门**（`tracking/project.yaml` 的 `next_action` 指向 M8-E 发布），本项建议**排在 v0.1.0 之后**，除非你要求并行（则走独立分支）。
