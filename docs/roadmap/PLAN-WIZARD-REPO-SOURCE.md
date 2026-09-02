# PLAN-WIZARD-REPO-SOURCE: Managed DSH 源码仓库形态 + 渐进启动（决策 D5）

> 2026-09-02 maintainer 拍板（本会话）。状态：**设计定稿，待实施**（契约+执行层+UI 改动面广，独立工作项实施）。
> 背景：wizard 现状只支持 executable 路径+Search；DSH 官方分发无 exe 形态（Node/TS 项目），
> 真实拉起方式只有两种：npm（npx @deepseek-ai/dsh）与源码（clone + pnpm install/build + pnpm dsh web）。
> 已查证：pnpm dsh = root script = `node --import scripts/register-tsx-esm.mjs apps/cli/src/bin.ts`；
> CLI 入口 apps/cli/src/bin.ts（TS 源码直跑，无需先 build CLI）；build 主要为 web 前端资源
> （apps/web → @deepseek-ai/dsh-web-frontend）与发行。

## 决策（D5，2026-09-02）

1. **exe 形态移除**：wizard 不再提供「可执行文件」来源（DSH 无 exe 分发）；schema/discovery 的
   executable 保留（兼容旧 catalog 数据），但不再新建。npx managed 不做（v1）；npx 用法属 attach 场景。
2. **wizard DSH 来源 = 源码仓库**：输入 repo 目录 → 自动探测（package.json + pnpm-workspace +
   apps/cli/src/bin.ts + scripts/register-tsx-esm.mjs）→ 配置 repository 环境。
3. **Managed 启动策略（渐进恢复）**：
   - 先尝试直接拉起（等价的 `node --import <loader> <bin.ts> web --host 127.0.0.1 --port N --no-open`，
     cwd=repo，DSH_HOME 注入；node 由 nodePath 或 PATH 探测）
   - 拉起失败 → 自动全流程恢复：`pnpm install` + `pnpm run build` → 再次拉起
   - repo 未配置/不存在 → wizard 提示**在哪个位置 clone**（建议目录 + git clone 命令），
     clone 完成后该位置即环境配置（harness.path/cwd）

## 待实施拆分（独立工作项，按仓库流程）

### WI-A：wizard 来源步重做（environment-settings）
- 来源类型单选改为「源码仓库」单形态（附说明：官方推荐 clone 方式；npx 属手动 attach）
- 目录输入 + 探测（repo 有效性/入口/loader/web-assets build 状态）
- 未 clone：显示建议位置与 clone 引导（可一键 clone：git clone --depth 1 官方 repo 到指定目录，后台任务）
- id/label 可编辑（多环境）；advanced 暴露 nodePath/cwd/extraArguments（编辑回填可见）
- SetupWizard 文案 i18n 化（zh/en，随本工作项）
- 测试：SetupWizard.test 扩展（repo 来源路径）

### WI-B：discovery 目录探测（src-tauri/discovery.rs）
- inspect_candidate 对目录：识别 deepseek-harness repo（package.json name=@deepseek-ai/dsh-root 或
  pnpm-workspace.yaml + apps/cli/src/bin.ts）→ candidate mode=Repository + launchable（含 evidence：
  入口/loader/需要 install 或 build 的状态）
- 契约：HarnessCandidate 增可选字段（repoRoot/entry/loader/needsInstall/needsBuild），fixtures 更新

### WI-C：执行层 repository recipe（managed-runtime supervisor + daemon runtime）
- build_launch_spec：repository + entry 为 .ts → 自动插 `--import <repo>/scripts/register-tsx-esm.mjs`
  （entry 为构建产物 .js 时不插）；entry 缺省探测 apps/cli/src/bin.ts
- 启动恢复：managed start 失败（ProcessExited/ReadinessTimeout 且 code 指示依赖缺失）→ 自动执行
  `pnpm install`（如需）+ `pnpm run build`（如需，web assets 缺失时）→ 再次启动（一次）
- 长任务（install/build）以子进程+日志上报方式运行；整体有界（超时后 fail-closed）
- 契约/ADR：managed start 行为扩展 → ADR-0021 草案 + managed-runtime-report 可选字段（bootstrap 阶段）
- 测试：recipe loader 单测 + 恢复路径（mock install/build 成功/失败）

### WI-D：UI 状态（可选，随 WI-C 验收）
- start 过程中显示阶段（starting → installing/building（进度）→ ready/失败原因）

## 风险与开放点

- pnpm 定位：install/build 需要 pnpm（PATH 或 pnpm home 探测）；node 直跑 recipe 不依赖 pnpm
- install/build 时长不可控（分钟级）→ 超时上限 + 可取消（后置）
- web assets 判定：apps/web/dist 存在性不是权威（构建产物路径待实施时在官方 repo 核实）
- profile 语义沿用现有（dsh --profile 注入逻辑不动）
