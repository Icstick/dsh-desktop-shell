# PLAN-DEBUG-OPTIMIZATION: v0.1.0 发布门 + 稳定优化期 + 多 profile 远期

> 2026-09-02 maintainer 拍板（本会话，来源：现状梳理 + `multi-profile-reference.md` +
> `open-questions-2026-09-01.md` 综合）。本文是「debug 和优化阶段」的**可跟踪方案**：
> 决策、阶段、门禁与文档引用链全在此处，后续会话按 tracking 规则推进。

## 0. 决策记录（2026-09-02）

| # | 决策 | 影响 |
|---|---|---|
| D1 | **修复先于 release**：卡点 A（Shell GUI bootstrap 偶发卡）与卡点 B（i18n 中文不生效）**必须先修复并验证，才能 publish v0.1.0** | M8-E 的 publish 动作被两个 BLOCK 门控（见 §2）；draft v0.1.0 保持 draft |
| D2 | **B2 先记录不立项**：并发多 profile（PLAN-B2 草案，M9/M10）只更新状态与范围记录；**触发条件 = 首个可用版本（v0.1.0）发布后，作为 feature 正式立项** | 不新建 WI-M9-*；不扩 tracking 认领；PLAN-B2 状态段更新 |
| D3 | **方案落成可跟踪文档**：本文（roadmap PLAN）+ tracking/blockers/BLOCK-M8E-* ×2 + PLAN-B2 状态更新 + project.yaml / CURRENT.md / WI-M8-RELEASE 同步 | 文档链见 §5 |

## 1. 现状快照（2026-09-02，真源 tracking/ + HANDOFF-M8E-RELEASE）

- 主线：M1–M8 已合并 main（M7 配置向导 + 多 profile B1 @ e629c7f；M8 Stable Candidate @ 4c21489）。
- M8-E（WI-M8-RELEASE，v0.1.0）：token 验证 ✅、release workflow ✅（draft 17 assets + 中文 notes）、
  本地自签 ✅、externalBin daemon 打包提交 0e25ac8（本地重建验证未做）、live-daemon-qa 25/25 ✅。
- 卡点 A：Shell GUI 停在 "Reading canonical runtime state"——daemon 协商偶发卡死；
  调查暂停于 2026-09-02（`docs/investigations/m8e-shell-bootstrap-stuck.md`），4 个假设未验证 → BLOCK-M8E-BOOTSTRAP-STUCK。
- 卡点 B：切中文后界面仍英文 → BLOCK-M8E-I18N-ZH。
- 代码遗留：TODO(M6-C)×3（daemon lease 撤销、envelope 固定端口）、TODO(M6-C4)×2（browser 导航状态上报）、
  M3 尾项（TerminalPanel 自动化、macOS/Linux target-host 证据）。
- 文档漂移（本方案已修一部分）：ROADMAP.md 未含 M8+；project.yaml 停在 M6；CURRENT.md 停在 M7 合并前；
  `specs/tracking/project.schema.json` 的 current_milestone pattern 仅 `^M[0-7]$`（随 M8 修正）。
- 分支注意：本地 main @ 0e25ac8；remote 有用户 README 提交（5c50ed6）——动代码前先对齐。

## 2. 阶段 0：M8-E 发布门（现在 → v0.1.0 publish）

### 2.1 卡点 A（BLOCK-M8E-BOOTSTRAP-STUCK，发布硬门）
调查文档：`docs/investigations/m8e-shell-bootstrap-stuck.md`（4 假设：A spawn 竞态 / B 双连接并发协商 /
C AppData credential 残留 / D snapshot UI 永挂分支）。调试路径（按文档建议）：
1. 复现时抓 daemon 线程栈（procdump/dotnet-dump/VS 附加），定位死锁持锁方；
2. shell spawn 段加时序日志（spawn → credential wait → connect → negotiate 各阶段时间戳）；
3. 卡住时对 daemon 做第二次裸 socket Hello——第二个也卡 = 全局死锁；第二个过 = 首个连接持有状态卡住；
4. 手动预热 daemon（DSH_DAEMON_EXE 指向 QA 验证的 daemon，就绪 2s 后再启 shell）——稳定过则确认假设 A。
解除条件：修复落地后本地 GUI 连续启动验证通过 + live-daemon-qa 回归全绿。

### 2.2 卡点 B（BLOCK-M8E-I18N-ZH）
排查 ShellApp 语言切换链路：切换事件 → 持久化 → t() 文案表（zh 缺失?）→ 重渲染。
解除条件：切中文后界面文案生效（关键页面清单：环境列表 / 设置 / 向导）。

### 2.3 发布收尾清单（blocker 解除后）
1. externalBin 本地 `tauri build --bundles nsis` 验证安装包含 daemon → 更新 draft setup exe；
2. publish v0.1.0（tag + release notes + SBOM artifacts + checksums；attestation 留 repo public 后）；
3. 收尾：CURRENT.md / project.yaml / ROADMAP.md；对齐 remote（含用户 README 提交 5c50ed6）。

**阶段 0 退出判据**：v0.1.0 published；GUI bootstrap 连续 N 次启动无卡；中文界面生效；tracking 收尾。

## 3. 阶段 1：稳定 debug/优化期（v0.1.0 发布后，用户明示「还需要一段时间 debug 和优化」）

按「真实负载优先」排（依据 open-questions §0/§4 纪律：按实际负载排修复优先级）：
1. **遗留代码收尾**：TODO(M6-C) lease 撤销与 envelope 固定端口、M6-C4 browser 导航状态上报、M3 尾项；
2. **回归基线 CI 化**：live-daemon-qa（现仅本机脚本）接入 CI 门禁，防止回归；
3. **真实使用驱动优化**：i18n 全覆盖、诊断面板（MOD-RUNTIME-DIAGNOSTICS）、TerminalPanel 自动化用例恢复；
4. **落三条设计决策记录**（对应 open-questions §4，供 B2 使用）：
   ① 便宜门控 = 一等公民（采纳，P0）；② 隔离 = 配置约束 + Shell 注入命名空间（采纳，P0）；
   ③ 共享资产 = 引用不复制，引用失效硬失败（采纳，P1）。

## 4. 阶段 2：B2 并发多 profile（远期，作为 feature——D2）

- **触发条件**：v0.1.0 发布后，以 feature 立项（届时按 tracking 规则建 WI-M9-* / 走 ADR）。
- **输入链**：`docs/multi-profile-field-evidence.md`（A/B/C/D/R 建议 + ROI 排序）←
  `multi-profile-reference.md`（现场证据）← open-questions §4 三条设计点。
- **范围（草案，来自 PLAN-B2 + field-evidence）**：
  - M9（后端）：supervisor per-environment 状态表 + 端口分配器；precheck 门控（A1/A2，P0）；
    配置合成体系 base+override、禁绝对路径、合成校验、引用失效硬失败（B1/B2/B4/C1 最小版，P0）；
  - M10（前端）：多 surface tab + 负载分布视图（D1）+ 健康上报强制（D2）；
  - 红线：R1 不配 per-profile IM Bot；R2 模板只放实测在用的资产；R3 无「警告后继续」路径。
- **风险修正**：PLAN-B2 风险表「多 DSH 磁盘/资源竞争=低（profile 天然隔离）」需拆出
  「配置级隔离失效=高」——reference §2.3 证明目录隔离是假象。

## 5. 文档引用链

- 本方案：`docs/roadmap/PLAN-DEBUG-OPTIMIZATION.md`
- 卡点调查：`docs/investigations/m8e-shell-bootstrap-stuck.md`
- 发布细节：`docs/release/v0.1.0-release-notes.md`、`docs/release/signing-and-distribution.md`
- 多 profile 输入：`docs/multi-profile-field-evidence.md`、`docs/roadmap/PLAN-B2-MULTI-PROFILE-CONCURRENT.md`
  （外链：`D:\DSH_workspace\docs\multi-profile-reference.md`、`D:\DSH_workspace\docs\open-questions-2026-09-01.md`）
- 状态：`tracking/project.yaml`、`tracking/CURRENT.md`、`tracking/work-items/WI-M8-RELEASE.yaml`、
  `tracking/blockers/BLOCK-M8E-BOOTSTRAP-STUCK.yaml`、`tracking/blockers/BLOCK-M8E-I18N-ZH.yaml`
