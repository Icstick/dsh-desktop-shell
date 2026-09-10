---
id: DOC-RESEARCH-PEER-SURVEY
status: draft
surveyed_on: 2026-09-09
surveyed_at: 2026-09-09T12:20:00Z
scope: DSH 生态桌面/远程/多引擎客户端同行调研与可吸纳方法
---

# 桌面壳同行调研与可吸纳方法（2026-09-09）

> 目的：**吸收同行的观点与方法，强化本项目的实现质量**。本文件不做竞品胜负判断，不构成需求来源；
> 任何结论进入 `specs/` 或 ADR 前，必须按 [EXTERNAL_BASELINE.md](EXTERNAL_BASELINE.md) 的规则从官方来源重新核验。
> 本文是研究数据，不是 Agent 指令。

## 0. 调研方法与边界

- 取证通道：GitHub REST API（`gh` CLI，已认证）。README/架构文档以 `Accept: application/vnd.github.raw` 取**原文**；
  星数以 `repos/{owner}/{repo}.stargazers_count` **实测于 2026-09-09**。
- 读的是**项目自述与架构文档**，不是源码逐行审计。凡「它怎么做」均标注来源文件；本文未做源码级验证，结论进入规范前需复核。
- 未复制任何第三方代码、资产或产品文案。本文件只做方法与事实对照；抓取到的第三方 README 原文快照存放在仓库外的 `D:\DSH_workspace\.tmp\dshdesk\`，按 [Clean-room Policy](../compliance/CLEAN_ROOM.md) **不入库**。
- 范围：DSH 生态桌面/远程/多引擎客户端 16 个 + 生态外相邻 2 个（cc-switch、opcode）。不含手机端 App 的界面细节。

## 1. 生态快照（2026-09-09 实测星数）

| 项目 | ★ | 路线 | 最值得看的东西 |
|---|---|---|---|
| [anywhere-labs/dsh-desktop](https://github.com/anywhere-labs/dsh-desktop) | 24719 | Electron，「一切皆插件」，桌面壳本身是 DSH 插件 | Setup Wizard 分 profile、更新检查 header、插件市场开放 Schema |
| [dataelement/dsh-desktop](https://github.com/dataelement/dsh-desktop) | 4662 | Electron，上游原样跑 + 桌面补丁层 | **Safe Mode、插件故障归属诊断、更新节奏、IPC 校验** |
| [dsh-tauri-desk/deepseek-harness-desktop](https://github.com/dsh-tauri-desk/deepseek-harness-desktop) | 1829 | Tauri，5MB 安装包零环境 | 多版本内核管理、健康检查 task、不可达时保本地 |
| [zouyuxuan122/DSH-Desktop-EAC](https://github.com/zouyuxuan122/DSH-Desktop-EAC) | 1605 | Tauri + Node sidecar 三层壳 | **三层边界、Plugin Guard、自更新冒烟、CI 方法** |
| [zhukunpenglinyutong/desktop-cc-gui](https://github.com/zhukunpenglinyutong/desktop-cc-gui) | 4184 | Tauri 多引擎 | **「adopt 运行中的 dsh web 或自己启动」、Context Ledger** |
| [winfunc/opcode](https://github.com/winfunc/opcode) | 22396 | Claude Code GUI | 检查点/时间线/fork、后台 agent |
| [zenolab124/monet](https://github.com/zenolab124/monet) | 279 | 多引擎指挥台 | **引擎中心、文件账本、审批卡片、交付物隔离预览** |
| [Buzzso/dsh-sev](https://github.com/Buzzso/dsh-sev) | 138 | 远程主机管理 | **会话混排、隧道自愈、loopback fence、配置滚动备份** |
| [Asaiuta/dsh-session-hub](https://github.com/Asaiuta/dsh-session-hub) | 4 | 多服务器会话聚合 | 「插件只搬数据不画界面」 |
| [Blank-not-black/dsh-Remote](https://github.com/Blank-not-black/dsh-Remote) | 33 | 插件/网关/客户端三件套 | 多服务器自动选优、离线可看历史 |
| [farion1231/cc-switch](https://github.com/farion1231/cc-switch) | 131905 | 多 CLI 配置中枢 | **SSOT + 双层存储 + 原子写 + 双向同步** |
| [vibeinging/dsh-desktop](https://github.com/vibeinging/dsh-desktop) | 584 | 插件集成版（Profile Bundles） | 客户端目录与数据目录分离、发布证据流水线 |
| [lencx/Minke](https://github.com/lencx/Minke) | 630 | 本地优先工作区 | Agent 浏览器共享控制、远程接入选项 |
| [myYangyunfan/dsh_desktop](https://github.com/myYangyunfan/dsh_desktop) | 642 | Windows 客户端，内置 Node + dsh CLI | 一键启动形态 |
| [shaobeichen/dsh-pocket](https://github.com/shaobeichen/dsh-pocket) | 1034 | 手机同屏 | 扫码配对 |
| [slywalker2006/dsh-passwords](https://github.com/slywalker2006/dsh-passwords) | 46 | 服务端多租户网关 | 子用户权限与配额、沙箱强制、审计日志 |

## 2. 同行独立验证了我们哪些决策

| 我们的决策 | 同行证据 | 结论 |
|---|---|---|
| Managed/Attached 显式分权（ADR-0002） | cc-gui：「can **adopt a running local \`dsh web\` host or start one**」 | 路线被独立走到，**不是孤例** |
| 不 fork 上游 UI（ADR-0004） | session-hub：「插件只搬数据，不画界面」 | 边界正确 |
| 不拥有/不分发 Core（INVARIANTS #1） | monet：「原工具数据架构级只读，增值数据独立存放 \`~/.monet/\`」 | 数据主权是同向设计 |
| Local transport 默认不可 LAN 访问（INVARIANTS #13） | dsh-sev：「remote host only listens on \`127.0.0.1\` … loopback-only API fence」 | 默认拒绝是对的 |

## 3. 可吸纳的方法

> 每条格式：**它怎么做** → **我们现状**（引用本仓库文件）→ **建议**。

### 🔴 P0-1 Safe Mode：非破坏性隔离恢复

- **出处**：[dataelement/dsh-desktop](https://github.com/dataelement/dsh-desktop) `docs/architecture.md`。
- **它怎么做**：故障时启动一个**独立的官方核心 profile**，Agent 与用户数据照常可用；用户在里面逐个移除第三方插件后再回正常 profile。**非破坏性**——不改动正常 profile。相邻项目 [dsh-safe-tui](https://github.com/aorucshiea/dsh-safe-tui)、[dsh-rescue-bootloader](https://github.com/Mauit06/dsh-rescue-bootloader) 说明这是公认痛点。
- **我们现状**：[docs/operations/RECOVERY.md](../operations/RECOVERY.md) 有 Managed Core 的 backoff + budget + Safe Stop，也明确「Desktop 不自动修 Profile」；但**没有「隔离 profile 启动」这条用户可自救路径**——当前只能切 Environment 或进 Settings/Diagnostics。
- **建议**：新增 WI，产出「Safe Mode 启动」能力：隔离 DSH_HOME（或隔离 profile）+ 只读呈现故障证据 + 插件逐个禁用的可逆操作；写 ADR 明确它**不修改**正常 profile 与 Desktop state。

### 🔴 P0-2 插件故障归属诊断链

- **出处**：[dataelement/dsh-desktop](https://github.com/dataelement/dsh-desktop) `docs/architecture.md` §Profiles and plugin recovery。
- **它怎么做**：收集日志 + 渲染器证据，然后沿 **profile manifest → lockfile → bundles → loader entry IDs → slot 冲突 → 依赖 → Cordis patch 行**逐层解析归属，最后给出**针对性动作**；破坏性变更必须用户显式确认。
- **我们现状**：`MOD-RUNTIME-DIAGNOSTICS` 与 `IF-RUNTIME-STATUS` 已提供状态面；但「从症状回溯到责任插件」的证据链没有成体系。
- **建议**：把这条链做成可复用的诊断工具（纯读），输出结构化归属结论；先只做「指认」，不自动修。

### 🔴 P0-3 多后端混排 + 隧道自愈

- **出处**：[Buzzso/dsh-sev](https://github.com/Buzzso/dsh-sev) README §Architecture。
- **它怎么做**：左侧栏**远程与本地会话混排**（标题/运行脉冲/相对时间）；SSH 隧道带 watchdog **自动重连**、重启后恢复；配置为纯 JSON + 滚动 `.bak` 备份 + 删除确认；双通道（面板看 GUI / CLI 跑一次性任务）；远程会话列表与健康经隧道从远端自身 API 拉取。
- **我们现状**：`docs/roadmap/ROADMAP-V021-PLUS.md` 把 **0.5.0 workbench Phase 2 (SSH)** 排在路线图上；`PLAN-B2-MULTI-PROFILE-CONCURRENT.md` 已规划 M9 supervisor per-environment + 端口分配器、M10 多 surface tab。
- **建议**：把「会话混排 + 隧道 watchdog + 配置滚动备份」三项并进 0.5.0 的验收条件，避免只做「能连」不做「掉线能自愈」。

### 🔴 P0-4 上下文归因（Context Ledger）

- **出处**：[zhukunpenglinyutong/desktop-cc-gui](https://github.com/zhukunpenglinyutong/desktop-cc-gui) README §Project intelligence。
- **它怎么做**：把选中/继承的上下文来源集中展示，带 **token/字符估算、新鲜度、归因置信度**。
- **我们现状**：`MOD-USAGE-COLLECTOR` / `IF-USAGE` 覆盖用量；但「每段上下文来自哪、多新、可信度多少」没有对应能力。
- **建议**：作为 0.3.0 之后的能力候选登记（先不做）；若做，走新 Capability + 独立 apiVersion，遵守 INVARIANTS #7。

### 🟠 P1-1 Plugin Guard 六件套

- **出处**：[EAC](https://github.com/zouyuxuan122/DSH-Desktop-EAC) README §目录结构（`plugin-guard.js`、`profile-module-heal.js`）。
- **它怎么做**：快照 / 回滚 / 体检 / 修复 / 守护启动 / 事故报告；另有 profile 模块遮蔽自愈（真实目录 + pnpm 链接）。
- **我们现状**：INVARIANTS #11 明确 **Plugin management 属于 DSH**，Desktop 不越界。
- **建议**：只吸收**观测与证据**部分（体检/事故报告），**不吸收**自动修复与守护启动——否则违反 #11。这条边界要写进 WI 的 security_impact。

### 🟠 P1-2 壳层独立路由页

- **出处**：[EAC](https://github.com/zouyuxuan122/DSH-Desktop-EAC) README §架构（L1 壳页 HTTP 路由）。
- **它怎么做**：壳自带 `/loading /exit /died /update /about /wizard` 等页面——**内核挂了壳还能说话**。
- **我们现状**：`MOD-SHELL-UI` 已有 Surface 与 Settings；壳级故障页未成体系。
- **建议**：补「壳存活页」清单与最小内容（版本、环境、最后错误、可用动作），纳入 0.4.0 startup rollback 一并设计。

### 🟠 P1-3 更新策略

- **出处**：[dataelement](https://github.com/dataelement/dsh-desktop) §Updates；[anywhere-labs](https://github.com/anywhere-labs/dsh-desktop) README §首次设置。
- **它怎么做**：启动后不久 + 每 6 小时 + 长休眠恢复后检查；**先提示、后下载、用户选择才安装**；可跳过单个版本但不屏蔽后续。更新检查请求只带版本/通道/本机持久随机 UUID（**明确声明非硬件推导**），下载请求不带安装 ID。
- **我们现状**：`ROADMAP-V021-PLUS.md` 已排 **0.4.0 startup rollback (ADR first)**。
- **建议**：把「检查节奏 + 跳过单版本 + 隐私最小化 header」写进该 ADR 的候选条款。

### 🟠 P1-4 启动流程标准化

- **出处**：[dataelement](https://github.com/dataelement/dsh-desktop) §Startup flow。
- **它怎么做**：单实例锁 → launch-root 中性工作目录 → 启动界面 + 检查 profile → 钉 pnpm store 并修不完整包状态 → 随机回环端口启动 → 轮询到持续健康 → 装载 → 移动桥/更新管理。
- **我们现状**：`crates/supervisor` + `crates/daemon` + `MOD-PROCESS-MANAGER` 已覆盖大部分；**「launch-root 中性工作目录」**与**「修不完整包状态」**两步值得对照。
- **建议**：做一次启动序列对照表，找出我们缺的步骤；缺的写 WI。

### 🟠 P1-5 配置滚动备份 + 原子写

- **出处**：[dsh-sev](https://github.com/Buzzso/dsh-sev)（JSON + `.bak` 滚动备份 + 删除确认）；[cc-switch](https://github.com/farion1231/cc-switch)（SSOT + 双层存储 + 原子写 temp+rename + 互斥保护 + 双向同步）。
- **我们现状**：RECOVERY.md 已声明「Desktop state 使用原子写/备份策略」。
- **建议**：核对实际实现是否覆盖「滚动多份」与「外部编辑回填」；`PLAN-POST-WIZARD` 阶段 2 的配置文件形态（D2/D3 决策）正好需要这两条。

### 🟡 P2 产品面（择机）

| 方法 | 出处 | 一句话 |
|---|---|---|
| Setup Wizard 按 profile 记录完成/跳过 | anywhere-labs | 向导完成前不启动 Host 与主窗 |
| 局域网访问显式开关 + 危险提示 + 显示真实 URL | anywhere-labs | 暴露范围是需要用户点确认的决定 |
| 引擎中心：安装/认证/版本/能力/诊断，单引擎故障不拖垮另一个 | monet | 正是 Adapter 契约要回答的 |
| 审批卡片 GUI 化：危险命令红标 + 大白话批注 + Enter/Esc | monet | 审批是最高频打断点 |
| 交付物就地预览，HTML **默认禁脚本禁联网**隔离加载 | monet | 安全与体验不冲突的样板 |
| 文件账本：会话动了哪些文件、每次 diff、跳回「为什么改」 | monet | 比「改了啥」更有价值的是「为什么」 |
| 检查点 / 时间线 / 分支 fork | opcode | 会话可回退可分叉 |
| 客户端安装目录与数据目录分离 | vibeinging | 换安装位置不迁移数据（我们已有 INVARIANTS #2 同向） |

## 4. 工程方法（可照用）

- **发布证据流水线** —— [vibeinging](https://github.com/vibeinging/dsh-desktop) `.github/workflows/windows-release-evidence.yml`：`workflow_dispatch` 手动触发、消费签名证书、在真实 Windows runner 上跑**安装器验收**、产出 `result.json` 证据 artifact；并强制 **release ref 必须等于 `v<version>` 标签**、必需 secret 缺失直接失败。
- **CI paths 负向匹配** —— [EAC](https://github.com/zouyuxuan122/DSH-Desktop-EAC) `.github/workflows/ci.yml`：跳过纯文档改动，但**随包分发的文件（SKILL.md）放行**。
- **来源台账进 CI** —— EAC 在 CI 跑 `scripts/plugin-ledger.mjs` 校验 `SOURCES.json`。我们已有 `docs/compliance/SOURCE_REGISTER.yaml`，接进 CI 即可。
- **目标原生构建** —— dataelement 与 dsh-tauri-desk 都强调：产物必须在匹配的 OS/架构上构建（原生依赖）。
- **锁版本工具链** —— EAC：`corepack disable && npm i -g pnpm@<pinned>`，避免 CI 漂移。
- **安装器 A/B 与冷样本纪律** —— [anywhere-labs](https://github.com/anywhere-labs/dsh-desktop) `docs/evidence/windows-nsis-ab-methodology.md`。要点可直接照用：
  ① A/B 两个安装器必须消费**同一份预打包应用目录**，且验证 B 真的解析到未打补丁的构建器，产出后复算整棵应用目录，变化即拒绝出 manifest；
  ② 每次 installer build 消费**单次授权 token + afterAll hook 锚定路径**，hook 未执行或 token 复用即失败；
  ③ 正式测量单位是「**准备一次 canonical 状态 → 关机 → 快照 → 每个 variant 恢复快照只测一个 case**」，避免文件系统与 Defender 缓存跨 case 污染；
  ④ manifest 记录门禁来源（命令行 / 环境变量），**不把「跳过门禁」的产物当成已验证产物**；
  ⑤ 计时前不做整包哈希，避免主动预热缓存。
- **轻量治理惯例** —— [vibeinging](https://github.com/vibeinging/dsh-desktop) 用 `docs/design/YYYY-MM-DD_topic.md` + `docs/plans/` 的日期前缀惯例；[EAC](https://github.com/zouyuxuan122/DSH-Desktop-EAC) 用 `docs/adr/`（3 篇）+ `docs/schemas/` + `docs/HANDOVER-*.md`；anywhere-labs 用 `docs/evidence/`。我们的 tracking/specs/ADR 治理比这些都重——可考虑在 `docs/testing/evidence/` 之外固定一个「安装器/升级实验」的证据目录，对齐他们的证据落点习惯。

## 5. 我们不该学的

- **不打包/不分发内核**。生态里多数项目选择内置 Node + 内核发行版，代价是跟着上游版本跑（`dsh 0.1.2-rc.1` 一变就要重发）。我们的 INVARIANTS #1 正是为躲这个。
- **不把「一切皆插件」推到桌面壳自身**。桌面能力可组合听起来优雅，但会让 trust boundary 变模糊；我们已有 Capability Broker + 明确 permission。
- **不做自动修复/守护启动**。INVARIANTS #11 把 Plugin management 划给 DSH；我们能做的是**证据与指认**。
- **不按星数跟路线**。★131905 的 cc-switch 是配置切换器，★24719 的 anywhere-labs 是 Electron 打包器——量级不同，方法可借，路线不必跟。

## 6. 落地建议（按投入产出）

1. **Safe Mode 隔离恢复** —— 用户自救的唯一出口，投入小、收益大。
2. **插件故障归属诊断链** —— 出问题能指认，而不是让人猜；只读，不越界。
3. **多后端混排 + 隧道自愈** —— 与既有 0.5.0 SSH 路线合并，做完定位即与生态错开。

草案工作项（status: proposed，未认领；编号与 milestone 由项目会话按 tracking 规则确认）：

- `tracking/work-items/WI-M9-SAFE-MODE-RECOVERY.yaml` —— P0-1
- `tracking/work-items/WI-M9-PLUGIN-FAULT-ATTRIBUTION.yaml` —— P0-2
- `tracking/work-items/WI-M10-MULTI-BACKEND-FLEET.yaml` —— P0-3

## 7. 来源清单

| 来源 | 取用内容 | 抓取日期 |
|---|---|---|
| [dataelement/dsh-desktop](https://github.com/dataelement/dsh-desktop) | README、`docs/architecture.md` | 2026-09-09 |
| [anywhere-labs/dsh-desktop](https://github.com/anywhere-labs/dsh-desktop) | README、`docs/why-desktop.md`、`docs/evidence/windows-nsis-ab-methodology.md` | 2026-09-09 |
| [zouyuxuan122/DSH-Desktop-EAC](https://github.com/zouyuxuan122/DSH-Desktop-EAC) | README、`.github/workflows/ci.yml` | 2026-09-09 |
| [dsh-tauri-desk/deepseek-harness-desktop](https://github.com/dsh-tauri-desk/deepseek-harness-desktop) | README、`.github/workflows/build-test.yml` | 2026-09-09 |
| [zhukunpenglinyutong/desktop-cc-gui](https://github.com/zhukunpenglinyutong/desktop-cc-gui) | README | 2026-09-09 |
| [zenolab124/monet](https://github.com/zenolab124/monet) | README | 2026-09-09 |
| [Buzzso/dsh-sev](https://github.com/Buzzso/dsh-sev) | README（含 Architecture） | 2026-09-09 |
| [Asaiuta/dsh-session-hub](https://github.com/Asaiuta/dsh-session-hub) | README | 2026-09-09 |
| [Blank-not-black/dsh-Remote](https://github.com/Blank-not-black/dsh-Remote) | README | 2026-09-09 |
| [farion1231/cc-switch](https://github.com/farion1231/cc-switch) | README §Design Principles | 2026-09-09 |
| [vibeinging/dsh-desktop](https://github.com/vibeinging/dsh-desktop) | README、`.github/workflows/windows-release-evidence.yml` | 2026-09-09 |
| [winfunc/opcode](https://github.com/winfunc/opcode) | README §Features | 2026-09-09 |
| [lencx/Minke](https://github.com/lencx/Minke) | README | 2026-09-09 |

**核验状态**：星数为 2026-09-09 实测；其余均为项目自述（二手），未做源码级验证。任何条目进入 `specs/` 或 ADR 前必须按 EXTERNAL_BASELINE 的规则复核。
