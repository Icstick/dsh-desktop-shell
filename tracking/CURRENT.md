# Current Project State

- Phase：`shell-mvp`
- Milestone：M8 Stable Candidate（已合并 main @ 4c21489）+ **M8-E v0.1.0 发布（进行中，被两个发布门 blocker 门控）**
- Status：M1–M8 全部合并 main；wizard 分支 feat/wizard-repo-source 已 squash 合并 main @ 23c5027；ux-polish 分支进行中
- Implementation authorized：`true`
- External baseline verified：2026-08-25（dsh-std 刷新至 3df0543 / core rc.1）
- Last updated：2026-09-03T13:40:00Z

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
- **PLAN-ENV-QUICK-EDIT（2026-09-03 拍板 B 方案，分支 feat/env-quick-edit @ dd47129）**：1/5 docs ✅ 7fad4d4；
  2/5 backend remove_environment ✅ 8f85e4e（store NotFound 变体 + remove fn + 3 测试；commands + lib.rs 注册；cargo test 141/141）；
  3/5 设置页卡片化 + 向导触发式 + 移除流 ✅ 2ff77ef（i18n zh/en、DesktopApi.removeEnvironment、
  EnvironmentList 卡片操作+内联确认、SetupWizard onClose、ShellApp 编排 stop→remove→空态；vitest 88/88、tsc 绿）。
  4/5 EnvironmentEditForm 分区编辑 ✅ 9535d4f（六分区平铺无步骤机、id/policy/ownership/cwd 只读、policy 区仅 managed 显示、nodePath 仅 managed+repository、保存 validate→save→onSaved 关闭并刷新；i18n envEdit.* 28 键双语、12 用例；ShellApp 接线；vitest 102/102、tsc + cargo build --workspace 绿）。
  5/5 GUI 实机验收（2026-09-03 晚，tauri dev + 真实 catalog rev37→38）：
  ✅ 设置页卡片+添加按钮、无常驻向导；分区编辑保存链通（rev38，原子写+bak）。
  ❌ 验收问题 A：编辑 dev-repo 保存后 daemon 全链路不可用（快照无法刷新/Managed 加载中/诊断不可用）——catalog rev38 dshHome 被写成 C:\Users\Administrator\.dsh-isolated（目录不存在；隔离 home 实际在 D:\DSH_workspace\.dsh-isolated）。疑似 daemon 对无效 dshHome 环境的查询全挂。修：改回 D:\ 路径重测。
  ❌ 验收问题 B：移除 local-dsh 失败且文案显示字面 key「envlist.errorRemove」——zh/en 字典均漏该 key（i18n.test 只验双语平衡、不验 UI 引用完整性——测试盲区）；前端 catch 吞掉真实后端错误。✅ 已修 b3d74af：补 key + 错误显示后端 message + 用例（remove 后端失败原因在 GUI 重测时复现）。
  🔧 2026-09-04 早继续：A 根因确认=隔离 profile robocopy 复制丢失 16 个 reparse 插件链接+顶层包不全（@memtensor/memos-local-plugin 等）；B 修复提交 b3d74af。
  ✅ 隔离 home 改用 junction 方案（profiles/web/node_modules、profiles/node_modules、.dsh-module-fallback/node_modules 三处 junction 指向主安装，配置/数据独立、模块只读共享）→ 手工启动 3082 成功：插件全加载、数据全落 .dsh-isolated（memos.db 等）、主 sessions 零接触。
  ✅ dev-repo 配置修复：dshHome=D:\DSH_workspace\.dsh-isolated、port 3081→3082（3081 被并行 dev 升级线占用）→ catalog rev41。GUI 实机点「启动 Managed DSH」待重测（未完成）。
  ⬜ 改进 C（用户建议，待做）：启动失败报错细化——commands.rs:397 兜底 "Managed runtime is unavailable."（用户实测所见），daemon 侧 stderr 细节（spawn_output_reader 1351/1376）不透传；建议：失败路径把进程 stderr 尾部摘要放入 ManagedRuntimeReport.evidence（UI 已有 evidence[0] callout 通路 1092）或 CommandError message。
  ⚠️ 注意：tauri dev 仍在跑（job 保留），GUI 窗口在用户桌面；.dsh-isolated（D:\）已就绪 763MB。
  ✅ 改进 C 完成 268974f：启动失败原因端到端透传（supervisor spawn/attach 携带 os 原因；ManagedRuntimeError 去 Copy 加 SpawnFailed/ProcessTreeFailed/RuntimeUnavailable(String)；daemon RPC message 保留；CommandError.message static→String + truncate；desktop adapter 不再丢 daemon message）。Rust 测试全绿。
  ✅ 根因修复 ab264db：GUI managed 启动长期失败 = daemon 侧 local-transport read_deadline 30s idle 即关连接（长连接被当短连接）；GUI 无自动重连（注释明示 known limit）。修：daemon limits.read_deadline=24h。GUI 实测一次成功。
  ✅ 5/5 实机验收完成（2026-09-04）：GUI 启动 dev-repo → 3082 healthy gen1（endpoint verified）→ .dsh-isolated\sessions\--D-dsh-workspce-shell--\session-f010c5cf（111KB 真实会话）→ 主 GUI sessions 9:03 后零写入。隔离验证通过。
  ⬜ 遗留（用户报告）：窗口缩放时 DSH 界面（surface WebView）不随窗口缩放、内容偏小——建议排入 v0.1.0 后阶段 1（real-usage 优化，surface resize/视口同步），归属待用户拍板。
  ⬜ 已知缺口（记录）：GUI daemon client 连接死后无自动重连（start_background 注释 "fail closed until a future reconnect slice"）——建议后续改进（invoke 失败自动重连）。
  📋 试用反馈批次（2026-09-04 GUI 实测，5 项）：
    1) 浏览器：独立 WebView 窗口=设计（ADR-0017/AC-BRW-001）；问题=导航百度后 panel 收到 load_failed 置 error（页面实际成功）——疑似误报/事件语义，待复现看 browser://event 负载（browser-provider 侧查 load_failed 判定）。
    2) 终端切回黑屏 ✅ caccc6a：TerminalPanel 曾随 surface 卸载（xterm dispose 丢 buffer）→ 改 visited 后保持挂载隐藏；切回不再黑。
    3) 终端关闭后无重开按钮 ✅ caccc6a：session null 时 chrome 显示「打开终端」按钮。
    4) 通知缺全部关闭 ✅ caccc6a：header dismiss-all（循环 dismiss，测试 31/31 含新用例）。
    5) 用量：shell 对话中途手动停止后 usage 未见记录——假设：被中断 turn 无 usage 记账事件；待查 daemon usage collector 数据源与 interrupted turn 处理（desktop 侧观察 dsh 用量 vs dsh 内部按请求记账）。
  ⬜ 缩放问题（前条记录）：窗口缩放 DSH surface 不跟随——排 v0.1.0 后阶段 1。
  ✅ 浏览器误报修复 82ca72c（试用反馈 1）：双根因=①NavigationCompleted 把取消/重定向导航当失败（IsSuccess=false 未查 WebErrorStatus；OPERATION_CANCELED/REDIRECT_FAILED 现被过滤）②Error 态被成功加载无法恢复（mark_ready 只接受 Loading）→ 现 Error→Ready 恢复并清除 error 消息。browser-provider 34 + desktop 141 测试全绿。
  ✅ 用量项根因（反馈 5）：desktop usage collector 只记录 Desktop 自身事件（terminal/notification，usage.rs 34 行 Sources 注释）——**从未接入 dsh 对话 token 用量**；面板 totals 仅反映本地记录。停止对话是否加剧取决于 dsh-cost-meter 记账时机（待查插件侧）。接入方案（阶段 1）：读 dsh-cost-meter ledger（DSH_HOME/storages/cost-meter/ledger.json）或订阅 dsh usage 事件——设计待定。
  ✅ 2026-09-04 下午：PLAN-ENV-QUICK-EDIT 全 5/5 完成 → squash 合并 main @ c8ef038（21 files +2338/-136，已推送 origin）。
  状态更新：Status=env-quick-edit 已合并 main；分支 feat/env-quick-edit 本地保留（未删）。
  Open items（阶段 1/发布后）：①浏览器 load_failed 误报（试用反馈 1）②用量停止记账待查（反馈 5）
  ③缩放不跟随 ④daemon client 无自动重连缺口 ⑤zh 措辞润色。
  ✅ M8-E v0.1.0 发布完成（2026-09-04）：externalBin 本地重建验证 ✅（nsis 2m16s）→ tag v0.1.0 前移 980906f（含 wizard/ux/env-quick-edit/终端/通知/浏览器修复全量）→ CI 三平台重建成功（run 33846974919；workflow 修复：daemon sidecar 需 target-triple 后缀）→ 本地补传 11 crate SBOM + npm-sbom + 自签 Windows 安装包（证书 1B6A576C，UnknownError=自签预期）→ **published 2026-09-04T07:15Z**（github.com/Icstick/dsh-desktop-shell/releases/tag/v0.1.0；资产：windows nsis/msi + signed、macOS aarch64 dmg、ubuntu deb、checksums、SBOM）。
  → 阶段 0（发布门）完成；阶段 1（稳定期）开始：open items（usage 接入 dsh 对话用量、缩放、daemon 重连缺口、zh 润色、live-daemon-qa 入 CI、M6-C/M6-C4 TODO）。
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
