# PLAN-M7: Setup Wizard + Multi-Profile (B1)

> M7 切片规划（maintainer 2026-08-30 确认方向：向导 + B1 实现，B2 单独规划）

## 背景

M6 交付了 daemon 化与真实 DSH 进程树管理（runtime.start/status/stop/restart by
environmentId）。但配置体验仍是单页表单（EnvironmentSetup），且「多 profile」只
有 catalog 注册能力，没有管理 UI 与切换流程。社区 Windows 桌面版 dsh 普遍支持
多 profile 管理——本里程碑补齐向导式配置与多 profile 切换（B1），并为并发多
profile（B2）单独规划。

## 切片

### M7-A 配置向导（WI-M7-SETUP-WIZARD）

分步引导（6 步），每步实时验证、可回退：

1. **模式选择**：Managed（本机启动 DSH）| Attached（连接已运行实例）
2. **DSH 发现**：自动探测（PATH + 常见安装位置 + 上次记忆）+ 手动选择
   —— 复用现有 discoverHarnesses
3. **Profile 选择/创建**：扫描 `~/.dsh` 下 profile 目录（config.yaml 存在性 /
   目录名列表），支持新建（`-p <name>` 语义）；默认 `default`
4. **数据目录与端口**：dshHome 默认 `~/.dsh`（可改）、端口默认 auto（Managed
   需显式或由 daemon 分配）——新增端口占用探测
5. **预检**：版本探测、依赖（node）、端口可用性、目录可写 —— 聚合展示
6. **保存 + 启动**：写入 catalog（含 daemon catalog 同步，见下）、启动 Managed /
   探测 Attached

后端新增：
- `discover_profiles(dsh_home)`：列出 profile 目录（含 config.yaml 校验）
- `probe_port(port)`：端口占用检测（复用 local-transport 探测模式）
- **catalog 同步**：Shell 保存环境时同步写 daemon 数据目录 environments.json
  （修复现状：daemon 读不到 Shell 保存的环境——两套 catalog 未打通）

### M7-B 多 profile 管理（B1，WI-M7-MULTI-PROFILE-B1）

- 环境列表面板：所有 catalog 环境（Managed/Attached 混合）卡片 + 状态
- 启动/停止/重启/切换按钮（runtime.start/stop/restart by id，单活跃语义：
  切换 = stop 当前 → start 目标）
- active 环境标记持久化（catalog activeEnvironmentId）
- 前端零后端改动预期（命令已具备）；核心工作量在 UI + catalog 同步打通

### M7-C B2 预研交付

- PLAN-B2-MULTI-PROFILE-CONCURRENT.md（ADR-0020 草案：per-environment supervisor、
  多 surface 架构、端口规划、风险与迁移路径）——见独立文档
- 定位：M8 或 M9 候选（supervisor 核心改造 + 多 surface，独立里程碑）

## 门禁

- 既有全量门禁（workspace 串行、fmt/clippy、vitest、specs、ACL）+ live QA 扩展
- 向导与 B1 的 vitest（UI 行为）+ Rust 单测（profile 扫描/端口探测/catalog 同步）

## 依赖

- M6（daemon 化）已完成；catalog 同步改动涉及 environment_store 与 daemon 侧
- B1 依赖 M7-A 的 catalog 同步（先 A 后 B）
