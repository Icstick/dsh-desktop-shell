# Multi-Profile 现场证据 → 行动建议（Field Evidence for Multi-Profile Planning）

> **来源**：本文来自另一会话（2026-09-02）对一套真实 Hermes 多部门 agent 生产实例
> （「罗德岛」：1 主实例 + 7 个 profile）的**设计 + 实测**静态分析。
> **本文不修改本仓库任何现有契约**（不改 tracking/、specs/、ADR、PLAN-B2 草案），
> 仅供本项目会话在规划多 profile（B2 并发多 profile → M9/M10）时取用。
> **完整素材**（908 行、26 张表、坑位表 20 条、§8 实测负载）：
> `D:\DSH_workspace\docs\multi-profile-reference.md`（下文简称「reference」）。
>
> **与仓库现状的挂钩点**（2026-09-02 快照）：
> - B1 单活跃切换已完成（M7，main @ e629c7f）；M8 Stable Candidate 已合并 main @ 4c21489（M8-E 待办）。
> - B2 并发多 profile 规划草案 = `docs/roadmap/PLAN-B2-MULTI-PROFILE-CONCURRENT.md`（ADR-0020 草案，
>   定位 M9 = supervisor per-environment 改造 + 端口分配器；M10 = 多 surface tab）。
> - 已有契约与本建议直接相关：`IF-SCHEDULE-WAKE`（wake/cancel/repeat，owner MOD-SUPERVISOR）、
>   `crates/daemon/src/scheduler.rs`、`crates/managed-runtime/src/supervisor.rs`（单 process 槽位）、
>   `MOD-ENVIRONMENT-SETTINGS`（catalog / discover_profiles / set_active_environment）、
>   `MOD-USAGE-COLLECTOR`（IF-USAGE）、`MOD-RUNTIME-DIAGNOSTICS`（attached-health-report schema 已有）、
>   `specs/config/environment-catalog.schema.json`。
>
> **阅读约定**：每条建议 = **建议 / 依据（reference 章节号 + 实测数字）/ 落点（本仓库模块·接口·建议工作项）/ 验收判据 / 优先级**。
> 优先级：**P0** = 进入 B2 规划前必须采纳（否则会把 reference 的坑原样搬进本项目）；**P1** = B2 范围内应包含；**P2** = 后续可选。
> 文中的「WI-M9-*」为**建议命名**的草案工作项，正式编号由项目会话按 tracking 规则创建。

---

## 0. 一句话结论

reference §0/§8.6：这套系统的**协议层设计（便宜门控、健康上报、质量门控、技能注册表）值得借鉴**；
它的**配置分发与资产分发方式（整份 clone、全树复制、协议文本内联）是必须避开的坑**——60% 的运维复杂度来自
「同一份事实被复制到 N 个地方」。实测负载证明：**设计完整度远高于实际使用**——91.8% 的会话集中在 1 个 profile，
6 个部门 profile 在备份前三周已集体停摆；真正撑起自动化的是
`cron 触发 → 门控过滤 → terminal 执行 → 技能按需加载 → kanban 流转` 这条**短链路**，而不是多部门编制本身。

**对 Shell 的推论**：多 profile 的价值取决于**持续且互不相同的负载**；Shell 的职责不是「多拉几个实例」，
而是「**便宜地决定要不要拉起、拉起后看得见它睡没睡、配置上杜绝跨实例污染**」。

---

## 1. 四条核心结论 → 行动建议总览

| # | 核心结论（实测） | 关键数字 | 对应建议 |
|---|---|---|---|
| ① | **便宜门控**：退出码 0/1 的预检脚本决定是否进入 LLM 阶段 | 每 3 分钟一次的会议推进任务累计触发 **10534** 次，被门控挡成近零成本（对应 scheduler profile 会话仅 11 个）；19 个 job 中 4 个零 LLM、7 个带前置门控 | A1–A4 |
| ② | **隔离靠配置约束，不靠目录**：整份 clone 继承绝对路径 → 两个实例共享同一记忆库；稀疏配置的实例反而各自独立 | midori 674 行克隆配置原样继承主实例 `db_path`，5 个部门 54–55 行稀疏配置各自生成独立 `memory_store.db`——隔离是 **clone 的副作用**，不是设计 | B1–B5 |
| ③ | **同一事实复制到 N 处是复杂度根源**：复制得越多，一致性越难保证、使用率反而越低 | 241 个唯一技能 ≈ **946 份**磁盘副本；USER.md 复制 **7** 份；同一协议块内联进 **19** 个 cron prompt（单 prompt >1000 字符） | C1–C4 |
| ④ | **没有持续负载的 profile 就是睡着的进程**：7 个 profile 中 1 个占 91.8% 会话，其余 5 个部门 2026-08-09 同日停摆（midori 更早，06-21） | engineering 1566/1705 会话；6 个非主力合计 139 会话（8.2%）；备份前三周系统已退化成单实例 | D1–D3 |

---

## 2. 建议 A：便宜门控 —— 调度器不要盲目拉起实例（最值钱的一条）

> 依据：reference §4.2/§4.3/§8.6/§10.3。门控契约极简：**退出码 0 = 有活干，进入 LLM/实例阶段；退出码 1 = 没活干，本轮成本为 0**。
> 对桌面 Shell 比原系统更值钱：**不启动 = 不占内存、不占 CPU、不烧额度**（§10.3）。
> 预检信号应廉价且可组合（§4.3/§10.3）：目录非空、文件 mtime 晚于上次运行时间戳、队列深度 > 0、外部端点可达。

### A1｜实例级 precheck 契约（spawn 前置门）
- **建议**：在 supervisor 的 start 路径前增加「precheck」步骤，契约即退出码：0 = 继续 spawn；1 = 不启动本轮成本为 0；非 0 视为失败并记录。precheck 是**纯 Rust/纯脚本、零 LLM、毫秒级**。
- **依据**：§4.3——10534 次 tick 靠该机制挡在 LLM 之外；§10.3——「调度器不该盲目拉起实例」是 reference 对 Shell 最有直接价值的一条。
- **落点**：`crates/managed-runtime/src/supervisor.rs`（spawn 前挂点）+ MOD-SUPERVISOR；建议工作项 **WI-M9-PRECHECK-GATE**（并入 B2 的 M9 后端改造）。
- **验收判据**：① precheck 退出 1 时 supervisor 不创建子进程（单测断言 spawn 调用计数 = 0）；② 退出 0 正常 spawn；③ precheck 超时/异常按「不启动 + 记录原因」处理（fail-closed，与 ADR-0013 restart 语义一致）；④ 每次 tick 有 skipped/executed 计数落库。
- **优先级**：**P0**

### A2｜把 precheck 接进现有调度唤醒链路
- **建议**：`IF-SCHEDULE-WAKE`（wake/cancel，含 repeat 周期唤醒）**触发时先跑目标 profile 的 precheck**，命中 1 则本轮不发 wake、不 spawn，仅累计 skipped 计数。不要在 daemon scheduler 里做「到点必起」。
- **依据**：§4.3——门控把「每 3 分钟触发 10534 次」变成近零成本；本项目已有 `crates/daemon/src/scheduler.rs` + `scheduler_wake.rs` 测试 + repeat 扩展（M6-D），增量最小。
- **落点**：`crates/daemon/src/scheduler.rs` + `specs/protocol/schedule-wake-capability.schema.json`（如需要，precheck 结果作为 wake 请求的可选字段，**新增字段必须选填**——§4.4 前向兼容规则）；MOD-SUPERVISOR。
- **验收判据**：`scheduler_wake.rs` 新增用例：repeat wake 命中 precheck=1 时不 spawn 且 skipped+1；命中 0 时正常 wake；schema fixture 覆盖「带 precheck 结果 / 缺省」正反例。
- **优先级**：**P0**

### A3｜门控命中率成为一等指标
- **建议**：skipped / (skipped + executed) 进入用量上报与 UI，作为「省了多少钱」的直接观感指标；每 profile 一栏。
- **依据**：§10.7——「门控命中率（跳过 vs 执行的比例）直接反映省了多少钱」；§8.5 反例——成本分层做了却因用量表缺列而无法核算，**成本必须可观测**。
- **落点**：MOD-USAGE-COLLECTOR（IF-USAGE usage-record 扩展，选填字段）+ MOD-SHELL-UI（EnvironmentList 卡片增加「门控命中率 / 上次运行 / 退出码」）。
- **验收判据**：usage-record schema 含 precheck 计数（正反 fixture 过 validate-specs）；UI 对每个 profile 渲染命中率；缺省值兼容旧记录。
- **优先级**：**P1**

### A4｜预检信号清单做成可组合约定（先不做引擎）
- **建议**：B2 首版不造「门控 DSL」，只约定「profile 可声明 N 个廉价信号检查（目录非空 / mtime 增量 / 队列深度 / 端点可达），任一命中即放行」，每个 profile 一个可执行文件或声明式清单。
- **依据**：§4.3——人事部门控按顺序检查 4 个廉价信号，全部不命中才退出 1；§11——「bash 门控脚本」这种分发成本近零的机制活得最好。
- **落点**：MOD-ENVIRONMENT-SETTINGS（catalog 中 profile 条目增加可选 `precheck` 声明）+ A1 的 WI-M9-PRECHECK-GATE。
- **验收判据**：两个示例 precheck（目录非空、mtime 增量）通过单测；文档写明信号语义与超时约定。
- **优先级**：**P2**

---

## 3. 建议 B：隔离靠配置约束，不靠目录

> 依据：reference §2.3（三个真实破口：绝对路径写回主实例库 / 共享后端命名空间逐字相同 / USER.md 复制 7 份）、
> §3.2（稀疏 54–55 行 vs 克隆 674 行，克隆永久冻结在克隆时刻）、§9.1 坑 1/2/3、§10.1。
> **核心教训：给每个实例一个目录 ≠ 隔离。隔离必须由「配置里没有跨实例指针 + 命名空间由 Shell 注入」保证。**

### B1｜base + override 合成，禁止整份 clone
- **建议**：Shell 持有唯一基线（随版本升级），新建 profile = 生成**空 override**（目标 20–60 行），绝不复制基线。spawn 前合成 base + override。
- **依据**：§3.2/§9.1 坑 1——克隆配置永久冻结 + 继承绝对路径 + 安全基线漂移（命令确认清单 8 vs 11 条）；稀疏配置能随升级自动获得新默认值。
- **落点**：MOD-ENVIRONMENT-SETTINGS（catalog 结构：`base + profiles/<id>/override`）；建议工作项并入 **WI-M9-CONFIG-SYNTHESIS**；specs/config/environment-catalog.schema.json 增补 override 语义。
- **验收判据**：① 新建 profile 不产生任何基线字段副本（fixture 断言 override 只含差异）；② 合成结果可复现（同一 base+override 两次合成逐字节相同）；③ validate-specs 门禁覆盖。
- **优先级**：**P0**

### B2｜override 禁止绝对路径，路径由 Shell 注入
- **建议**：override 里只允许相对路径或占位符（如 `{PROFILE_ROOT}`）；Rust 侧 spawn 时解析为绝对路径写入环境变量，**不允许从配置继承任何跨 profile 绝对路径**。
- **依据**：§2.3 破口 ①——midori 继承 `db_path` 指向主实例库；§9.1 坑 2/坑 4——可克隆配置写绝对路径、环境变量缺失导致静默降级（门控永远判空、整条流程空转）。§10.1——「这一条直接消灭坑 #2 和 #4」。
- **落点**：`crates/managed-runtime/src/environment.rs`（合成与校验）+ MOD-ENVIRONMENT-SETTINGS + specs/config/dsh-environment.schema.json（占位符白名单）。
- **验收判据**：① override 含未声明占位符或绝对路径 → 合成校验失败、**拒绝启动**并返回结构化错误（不静默改写）；② 两个 profile 的 `PROFILE_ROOT` 解析结果不同且各属自身目录；③ 单测覆盖「占位符缺失 / 越权路径 / 相对路径越界」三反例。
- **优先级**：**P0**

### B3｜共享后端的命名空间标识由 Shell 生成，不可被 clone 继承
- **建议**：凡指向共享后端（记忆库、消息总线、任何外部存储）的命名空间标识（user_id/agent_id/app_id 之类）由 Shell 在创建 profile 时**生成唯一值并注入**；catalog schema 强制必填、禁默认空/禁用户填相同值。
- **依据**：§2.3 破口 ②——主实例与 midori 三个标识逐字相同，「即使换了物理库，逻辑命名空间也是同一个」。
- **落点**：MOD-ENVIRONMENT-SETTINGS（catalog 增命名空间字段）+ 未来 M10 多实例时的 adapter 侧注入（`crates/adapter-dsh`）。
- **验收判据**：单测断言任意两个 profile 的生成标识互不相等且可复现（幂等读取）；schema 拒绝缺失/重复命名空间。
- **优先级**：**P1**

### B4｜合成后 schema 强校验，失败拒绝启动
- **建议**：spawn 前对合成配置做 JSON Schema 校验；失败 = 拒绝启动 + 明确报错。**不要让实例带病运行**。
- **依据**：§10.1——「合成后再校验，失败就拒绝启动并明确报错」；§9.1 坑 8/9——同名 key 重复、YAML 塞 JSON 字符串都是「宽松解析器吞掉问题」的后果。
- **落点**：MOD-ENVIRONMENT-SETTINGS + specs/config/（校验器复用现有 validate-specs 门禁模式）。
- **验收判据**：合成失败路径有结构化错误码与 UI 文案；校验器对「重复 key / 未知字段 / 类型错误」各有一个 fixture。
- **优先级**：**P0**

### B5｜配置版本号必须配迁移器
- **建议**：catalog/override 的 `config_version` 每升一版必须有一条 `migrate(v_n → v_n+1)` 函数链；无迁移器的版本号只是装饰（reference 的 `_config_version: 33` 即此）。
- **依据**：§3.4（有版本号没有迁移器）/§9.1 坑 13/§11「有 `_config_version` 却没有迁移器」。
- **落点**：MOD-ENVIRONMENT-SETTINGS。
- **验收判据**：迁移链单测：v1 → v3 连续迁移后数据不丢；无迁移路径的版本差被拒绝并提示。
- **优先级**：**P2**

---

## 4. 建议 C：单一事实源 —— 引用，不要复制

> 依据：reference §5.4/§5.5（协议块被设计成可复用片段、却内联进 19 个 prompt）、§6（USER.md 7 份客观事实逐字重复）、
> §8.3（复制最多的派遣学说对应工具仅调用 3 次）、§8.5（技能引用失效静默跳过）、§10.4（引用模型下漂移不可能发生）。

### C1｜技能单一仓库 + 每 profile 一份 enable 清单；引用失效硬失败
- **建议**：Shell 不为 profile 复制技能文件；profile 只声明「启用哪些技能」。**启动预检遍历 enable 清单，任一引用缺失/失效 → 拒绝启动**（错误码 `SKILL_MISSING`），绝不静默跳过。
- **依据**：§2.1/§8.5——241 唯一技能在归档中实体化约 946 份，软链失效后框架只打一行警告继续跑（相关警告词频 16 次），任务带病运行；§11.1——「技能引用在启动时校验并硬失败」是可借鉴项。
- **落点**：启动预检挂在 A1 的 precheck 阶段（`crates/managed-runtime`）；enable 清单声明在 catalog override（MOD-ENVIRONMENT-SETTINGS）；建议并入 **WI-M9-PRECHECK-GATE**。
- **验收判据**：① 删除某启用技能 → 预检失败、实例不启动、错误含技能名；② 不存在「只打警告继续跑」的路径（代码审查 + 单测断言）；③ 参考系统 946 份副本的场景在本项目不可能出现（无复制机制）。
- **优先级**：**P1**（若 B2 首版就要管技能，升 P0；否则随 precheck 一起做）

### C2｜协议块注册为具名片段，按 id 注入，禁止内联
- **建议**：凡需注入多实例的系统提示/协议文本，注册为**具名片段**（id + 版本），组装时按 id 引用；override/catalog 中禁止 >500 字符的内联协议文本。
- **依据**：§5.4/§5.5——协议块文件库存在却内联进 19 个 job 的 prompt（单 prompt >1000 字符，同一段复制 5–6 次），「协议改一个字 → 改 N 个 job + N 个 SOUL.md，且没有一致性校验」；§8.3——被复制最多的指令（派遣学说）对应工具仅调用 3 次。
- **落点**：片段注册表属 DSH 侧 prompt 组装能力，Shell 侧现阶段只做**约束**（B4 校验器检查内联长度阈值）；完整能力建议作为独立 WI（M9/M10 之后）。
- **验收判据**：校验器拒绝含超长内联文本的 override（fixture）；未来片段机制交付时按 id 注入且有版本一致性测试。
- **优先级**：**P1**（约束部分）/ **P2**（注册表能力）

### C3｜用户画像分层：共享 facts 层 + 每 profile 渲染层
- **建议**：如果未来 Shell 管理多 profile 的记忆资产，用户客观事实放共享层，各 profile 只持有「视角渲染」；改一处不用改 N 处。B2 首版不实现，只记录决策意图。
- **依据**：§2.3 破口 ③/§6——客观事实逐字复制进 7 份 USER.md，「改一条客观事实要改 7 处」；§11「共享事实 + 各自渲染」为可借鉴项。
- **落点**：决策记录（建议进 PLAN-B2 或 ADR-0020 补充）；无代码落点（P2）。
- **验收判据**：无（决策意图记录）。
- **优先级**：**P2**

### C4｜保留技能注册表的优质字段，但 hash 只用于外部导入校验
- **建议**：若 B2 引入技能清单，保留 `owner / version / hash / dependency / security.classification / last_audited / next_audit_due` 与五维分类标签；在引用模型下，hash 仅用于校验**外部导入**，不做副本漂移检测（漂移在引用模型下不可能发生）。
- **依据**：§4.7/§10.4——注册表字段设计本身值得借鉴（外部资产溯源做得比多数生产系统好）；副本 hash 追踪是「混用软链+实体副本」的补救措施，引用模型下不需要。
- **落点**：未来 WI（技能资产管理）；MOD-CAPABILITY-CONTRACTS 可作参照。
- **验收判据**：无（随 C1/C2 交付）。
- **优先级**：**P2**

---

## 5. 建议 D：按实际负载排优先级 —— 睡着的 profile 要看得见、能下线

> 依据：reference §8.1/§8.2/§8.6（负载高度集中 + 集体停摆；「多 profile 的价值取决于持续且互不相同的负载」）、
> §4.4（健康协议 5/7 采纳、「没有强制机制——这正是 Shell 应该补的位置」）、§10.7（负载分布视图 + 零会话提示下线）。

### D1｜负载分布视图 + 零会话下线提示
- **建议**：UI 提供每 profile 的「近 7/30 天会话数、运行次数、最近活动时间」；某 profile 连续 N 天零会话（N 可配置，默认如 14 天）→ 主动提示「该 profile 该合并或下线了」。
- **依据**：§10.7——「如果某个 profile 连续 N 天零会话，Shell 应主动提示」；§8.1——reference 正是缺了这一步才养出 6 个睡着的 profile（合计 139 会话，8.2%）。
- **落点**：MOD-USAGE-COLLECTOR（数据）+ MOD-SHELL-UI（EnvironmentList 扩展 + 提示条）。
- **验收判据**：① UI 渲染每 profile 负载三指标；② 构造「零会话超阈值」catalog 数据 → UI 出现下线提示；③ 阈值可配置并持久化。
- **优先级**：**P1**

### D2｜健康上报由 Shell 强制，补齐「协议采纳靠自觉」缺口
- **建议**：复用/扩展现有 `attached-health-report` schema 与 MOD-RUNTIME-DIAGNOSTICS：managed 实例到期未上报健康 → UI 标黄/标红；上报缺失本身是事件（可审计）。
- **依据**：§4.4——健康协议 v1.1 设计完整（单文件覆盖写、版本号、统一退出码、v1.x 新增字段必须选填），但 7 个部门只接了 5 个，采购与调度长期 NOT REPORTING；「协议写得好，但没有强制机制——这正是 Shell 应该补的位置」。
- **落点**：MOD-RUNTIME-DIAGNOSTICS + MOD-SUPERVISOR（managed 侧）+ MOD-SHELL-UI（状态徽标）。
- **验收判据**：① 模拟实例停止上报 → 超过阈值后 UI 状态从绿变黄/红；② 上报缺失记录可查（诊断面板）；③ schema 新增字段均为选填（前向兼容测试）。
- **优先级**：**P1**

### D3｜先有负载再开 profile（向导加「首个用例」门）
- **建议**：SetupWizard 新建 profile 时要求填写「这个 profile 的第一个持续负载是什么」（定时任务/目录监听/消息源），没有用例则提示「该 profile 大概率会睡着」；允许跳过但记录。
- **依据**：§8.1/§8.6/§11.1——「先有负载再开 profile」，reference 是先铺满编制再找活干，结果 6 个睡着的 profile 各自占 95–149 份技能副本与审计面。
- **落点**：MOD-ENVIRONMENT-SETTINGS + MOD-SHELL-UI（SetupWizard 步骤扩展）。
- **验收判据**：向导新建流程含「负载声明」输入且持久化到 catalog；跳过时有显式确认。
- **优先级**：**P2**

---

## 6. 三条实测反例 —— 决策护栏（B2 规划时必须对照）

| # | 反例（实测） | 对本项目的护栏建议 | 落点 | 验收判据 | 优先级 |
|---|---|---|---|---|---|
| R1 | **IM 总线只承载 0.3% 会话却配了 8 套 Bot/Token**（telegram 4 次会话 / 1566，§8.2；8 个 Bot 独立 Token，§2.1） | **不要为每个 profile 配独立 IM 通道**。通知通道按需共享或后置；B2 多实例的「人类入口」仍是唯一前台 profile（星形拓扑，§10.5） | M10 多 surface 设计评审；PLAN-B2 决策记录 | B2 设计文档明确「无 per-profile IM Bot」决策与理由；不新增任何 Bot 凭据类配置字段 | **P0**（设计约束） |
| R2 | **被复制最多的派遣指令对应的 `delegate_task` 只被调用 3 次（0.02%）**——分发成本最高的部分使用率最低（§8.3/§5.5） | 任何「批量注入/批量复制」机制上线前先回答「谁在用、多久用一次」；未验证使用率的功能不做 N 份复制。Shell 侧即：profile 模板只放**实测在用的**资产 | MOD-ENVIRONMENT-SETTINGS（profile 模板）+ B1 的 override 机制 | 模板资产清单与使用率数据（D1 的 usage 上报）可对照审查；无「一次性注入 N 份」的创建路径 | **P1** |
| R3 | **技能软链失效后框架只打一行警告继续跑**（cron 声明技能缺失被静默跳过，警告词频 16 次，任务带病运行，§8.5/§9.1 坑 18） | **引用失效必须硬失败**：启动预检发现启用清单中任一技能缺失 → 拒绝启动（与 C1 同一落点） | A1 的 precheck（WI-M9-PRECHECK-GATE） | 单测：缺失技能 → 预检失败且无「警告后继续」路径；错误含技能名与建议动作 | **P1** |

---

## 7. 与 PLAN-B2 草案的对应（给 B2 规划会话的挂钩点）

| PLAN-B2 草案条目 | 本建议的补充/修正 | 关键性 |
|---|---|---|
| 决策 1：per-environment supervisor 状态表（`active: HashMap<environment_id, ManagedProcess>`） | spawn 前插入 **A1 precheck**；「后台运行集」与「前台 profile」分离的语义正好支撑 D1 的负载视图 | 建议纳入 M9 范围 |
| 决策 2：多 surface（A tab / B 多窗口） | 对照 **R1**：人类入口唯一（星形拓扑），其余 surface 只做通知/后台 | 设计评审时对照 |
| 决策 3：端口分配器（40000–41000） | 端口号由 Shell 注入环境变量（**B2** 的「路径/端口由 Shell 注入」同类原则），override 不得写死端口 | 与 B2 建议合并验收 |
| 决策 4：catalog 单文件共享（已验证） | catalog 是「配置真源」，要承担 B1（base+override）、B3（命名空间）、B4（合成校验）的载体职责 | schema 变更走现有 specs 门禁 |
| 风险表「多 DSH 会话磁盘/资源竞争 = 低（profile 天然隔离）」 | **该风险等级判断需要修正**：reference §2.3 证明「目录天然隔离」是假象，隔离必须靠配置约束（B2/B3）——建议把此风险拆成「配置级隔离失效（高）」 | 风险表更新 |
| IF-SCHEDULE-WAKE（已有 wake/cancel/repeat） | 增加 precheck 语义（**A2**）：到点先预检再 wake；「repeat 到点必起」在 B2 里不成立 | 契约扩展走 ADR |

另建议：B2 规划时新增一条 ADR 决策点——**「多 profile 的启动策略 = 门控驱动，而非到点驱动」**（A1/A2 的决策依据在此文档），并把 §6 三条反例作为「不做什么」的约束写进 PLAN-B2。

---

## 8. 如果只做三件事（ROI 排序）

| 序 | 做 | 为什么排这里 | 代价量级 | 收益依据 |
|---|---|---|---|---|
| 1 | **便宜门控 precheck（A1 + A2）** | 增量最小（现有 scheduler + IF-SCHEDULE-WAKE 骨架）、收益最大（参考系统 10534 次 tick 近零成本；桌面侧不启动 = 不占内存/CPU/额度）、可测性最强 | 小（supervisor 挂点 + scheduler 接线 + 单测） | §4.3/§10.3 |
| 2 | **配置合成 + 绝对路径禁止 + 合成校验（B1 + B2 + B4）** | 直接消灭参考系统最贵的坑（克隆继承绝对路径 → 跨实例写库、配置永久冻结、安全基线漂移）；这是 spawn 前最后一道安全门，B2 一开工就会踩到 | 中（catalog 结构改造 + schema + 校验器） | §2.3/§3.2/§9.1 坑 1/2 |
| 3 | **引用失效硬失败（C1 最小版：enable 清单 + 启动预检拒绝）** | 实现最便宜（预检遍历 + 拒绝）、消灭真实发生的静默降级（警告词频 16 次、任务带病运行）；且它直接复用第 1 件的 precheck 通道 | 小（随 A1 一并交付） | §8.5/§9.1 坑 18 |

> 如果三件事做完还有预算，第四件做 **D1（负载分布视图 + 零会话下线提示）**：它是唯一能防止「养出 6 个睡着的 profile」的机制（§10.7/§8.1），但依赖 usage 数据积累，收益延后，所以排在后面。
> 反例护栏 R1（不配 per-profile IM Bot）**不占实现预算**，只是设计评审时的一条红线，必须进 B2 决策记录。

---

## 附录：关键实测数字速查（摘自 reference §8 / 附录 B）

| 指标 | 数值 | 用在哪条建议 |
|---|---|---|
| 实例总数 | 8（1 编排 + 6 部门 + 1 辅助） | 背景 |
| 部门 profile 配置行数（稀疏） | 54–55 行 | B1 |
| 克隆 profile 配置行数（midori） | 674 行（与主实例 545 行相同 / 132 行不同） | B1/B2 |
| 定时任务 19 个中：零 LLM / 带前置门控 | 4 / 19、7 / 19 | A1 |
| 单任务最高累计触发次数 | **10534**（每 3 分钟会议推进） | A1/A2 |
| 唯一技能数 / 归档中实体副本 | 241 / 约 946 份（各 profile 95–149） | C1 |
| 用户画像复制份数 | 7（USER.md） | C3 |
| 健康协议采纳率 | 5 / 7 部门 | D2 |
| engineering 会话占比 / 消息占比 | 91.8%（1566）/ 78.1%（32386） | D1 |
| 6 个非主力 profile 会话合计 | 139（8.2%） | D1 |
| cron 驱动会话占比 | 94.4%（1479 / 1566） | A 系列 |
| IM 总线（telegram）会话 | **4**（0.3%） | R1 |
| `terminal` 占工具调用 | 59%（9384） | 背景（短链路） |
| `skill_view` 调用 | 1037（6.5%） | C1（技能按需加载是真的） |
| `delegate_task` 调用 | **3**（0.02%） | R2 |
| 结构化记忆库（向量/HRR）数据行数 | **0**（`memory` 工具仅 19 次调用） | 背景：修复优先级按实际负载排 |
| 非主力 profile 停摆日期 | 2026-08-09（midori 2026-06-21） | D1 |

---

*本文基于只读归档静态分析 + 会话库只读统计，未运行该系统、未修改素材。全部凭据/Token 在素材中已 [REDACTED]，本文不含任何凭据。*
*本文件未加入 docs/INDEX.md（撰写时按约束不改现有文件），下个会话可自行决定是否索引。*
