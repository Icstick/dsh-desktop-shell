# Current Project State

- Phase：`shell-mvp`
- Milestone：M8 Stable Candidate（已合并 main @ 4c21489）+ **M8-E v0.1.0 发布（进行中，被两个发布门 blocker 门控）**
- Status：M1–M8 全部合并 main；本地 main @ 0e25ac8（remote 另有用户 README 提交 5c50ed6，待对齐）
- Implementation authorized：`true`
- External baseline verified：2026-08-25（dsh-std 刷新至 3df0543 / core rc.1）
- Last updated：2026-09-02T15:00:00Z

## 当前状态

- M0–M6：done（已接受并合并 main；local-transport 非阻塞 socket 修复 e352b0d、M6-E flaky 根治三项也在 main）。
- M7 Setup Wizard + Multi-Profile B1：done（main @ e629c7f）。
- M8 Stable Candidate（三平台 CI、Unix PTY、browser 降级、cargo-deny、SBOM、Windows 自签）：accepted（main @ 4c21489），REVIEW-M8 双轴闭环。
- M8-E v0.1.0 发布（WI-M8-RELEASE，in_progress）：token 验证 ✅、release workflow 6 轮全绿 ✅（draft 17 assets + 中文 notes）、
  local 自签 ✅、externalBin daemon 打包提交 0e25ac8 ✅（本地重建验证未做）、live-daemon-qa 25/25 ✅。
- 环境：Rust 1.98.0 + MSVC 14.51 + Windows SDK；本机可完整跑 cargo/vitest 门禁。

## Blockers（v0.1.0 发布门，决策 D1：修复先于 release）

- **BLOCK-M8E-BOOTSTRAP-STUCK**：Shell GUI 停在 "Reading canonical runtime state"（daemon 协商偶发卡）。
  调查：`docs/investigations/m8e-shell-bootstrap-stuck.md`；调试 4 步见 `docs/roadmap/PLAN-DEBUG-OPTIMIZATION.md` §2.1。
- **BLOCK-M8E-I18N-ZH**：✅ **resolved（2026-09-02）**——zh 字典 123 key 全量翻译 + HarnessSurface/EnvironmentList 硬编码文案 i18n 化（fix/m8e-i18n-zh）；GUI 实机验收通过。遗留：zh 措辞润色（SetupWizard 文案 i18n 已随 wizard 重做完成）。
- **SetupWizard 重做（决策 D5，2026-09-02）**：设计 PLAN-WIZARD-REPO-SOURCE.md（WI-A..D）+ ADR-0020；
  **WI-B 完成**（discovery repo 识别 + HarnessCandidate.repository 契约扩展，specs 126 fixtures ALL PASS）；
  **WI-A 完成待验收**（来源步源码仓库单形态 + 探测详情 + clone 引导 + id/label 编辑 + advanced
  nodePath/cwd/extraArguments + wizard 全量 i18n + setup-wizard 样式接入；vitest 74/74、pnpm check、tauri build 门禁）。
  branch: feat/wizard-repo-source（9d92f3e WI-B / 2c7e9b9 WI-A / de78738 docs / ad0062b+80f4d15 review 两轮）。
  **review round 2（80f4d15）**：①执行层 repository 目录 recipe 落地（WI-C 最小核心，含 Windows
  `--import` file URL 坑修复）——Managed 保存后可真正启动 DSH；②Attached auto 端口不再误报保存失败
  （错误细分：保存/启动/验证分离 + 后端真实消息）；③shell exe 去控制台（windows_subsystem）。
  门禁：daemon ~109 / managed-runtime 26 / vitest 79 / pnpm check / specs 126 ALL PASS。
  **遗留**：WI-C 渐进恢复 + WI-D 启动阶段 UI、一键 clone、EnvironmentList 无样式、zh 措辞润色。

## remaining

- M8-E：两 blocker 解除 → externalBin 本地重建（tauri build --bundles nsis 含 daemon）→ 更新 draft → publish v0.1.0 → 收尾文档。
- 代码遗留：TODO(M6-C)×3（daemon lease 撤销、envelope 固定端口）、TODO(M6-C4)×2（browser 导航状态上报）、
  M3 尾项（TerminalPanel 自动化、macOS/Linux target-host 证据、diagnostics 专项 UI）。
- repo public 后：release workflow attestation 启用（已留 checksums 替代注释）。
- 文档漂移已修：project.yaml current_milestone M8 + schema pattern ^M[0-9]+$、CURRENT.md、INDEX.md（新增路线图节）。

## 下一动作（总方案 = docs/roadmap/PLAN-DEBUG-OPTIMIZATION.md，决策 D1/D2/D3）

1. 阶段 0（发布门）：卡点 A 四步调试修复 → 卡点 B i18n 修复 → externalBin 重建 + publish v0.1.0 → 收尾。
2. 阶段 1（稳定期，v0.1.0 后）：遗留 TODO 收尾、live-daemon-qa 入 CI、真实使用驱动优化、落三条多 profile 设计决策。
3. 阶段 2（远期 feature，D2）：v0.1.0 发布后 B2 并发多 profile 立项（M9/M10，输入 = field-evidence + PLAN-B2）。
