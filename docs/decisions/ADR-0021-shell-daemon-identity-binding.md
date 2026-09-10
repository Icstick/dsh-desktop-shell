---
id: ADR-0021
status: proposed
date: 2026-09-10
owner_role: runtime-and-security-owner
---

# ADR-0021: Shell–Daemon 身份绑定（control plane 不再靠自报）

## 背景

2026-09-10 的安全审计（`docs/audits/audit-summary-2026-09-10.md` H-2）发现：daemon 对「你是不是那个已认证的 Shell」的判据**全部来自对端自报**，没有任何与真实身份的绑定。

具体两处：

1. **请求什么就授予什么**。`crates/daemon/src/server.rs:493-504` 的 `handle_hello` 遍历对端 `hello.supports`，只要该 coordinate 在本地 catalog 里（`catalog_supports`）就放进 `granted` 并写进 Agreement。授权判据是「你声明支持 X 且我实现了 X」，与「你是谁」无关。
2. **控制面身份是纯字符串比较**。`server.rs:542-543` 在 broker 单 owner 冲突时用 `participant.component == "dsh-desktop-shell" && participant.facet == "shell"` 决定是否走 broker-relaxed 路径；`server.rs:676-678` 在 dispatch 时用同一个字符串再判一次。任何对端都能把这两个字段填成这两个值。

这两处合起来构成审计里的完整利用链：**同机另一个进程读走一次性凭据 → 连接 127.0.0.1:37771 → 自报 shell 身份 → 拿到 broker-relaxed 的 human 路径 → 调 `terminal.create` 并指定 `shell` 与 `cwd`（`crates/daemon/src/terminal.rs:76-87` 的请求确实携带这两个字段）→ 以指定解释器在指定目录执行任意命令。**

### 前提条件已经被 H-1 收紧，但这不等于缺陷消失

同一次审计的 H-1（daemon 凭据文件无权限收紧）已在 `6695c7f` 修复：Unix 上凭据目录 0700、文件 0600，且数据目录不再退化到当前工作目录。**链条的第一步现在很难走通**——同机另一用户读不到 token（Windows 上 `%APPDATA%` 本来就继承按用户的 profile ACL）。

但 H-2 描述的是一个**仍然存在且与文件权限无关的语义缺陷**：

- 凭据是一次性的、按连接握发的，**只要拿到 token 就能连**；token 本身不携带、也不绑定任何调用方身份（`crates/local-transport` 的 `AGENTS.md` 原文：「token 不证明单插件身份」）。
- 于是「human shell 路径」与「agent automation 路径」在服务端的唯一区别就是两个自报字符串。**任何能连上的对端都可以自称 Shell。**
- 这条链的失败模式是静默的：没有日志、没有告警、没有可观测的异常。

本 ADR 的任务是把这条语义缺陷写成决策，并给出**可落地的路线**——包括明确说出「哪一部分现在不能靠改代码真正关掉」。

## 决策

1. **把「control plane（人类 Shell 面）」定义为一等语义**，而不是一个字符串常量。`crates/daemon/src/server.rs` 的 `Activation` 记录必须显式携带该 activation 被服务端判定的 authority 类别（至少 `shell_control` / `participant` 两类），该类别在 `handle_hello` 时判定一次、在 dispatch 时按类别校验，不再每次重新比较字符串。
2. **身份判据必须在服务端可复算**。自报字段只允许作为**声明（claim）**；服务端必须有一个独立于声明的事实来源来确认 claim。在当前实现里唯一存在的事实来源是 **local-transport 的凭据握手**（连接级认证），因此：
   - 声明为 Shell 身份的 activation **只能由已通过凭据握手并仍持有该连接的连接提出**；连接断开后其全部 activation 与控制面资格一并作废（与 ADR-0014 的 disconnect 语义一致）。
   - 一个连接一旦以某个身份建立 activation，**不得在同一连接上再以另一种身份建立 activation**；违反即 `UNAUTHORIZED`，并在 daemon 日志留下记录。
3. **fail-closed 是默认值**。任何未绑定到上述事实来源的身份声明，其 authority 一律降到最低（等同普通 participant / agent automation），不得获得 broker-relaxed 路径。审计报告里 H-6 提到的「未知 authority 一律放行」是同一类错误的反面教材，本 ADR 明确取相反默认值。
4. **控制面操作必须可审计**。`shell_control` 类别的 activation 建立、broker-relaxed 路径的使用、以及任何身份判定失败，都要有带 correlation id 的结构化日志（不含 token / 完整 URL / 用户数据）。
5. **长期方向是 OS 级 peer identity**（见备选方案 B），本 ADR 只把它登记为下一决策的输入；在它落地之前，第 2 条的「连接级认证」是**声明的上限**，不得对外声称 H-2 已关闭。

## 推荐（基于证据的选择）

**推荐：先接受本 ADR 的第 1/3/4 条作为协议不变量并立即实现；第 2 条的完整形态与其他备选方案的取舍留给下一个决策点（建议 ADR-0022：local-transport peer identity）。**理由如下，逐条对应备选方案。

## 备选方案

### 方案 A：把身份写进握手（凭据绑定身份）

做法：在 `ClientHello`（`crates/local-transport/src/handshake.rs:11-14`，当前只有一个 `token` 字段）中加入调用方身份声明，由 daemon 把该声明与本次握手一并记录，后续 activation 只能使用本次握手声明的身份。

- **优点**：真正确立「token 与身份绑定」；服务端有明确的事实来源；与 ADR-0007 的 local-transport 设计一致（认证在 transport 层）。
- **代价**：**修改 wire format**——`crates/local-transport` 是公开协议（`specs/protocol/*.schema.json` + fixtures + compatibility 矩阵），按 AGENTS.md「公开协议变化：更新 Schema、changelog、compatibility fixture 计划和 migration note」，需要协议版本推进（`interop.dsh-desktop.local/v1alpha1` → `v1alpha2`）、fixture 计划与迁移说明；`crates/local-transport/AGENTS.md` 还明确「token 不证明单插件身份」，意味着这条改动会推翻一条既有书面约束，需要单独的安全 review。
- **判定**：方向正确但**超出本次授权范围**（触协议 wire format 且远超 200 行）→ 不在本切片实现。

### 方案 B：OS 级 peer identity（进程绑定）

做法：daemon 在 accept 后向内核查询**对端进程身份**，只有当对端确为 Shell 进程时才承认其 Shell 声明。

- **可选实现**：Windows 命名管道 `GetNamedPipeClientProcessId` + 进程可执行路径比对；Unix `SO_PEERCRED`（PID/UID）+`/proc/<pid>/exe` 比对。
- **优点**：这是唯一能在「同一用户下的另一个本地进程」这个威胁模型里真正区分「Shell」与「其它进程」的机制——**方案 A 和 C 都做不到这一点**。
- **代价**：①平台分叉两套实现；②daemon 需要知道「期望的 Shell 可执行文件路径」（安装路径 vs `tauri dev` 的 target/debug，需要新的配置面与失败语义）；③本机是 Windows，**无法验证 Unix 分支**，只能靠 CI 三平台；④`crates/local-transport` 的 `AGENTS.md` 限定该 crate 只做 local-only / replay / stale / oversize 防护，把 peer identity 加进去需要先更新模块契约。
- **判定**：**长期方向**，登记为 ADR-0022 的输入；本次不实现。

### 方案 C：保持自报，但收紧其后果（本次推荐实现的部分）

做法：不引入新的身份来源，而是让「自报 Shell」这件事**只能带来有限且可审计的后果**：

1. activation 显式记录服务端判定的 authority 类别（第 1 条）；
2. broker-relaxed 路径的判定改为读该类别，且**只**在类别为 `shell_control` 时生效（把现状的字符串比较集中到一处，去掉 dispatch 时的二次字符串比较）；
3. 同一连接上**混用身份**（先声明 Shell 再声明为 agent automation，或反之）一律拒绝——这是当前实现里一个真实存在的、不需要 OS 支持就能堵住的口子；
4. 身份判定失败与控制面激活都写结构化日志（第 4 条）。

- **优点**：不动 wire format、不改 Schema、不引入平台分叉；把散落的两处字符串比较收敛成一个显式语义，为方案 A/B 留好接口；新增的负向测试可以长期守护「非 Shell 拿不到 relaxed 路径」这条不变量。
- **诚实的局限（必须写进代码注释与报告）**：**方案 C 并不能阻止一个已经拿到 token 的同用户进程自称 Shell**。它减少的是「静默的、无痕迹的权限提升」，不是「权限提升本身」。因此第 5 条明确：在方案 A 或 B 落地前，不得宣称 H-2 已关闭。
- **判定**：**本次实现**（第 2 条的完整形态由于需要决定「连接断开后控制面归属」的语义，留给后续——见下）。

### 方案 D：取消 broker-relaxed 路径（Shell 也是普通 broker 参与者）

做法：不再为 Shell 开特例，Shell 启动时自己在 broker 里协商并持有 grant。

- **优点**：单一授权模型，没有特例就没有「谁是特例」的身份问题。
- **代价**：会改变 `credential_reissue.rs` 与 `terminal_integration.rs` 里已固化的 HIGH-2 语义（Shell 与 agent 的 single-owner 冲突解析），触及 broker 语义与 ADR-0014/0018，属独立决策。
- **判定**：不在本次范围。

## 后果

- **本切片（方案 C 的第 1/2/3/4 条）**：`Activation` 增加 authority 类别字段；`handle_hello` 一次判定；dispatch 与 broker-relaxed 判定读该字段；同一连接混用身份返回 `UNAUTHORIZED`。**不改 wire format、不改 Schema、不改 fixture**，因此不改兼容性矩阵。
- 新增一条「非 Shell 身份不得获得 broker-relaxed 路径」的负向集成测试，以及一条「同一连接不得混用身份」的负向测试；现有 `non_shell_conflict_stays_fail_closed` 语义必须继续成立。
- **不关闭 H-2**：本切片之后，「拿到 token 的同用户进程自称 Shell」依然可行（它甚至可以直接自称 human 并调 `terminal.create`）。审计条目在 H-2 的定位应从「已修复」改为「已收敛 + 已登记方案 A/B」，直到方案 A 或 B 落地。
- 后续决策：ADR-0022（local-transport peer identity 或握手身份），需要同时决定「Shell 可执行文件路径从哪来」与「连接断开后控制面如何转移」两个问题。

## 验证门禁

- 负向测试：非 Shell 身份（`component/facet` 自报为 agent automation）在与 Shell 冲突后**拿不到**任何 grant，且 dispatch 返回 `UNAUTHORIZED`（既有 `non_shell_conflict_stays_fail_closed` 必须继续通过）。
- 负向测试：同一个连接上先以 Shell 身份建立 activation、再以其它身份建立 activation → 第二次 `UNAUTHORIZED`；反向顺序同样拒绝。
- 正向测试：独立的第二条 Shell 连接（Shell 重启场景）仍能建立 `shell_control` activation 并沿用 human 路径。
- 回归：cargo workspace 全绿（本切片前 567 passed）；clippy `-D warnings` 退出 0。
- 反向证据：本切片**不**声称关闭 H-2；报告与 tracking 中 H-2 的状态只能是「部分收敛 + 已登记后续 ADR」。

## 受影响模块

- `MOD-PROCESS-MANAGER`（daemon server：activation 分类与 dispatch 门禁）
- `MOD-LOCAL-TRANSPORT`（仅作为后续方案 A/B 的落点，本切片不改）
- `MOD-TERMINAL-PROVIDER`（消费方：控制面语义变化的观察点）

## 参考

- `docs/audits/audit-summary-2026-09-10.md` H-2（`crates/daemon/src/server.rs:493-504` / `:542-552` / `:676-678`）
- `docs/audits/fixes-desktop-shell-2026-09-10.md`（H-1 落地记录，commit `6695c7f`）
- ADR-0007（local transport）、ADR-0014（capability broker）、ADR-0019（daemon 迁移，决策 5：凭据与握手）
