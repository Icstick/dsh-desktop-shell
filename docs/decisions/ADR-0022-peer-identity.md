---
id: ADR-0022
status: accepted
date: 2026-09-12
owner_role: runtime-and-security-owner
---

> **接受记录**：2026-09-12 由项目所有者接受。决策 1/2/3/4 生效；spike（Windows 半）已完成并支持 B2
> 方向（`docs/research/SPIKE-PEER-IDENTITY-20260912.md`）。实现按决策 3 的约束分片推进
> （carrier 拆分 → Windows Named Pipe → daemon 接线 → Unix 分支 + CI 验证）；落地并有负向测试后
> H-2 才可标「已关闭」。已接受 ADR 不原地改写结论。

# ADR-0022: Shell–Daemon peer identity（local-transport 对端进程绑定）

> 编号说明：本号由 ADR-0021 预留给 local-transport peer identity（见该 ADR 决策 5 与备选方案 B）。

## 背景

ADR-0021（accepted 2026-09-12）只落地了 option C：activation authority 类别显式化、
身份混用 fail-closed、控制面判定可审计。它明确留下一个**未关闭**的缺陷（H-2）：

- 凭据是一次性、按连接签发的；`dsh-local-transport` 的书面约束是「token 不证明单插件身份」。
- 于是「拿到 token 的同用户进程」可以连接 127.0.0.1:37771、自报 `shell` 身份、走 broker-relaxed
  的 human 路径，并通过 `terminal.create` 的 `shell`/`cwd` 字段以指定解释器执行任意命令。
- option C 收敛的是「静默的、无痕迹的权限提升」，**不是权限提升本身**。

现有载体的来源是历史妥协：ADR-0007 的原始设计是「Windows 优先 Named Pipe，macOS/Linux 优先
Unix Domain Socket；平台不可用或 PoC 未完成时允许 127.0.0.1 随机端口 fallback」。M2 实现选了
loopback TCP fallback，此后一直是唯一载体（0.2.1 起端口固定为 37771）。
**loopback TCP 的内核对端身份不可得，这正是 H-2 无法在应用层关闭的根因。**

## 决策空间（两个方向，按证据比较）

### B1 · 在现有 TCP 载体上查对端 PID

做法：daemon accept 后按 TCP 四元组反查对端进程——Windows `GetExtendedTcpTable`
（本地 37771 + 对端端口 → owner PID），Linux `/proc/net/tcp` + inode 反查 PID，macOS 需要
libproc。再将该 PID 的可执行路径与「期望的 Shell 可执行文件路径」比对。

- 优点：不改载体；改动集中在 daemon 侧一次校验（约 150–250 行 + 平台分叉）。
- 弱点：PID 复用与查询竞态是**理论弱点**（校验与连接建立之间有窗口）；macOS 查询路径更长；
  仍然需要一个「期望 Shell exe 路径」的新配置面（安装路径 vs `tauri dev` 的 target/debug）。
- 定位：可作为过渡，但不是终点。

### B2 · 恢复 ADR-0007 的载体（Named Pipe / UDS），拿内核级 peer identity

做法：`dsh-local-transport` 增加 Windows Named Pipe 与 Unix Domain Socket 载体；daemon 用
`GetNamedPipeClientProcessId`（Windows）与 `SO_PEERCRED` / macOS `getpeereid`（Unix）取得
**内核保证的**对端 PID/UID，再比对期望的 Shell 可执行路径或同用户范围。

- 优点：**内核保证、无竞态**；`local-transport` 的 framing codec 本来就是
  carrier-agnostic（`std::io::Read + Write` 即可，且代码注释已把 Named Pipe/UDS 标为
  ADR-0007 预留扩展点）；loopback TCP 可作为显式降级保留给不支持的平台。
- 代价：载体层实现（连接建立、地址语义、并发模型、credential 传递路径）、三平台验证；
  `local-transport` 的公开契约与测试面扩大（需更新 module 契约与 AGENTS 约束）；
  本机是 Windows，Unix 分支只能靠 CI 三平台。
- 定位：唯一能真正关闭 H-2 的方向。

## 决策（推荐）

1. **方向取 B2**：peer identity 的正确落点是载体，不是应用层校验；B1 只作为 B2 未就绪时的可选过渡，
   且必须以「过渡，不宣称关闭 H-2」为前提。
2. **Spike 先行**：在承诺全量载体迁移前，先做可丢弃 spike 验证三件事——
   ① Windows Named Pipe 在 Rust std 模型下与现有 framing/握手循环的契合度（含
   `GetNamedPipeClientProcessId` 取 PID）；② Unix UDS + peer credential 的取用路径（Linux CI 可验证）；
   ③ 期望 Shell 可执行路径从哪里来（安装路径与 `tauri dev` 两条来源的判定规则）。
   Spike 结论决定是否进入实现期，以及 loopback TCP 是否保留为显式降级。
3. **落地形态（待 spike 后细化，本条为约束）**：
   - peer identity 校验失败必须 **fail-closed**（拒绝连接或降到最低 authority），错误可审计；
   - 只影响 daemon ↔ Shell/Surface 的本地连接，不改 envelope/capability 的 wire 语义
     （不新增 envelope 方法、不改 capability 版本）；
   - 保留显式降级路径时，降级必须在报告/日志中可见，且不得用于 `shell_control` 判定。
4. **验收即状态变更**：只有 B2（或带内核保证的实现）落地并有「同用户另一进程冒充被拒」的
   负向测试后，H-2 的状态才可以从「部分收敛」改为「已关闭」；ADR-0021 决策 5 的约束随之解除。

## 备选方案（已被否决或降级）

- **A. 握手携带身份声明（凭据绑定身份）**：**否决为 H-2 的解**——它只是把自报提前到握手，
  同 token 的攻击者照样自称 Shell；可作为 B2 落地后的补充（自报字段降级为 claim）。
- **D. 取消 broker-relaxed 路径（Shell 也走普通 broker 协商）**：**否决**——不解决冒充问题
  （冒充者仍可抢先协商成为 owner，反而把 Shell 挤出去），且触及 ADR-0014/0018 的既定语义。

## 后果

- `crates/local-transport`：新增载体与 peer identity 取用接口（公开行为变化 → 更新 module 契约、
  AGENTS 约束与 fixture 计划）；ADR-0007 的「loopback fallback」决策在本 ADR 落地后收窄
  （在 ADR-0007 中记录 superseded-by 关系）。
- `crates/daemon`：握手阶段增加 peer 校验与审计记录；`shell_control` 判定从「自报 + 凭据」
  升级为「自报 + 凭据 + 内核身份」。
- 发布面：载体变化影响打包与测试基建（live QA 脚本、split_brain 端口语义），需在实现期同步。

## 验证门禁

- Spike 报告：三件事的结论与可复现步骤（含 Windows Named Pipe 的 PID 获取证据）。
- 负向测试（实现期）：同一用户下的另一个进程（脚本模拟）持有有效 credential 连接 → **被拒**或
  只能获得最低 authority；Shell 正常连接不受影响。
- 正向回归：daemon/shell 全链路（含 live-daemon-qa、split_brain、credential_reissue）三平台绿。
- 反向证据：在 B2 落地前，本文档与 tracking 中 H-2 的状态**只能**是「部分收敛」。

## 受影响模块

- `MOD-LOCAL-TRANSPORT`（主落点：载体 + peer identity）
- daemon（握手校验与审计；MOD-PROCESS-MANAGER 视角）
- 关联：ADR-0007（载体决策，落地后收窄）、ADR-0021（决策 5 的承接）、ADR-0023（手势闸门的
  「issuer 身份有界」前提同样依赖本 ADR）

## 进展注记（2026-09-12）

- Spike（Windows 半）已完成：`docs/research/SPIKE-PEER-IDENTITY-20260912.md` —— Named Pipe + 现有 framing 兼容、`GetNamedPipeClientProcessId` + 镜像路径解析可用、客户端为独立进程可区分。Unix 半（UDS + SO_PEERCRED）待 CI 矩阵验证。决策 2 的 ①③ 已获证据，② 待补。

## 进展注记（2026-09-13）：决策 4 条件满足 —— H-2 关闭

- **载体与身份（决策 1/2 的 B2 方向全部落地）**：Windows named pipe（`GetNamedPipeClientProcessId` + 镜像路径）与
  Unix domain socket（Linux `SO_PEERCRED` + `/proc/<pid>/exe`；macOS `getsockopt(LOCAL_PEERPID)` + `proc_pidpath`）
  都在默认路径上工作；其他 Unix 返回 typed `Unsupported` 并 fail-closed。loopback TCP 保持显式降级：
  内核对端身份不可得，因此**永远不能**获得控制面（决策 3）。
- **负向测试（决策 4 的直接条件）**：`crates/daemon/tests/peer_identity_carrier.rs` 的正/负两条腿在
  **三个平台**的 CI 矩阵上运行——同用户进程、有效一次性凭据、自称 Shell、镜像不匹配 → 无 grant、
  dispatch `Unauthorized`；镜像匹配 → 保留 `ShellControl`。
- **daemon 默认姿态**：三平台默认 `strict`（Unix 的平台闸门已移除）；`strict` 而载体未挂载时启动即显式告警。
- **live QA**：`scripts/qa/live-daemon-qa.mjs` 的载体腿 A19/A20（载体连接携带内核身份；非 Shell 镜像的 Shell
  声明被判为 `Participant`）与对照腿 B8（真 Shell 在同一 daemon、同一载体上拿到 `ShellControl`，且 `peer=pipe:|uds:`
  证明走的是载体而非 TCP）。
- **证据**：CI run `34729017674`（ubuntu + macos + windows 矩阵 + live-qa-windows 全绿）；
  另有本地证据——Linux 真跑（WSL：全部 UDS 测试通过，SO_PEERCRED 与 `/proc/<pid>/exe` 实测）、
  Windows 全量套件 + live QA 28/28。
- **结论**：H-2 由「部分收敛」转为**已关闭**，ADR-0021 决策 5 的约束随之解除；ADR-0021 的 2026-09-12 接受记录
  （「H-2 未关闭」）保留为历史状态，不原地改写。
  审计原文 `docs/audits/audit-summary-2026-09-10.md` 不在本仓库（外部引用，无法原地编辑），
  因此关闭记录落在本 ADR 与 `docs/security/IPC_SECURITY.md`。

## 参考

- `docs/decisions/ADR-0021-shell-daemon-identity-binding.md`（决策 5 与备选方案 B）
- `docs/research/SPIKE-PEER-IDENTITY-20260912.md`（本 ADR 决策 2 的 spike）
- `docs/audits/audit-summary-2026-09-10.md` H-2
- `docs/decisions/ADR-0007-local-transport.md`（原始载体设计）
- `crates/local-transport/src/lib.rs`（carrier-agnostic framing 与扩展点注释）
