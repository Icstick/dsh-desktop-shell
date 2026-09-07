# ADR-0020: Managed DSH 源码仓库来源（Repository Source Form）（D5）

- Status: accepted
- Date: 2026-09-02
- Milestone: M8-E（v0.1.0 前落地）
- Owner: ui-and-runtime-owner
- 关联：docs/roadmap/PLAN-WIZARD-REPO-SOURCE.md（决策 D5 定稿）；ADR-0021（执行层渐进启动，WI-C 起草）

## Context

DSH 官方分发**没有 exe 形态**（Node/TS 项目）。真实拉起方式只有两种：npm
（`npx @deepseek-ai/dsh web`，属 attach/手动场景）与源码仓库
（clone + `pnpm install` + `pnpm run build` + `pnpm dsh web`）。而 SetupWizard 的
Managed 来源步只支持"可执行文件"心智（Search PATH/DSH_PATH 里的 `dsh.exe`、手动输
路径），discovery 对目录一律返回 `requires_recipe` 且不验证目录是否为 DSH 仓库——
wizard 对真实用户**无法配置 managed 环境**（D5 前的断裂点）。

已查证（D:\deepseek-harness 实证，2026-09-02）：
- root package.json name = `@deepseek-ai/dsh-root`；
- root script `dsh` = `node --import ./scripts/register-tsx-esm.mjs apps/cli/src/bin.ts`
  （TS 源码直跑，CLI 无需先 build；build 主要产出 apps/web 前端资源）；
- 构建产物判定：`apps/web/dist/index.html` 存在 = web assets 已构建。

## Decisions

### 决策 1：wizard Managed 来源 = 源码仓库单形态
- SetupWizard 来源步只提供「源码仓库」形态；exe 不再作为**新建**来源引导。
- `executable` mode 在 schema/discovery/contracts 中**保留**（兼容旧 catalog 数据与 attached 历史路径），不新建。
- npx 用法归 attach/手动场景，不在 wizard 引导（v1）。

### 决策 2：discovery 目录识别（WI-B 实现）
- 目录探测判定为 DSH 源码仓库的条件（OR）：
  a) `package.json` 的 `name` == `@deepseek-ai/dsh-root`；
  b) 结构 fallback（fork 支持）：`pnpm-workspace.yaml` + `apps/cli/src/bin.ts` + `scripts/register-tsx-esm.mjs` 同时存在。
- 识别成功 → `mode=repository`、`status=available`、`launchable=true`，并携带 repository 详情（决策 3）。
- 识别失败（目录存在但不是 DSH 仓库）→ `status=requires_recipe` + `NOT_A_DSH_REPO`（error）evidence。
  `requires_recipe` 语义收窄：**非 DSH 目录或结构损坏**（不再表示"任何目录都需要 recipe"）。
- 只读探测：不执行任何文件、不写仓库（与 M1 不变量一致）。
- 结构损坏细化：
  - 判定为 repo 但 `apps/cli/src/bin.ts` 缺失 → `REPO_ENTRY_MISSING`（error），status=requires_recipe；
  - loader（`scripts/register-tsx-esm.mjs`）缺失 → `LOADER_MISSING`（warning）——entry 为 .ts 时执行层需要 loader。

### 决策 3：HarnessCandidate 契约扩展（向后兼容）
- `HarnessCandidate` 增可选字段 `repository?: { repoRoot, entry, loader, needsInstall, needsBuild }`：
  - `repoRoot`：仓库根（canonical）；
  - `entry`：相对入口（`apps/cli/src/bin.ts`，POSIX 分隔）；
  - `loader`：相对 loader 路径或 null（`scripts/register-tsx-esm.mjs`；无 loader 需求/未知时为 null）；
  - `needsInstall`：`node_modules` 缺失 = true；
  - `needsBuild`：`apps/web/dist/index.html` 缺失 = true。
- 约束：`mode=repository` 且 `status=available` 时 `repository` 必填；否则缺省。
- 旧 client 不受影响（可选字段）；schema allOf 约束同步。

### 决策 4：wizard 数据落库（WI-A 实现）
- 来源目录 = `harness.path`（canonical repo 根），`harness.mode=repository`；
- `harness.cwd`：convert 时若 repository mode 且 cwd 为空 → 自动填 repo 根（recipe 以 repo 为工作目录）。
- advanced 暴露 nodePath / cwd / extraArguments。
- **环境标识（2026-09-02 maintainer review 变更）**：label（Profile 名称）可编辑；**id 由 label 自动生成且不提供手动编辑**
  （UI 只读展示 Profile-ID；编辑已有环境时 id 锁定不变）。派生规则保证 id 恒合法（数字/空开头自动加
  `env-` 前缀）。保存前对 catalog 做 id 冲突检查（同 id 且非当前编辑对象 → 拒绝保存，防静默覆盖）。
- clone 引导：未提供有效 repo 时显示建议位置 + `git clone --depth 1` 命令文本（v1 不内置一键 clone 按钮——需要 daemon 执行 git 的后台命令，与 WI-C 执行层一并评估）。


## 实现记录（2026-09-02，feat/wizard-repo-source）

- **WI-B（9d92f3e）**：决策 2/3 落地——discovery repo 识别 + HarnessCandidate.repository
  （schema/fixtures/contracts.ts/discovery.rs 三处同步，字段 5/5）。
- **WI-A（2c7e9b9 + ad0062b）**：决策 1/4 落地——wizard 来源步单形态、id 自动派生、
  advanced 暴露 nodePath/cwd/args、clone 引导（v1 展示命令，不做一键 clone）；
  cwd 留空自动=仓库根（environment-draft.ts convert）。
- **WI-C 最小 recipe（80f4d15，review round 2 实测驱动）**：决策 2 的执行层落点——
  build_launch_spec repository 分支改为目录语义（entry/loader 探测、cwd=repo、
  nodePath 或 PATH 探测 node）。**实测发现的 Windows 坑**：`node --import <绝对路径>`
  在 Windows 报 `ERR_UNSUPPORTED_ESM_URL_SCHEME (protocol d:)`（官方 recipe 用相对路径
  未暴露）——recipe 统一把 loader 转 `file:///` URL 传入。node 直跑 `.ts` 入口依赖
  node >= 23.6 默认 type-stripping（DSH engines ^22.19||>=24 中 22.x 需评估；渐进启动
  阶段可兜底）。
- **未实施（保持 PLAN 拆分）**：WI-C 渐进恢复（启动失败 → pnpm install/build → 重试）、
  WI-D 启动阶段 UI、一键 clone（需 daemon git 命令）。

## Consequences

- v0.1.0 用户可用 wizard 配置 managed 源码仓库环境；`requires_recipe` UI 呈现为"非 DSH 仓库"引导而非笼统失败。
- repository mode 的执行层 recipe（`--import` loader 注入、cwd、nodePath 探测、install/build 渐进恢复）留 ADR-0021（WI-C）。
- attached 行为不变；executable 旧 catalog 数据继续可编辑（environmentToDraft 路径保留）。

## 兼容性影响（执行层语义变更）

- repository 模式语义变更：base 中 repository =「nodePath + 脚本/构建产物文件路径」直跑；本 ADR 起 =「源码 checkout
  目录 + 固定 entry/loader 探测」。旧 catalog 中 `mode=repository` 且 path 指向文件的条目在启动时得到明确的
  UnsupportedSource 错误，需要改为指向 checkout 目录（v0.1.0 前无迁移负担，CHANGELOG 已记录）。

## 风险

- fork 识别过宽/过窄：name 判据优先，结构 fallback 需要三件套齐备才认；
  改名 repo（root name 改掉）靠结构 fallback 兜底。
- needsBuild 判据（apps/web/dist/index.html）随官方 repo 布局演进可能漂移——实施时以官方 main 为准复核；
  渐进启动（WI-C）以"拉起失败→恢复"兜底，判据只用于 UI 提示。
