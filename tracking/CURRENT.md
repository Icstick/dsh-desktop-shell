# Current Project State

- Phase：`shell-mvp`
- Milestone：M8 Stable Candidate（已合并 main @ 4c21489）+ **M8-E v0.1.0 发布（进行中，被两个发布门 blocker 门控）**
- Status：M1–M8 全部合并 main；wizard 分支 feat/wizard-repo-source 已 squash 合并 main @ 23c5027；ux-polish 分支进行中
- Implementation authorized：`true`
- External baseline verified：2026-08-25（dsh-std 刷新至 3df0543 / core rc.1）
- Last updated：2026-09-03T09:00:00Z

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
- **M8-E 后续计划执行（PLAN-POST-WIZARD，2026-09-02 定稿）**：wizard repo-source 已 squash 合并 main（23c5027，10 commits，
  ADR-0020；云端独立审计 3 阻塞项均已在合并前修复）。进行中：feat/ux-polish 盲审修复——
  1.1 P3 3989 调查结论：无硬编码/无换算，策略端口=镜像 catalog active 记录（本机 catalog=3080）；
  已加来源标注。1.2 文案人话化完成（云端草案落地）。1.3 H1=页面名已满足。1.4/1.7 枚举本地化+状态色完成。
  待人工验证：P5 布局观感、P6 按钮、8 项视觉清单（PLAN 1.6）；阶段 2 配置持久化（ADR-0022 形态 A 推荐，待用户拍板）。
  门禁：vitest 83/83、pnpm check 全绿。
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
