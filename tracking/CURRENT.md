# Current Project State


## 2026-09-13 早 · 合并后 CI 修复（三条，全部本地复现）

- CI 首次结果：测试矩阵三平台（win/mac/ubuntu）**全绿**；live-qa-windows 失败。
  修正记录：ADR-0022 合并后的 ubuntu/macos 曾因 cfg(unix) lint 红（dead_code/unused import/
  unused_mut）——已在 6ee6a44 修复，并用**本地交叉 clippy**（rustup linux/macos target）验证；
  方法已写入 AGENTS.md 证据要求节。
- live QA 暴露真 bug（c2755ea）：PipeStream::read 把「对端已关闭」当成「暂无数据」→
  serve_connection 永不结束 → 断连路径全部失效（凭证重签 / lease 撤销 / 所有权释放）。
  修复：关闭的管道按 EOF（read==Ok(0)）返回，回归测试 peer_close_surfaces_as_eof。
- 附带：live-daemon-qa B4/B5 竞态（读到已消费 token → replay）改为「读文件 + 连接」重试循环；
  A3 断言升级为 credential schema v2 + pipe carrier。
- 本地验证：live-daemon-qa **25/25 PASS**；transport/daemon/desktop 全绿；交叉 clippy 干净；fmt 干净。
- 新 CI run 34727142785 监视中。
## 2026-09-12 22:00 · ADR-0022 Windows 全量合并 main

- feat/m12-peer-identity（17 commits，含并行会话的 pending-reap 修复）squash 合并 main @ dca033b，
  已 push；分支本地与远程均已删除。
- 合并前门禁：fmt/clippy 干净、workspace 全量测试无失败、specs 63 schemas + 133 fixtures 全过。
- WI-M12-PEER-IDENTITY → done（Windows 闭环：实现 + 负向测试 + 真实会话冒烟）。
- 后续：WI-M13-UNIX-UDS-CARRIER（ready）承接 Unix UDS 载体、live QA 管道腿与 H-2 状态翻转。
- CI：main push 已触发，稍后可查（gh run list）。
## 2026-09-12 21:30 · 真实会话冒烟通过（Windows 闭环）

- debug Shell + daemon 构建（target/debug 同目录）→ 隔离数据目录启动。daemon banner：管道名 +
  identity strict（expected shell 自动推导到 target/debug/dsh-desktop-shell.exe）。
- daemon 日志：connection accepted peer=pipe:... identity=pid=25476 image=<Shell exe>；
  activation ... authority=ShellControl —— Shell 真实进程走管道并拿到控制面权限。
- GUI 正常渲染（导航/运行时面板），仅有空 catalog 的预期领域错误；磁盘凭据为 v2（含 pipeName）。
- 剩余：slice 6（Unix UDS + CI）、live QA 管道腿、H-2 状态翻转、合并 main。
## 2026-09-12 深夜 · ADR-0022 全部 Windows 切片落地（feat/m12-peer-identity）

- slice 4：附加载体共享监督状态 + daemon 挂载管道 + credential v2(pipeName) + --peer-identity-policy
  （Windows 默认 strict / Unix 默认 Off 直到 UDS 载体）+ 策略负向测试（TCP 无身份拒绝 / Off 放行）。
- slice 5：Shell 客户端载体泛型化（LocalClient<S> / ClientStream / connect_stream），daemon_client
  优先管道、TCP 降级并记日志；端到端测试证明管道连接在 daemon 侧携带内核身份。
- slice 7 核心：peer_identity_pipe.rs 两条腿——镜像匹配的 Shell 走管道保留 shell_control（dispatch 成功）；
  同用户伪装进程（有效凭据 + 自称 Shell + 不同镜像）拿不到任何 grant、dispatch Unauthorized。
- H-2 技术判据已满足（AD(4) 的负向测试 + 默认路径）；完整关闭前还差：真实会话冒烟（GUI 走管道）、
  slice 6（Unix UDS + CI 矩阵）、live QA 的管道腿。
## 2026-09-12 晚 · ADR-0022 实现开工（WI-M12-PEER-IDENTITY @ feat/m12-peer-identity，已 push）

- ADR-0022 转 accepted；WI-M12 认领。slice 1（7047fd6）carrier 抽象 + peer identity
  （Windows FFI / Linux nix SO_PEERCRED / 其他 Unix 明示 Unsupported；unsafe 基线 forbid→deny 单点豁免）；
  slice 2（0833542）server core 泛型化（TCP 行为不变）；slice 3（b2549e0）Windows 命名管道载体
  （帧往返 + 内核 PID 断言 + 读超时语义 + 毒丸式可中断 accept；transport 全绿）。
- NEXT slice 4/5（必须同批）：daemon 接受命名管道连接（credential 文件加 pipe 名）+ Shell 客户端优先管道、
  TCP 降级；expected-Shell-path 策略（override → 布局推导 → fail closed）决定 shell_control；
  TCP 无身份 → 不得 shell_control（ADR-0022 决策 3）。之后 slice 6 Unix UDS（CI 矩阵）、slice 7 伪装进程负向测试 + live QA。
## 2026-09-12 · P0/P1 执行（审计后收口 + 排期 + 设计）

- P0：CI 回绿确认（09-11 三次 main run success；09-08 的 clippy 红已随审计修复消除）。
  ADR-0021 转 **accepted**（option C 已合并 f813d7a；决策 5 的 H-2 部分收敛状态保留）。
- 收编 09-11 WIP：ADR-0023（user gesture gate，accepted）+ WI-M11（proposed）已提交（182b897）。
- ADR-0022（local-transport peer identity）起草为 **proposed**：方向取 B2（Named Pipe/UDS
  内核级 peer identity），spike 先行；B1（TCP 表查 PID）仅作过渡。落地后 H-2 才可标「已关闭」。
- WI-M10-BROWSER-FLAKE：ownership flake 根因 = 测试断言 pre-M6-C4 旧行为（断连释放所有权）
  与异步 teardown 竞态；重写为双语义（活连接拒绝 + 无主会话可接管，轮询 5s 消除竞态），
  修复后 20/20 绿（修前 10/20 失败）。WI-M10-IDENTITY-BINDING 关闭（done）。
- 排期：ROADMAP 新增 Priority queue（ADR-0022 spike → SAFE-MODE-RECOVERY →
  PLUGIN-FAULT-ATTRIBUTION → WI-M11 GESTURE-GATE → MULTI-BACKEND-FLEET）。
- 0.3.0 设计：docs/roadmap/DESIGN-WORKBENCH-PHASE1.md（containment 安全核心、树/编辑/保存、
  git CLI 后端、命令面与分期 FS-M1/M2 + GIT-M1/M2、非目标）。
- ADR-0022 spike（Windows 半）已完成：docs/research/SPIKE-PEER-IDENTITY-20260912.md ——
  Named Pipe + 现有 framing 兼容、GetNamedPipeClientProcessId + 镜像路径可用、独立进程可区分；
  Unix 半（UDS + SO_PEERCRED）待 CI 矩阵。spike 项目在仓库外 D:\DSH_workspace\.spike-peer-identity（可删）。
- NEXT：用户回后定 ADR-0022 是否接受 → 开实现 WI（含 Unix spike）；或并行 claim FS-M1。
## 审计修复已合并 main（2026-09-10 晚）

- 分支 fix/audit-20260910-security-hardening（含 docs/peer-survey-20260909 的调研文档）
  squash 合并 main @ f813d7a 并推送；两条远程分支已删。
- 合并前本地全门禁（CI 口径）：fmt clean / clippy -D warnings exit 0（main CI 红因
  browser.rs doc list 缩进，本次修复）/ cargo test --workspace --test-threads=1 全绿 /
  specs 63 schemas + 133 fixtures ALL PASS / tsc + vitest 112/112 / ACL 40 commands passed。
- CI run 34483524355 已触发（验证中）；此前的 34191183850 / 34191163709 失败即 clippy 项。
- WI 状态：WI-M10-AUDIT-FIXES done、WI-M10-CLIPPY-DOCLIST done、
  WI-M10-IDENTITY-BINDING review（ADR-0021 proposed，等用户接受；H-2 仍为部分收敛，
  方案 A/B 留给 ADR-0022）。

## 安全审计修复（2026-09-10, WI-M10-AUDIT-FIXES @ fix/audit-20260910-security-hardening）

### 历史记录：分支工作期（2026-09-10, WI-M10-AUDIT-FIXES @ fix/audit-20260910-security-hardening）

- 依据 docs/audits/audit-summary-2026-09-10.md 落地 6 条 desktop-shell 侧修复，每条一个 commit，**未 push**：
  - H-1 `6695c7f` daemon 凭据文件 0600/目录 0700（Unix，rename 前设权限）+ `data_dir()` 不再退化到 cwd（XDG/HOME 或明确报错）；`daemon_client::StartupOptions::try_default`、`default_catalog_path` 随之改为 fallible。
  - H-3 `d30e5e6` DSH Surface `page_load` 诊断不再把带 43 字符 web token 的 bootstrap URL 写进 stderr（保留 host/path，抹 query/fragment/userinfo）。
  - theme D `a1e9b2b` `harness.path` 成为显式信任边界：`validate_launch_target` 在 `save_environment` 与 `start_managed_environment` 两处校验存在性/文件类型/可执行性/路径归一化。
  - theme B `0da7bf7` `scripts/validate-specs.mjs` 的未支持关键字检查不再是死代码（递归 + 计入退出码；顺带实现 `minProperties`）。
  - theme B `f82cfd8` terminal create/write/resize 在 Shell 层落地 schema 边界（cols/rows/shell/cwd/data）。
  - theme B `676e61c` `tests/contract` 变成真实 workspace 成员 crate（4 个契约测试），接入 `cargo test --workspace` 与根 `pnpm test`。
- 门禁：基线 cargo 550 / vitest 112 / specs 63-133 ALL PASS → 复检 cargo **567** / vitest 112 / specs ALL PASS，`cargo fmt --check` 干净。
- 未做（需先写 ADR）：**H-2 hello 身份自证 + 请求即授予**（建议 ADR-0021，要点见修复报告）。
- 预存问题（本次未动，需单独 WI）：`crates/daemon/src/browser.rs:20-22` clippy `-D warnings` 失败；`browser_integration::browser_session_ownership_is_connection_scoped` 并行下约 40% flake。
- 产物：`docs/audits/fixes-desktop-shell-2026-09-10.md`（逐条修复 + 证据 + 遗留风险）。
## 0.2.1 stabilization in progress (2026-09-08, WI-M9-STABILIZATION @ feat/021-stabilization)

- M6-C lease revocation on disconnect: DONE (45473f8) - broker
  revoke_agent_grants takes a reason (Disconnect vs HumanTakeover),
  daemon teardown revokes every negotiated lease; supervisor 27 + daemon
  lib 69 + lease_disconnect integration all green.
- M6-C fixed-port envelope: DONE (36ac405) - LocalServer::bind_on,
  daemon envelope binds 37771 (single instance + probe + connect in one);
  InstanceGuard lock-file only; ADR-0019 d5 implementation record;
  workspace tests all green (split_brain 5/5 unchanged).
- M6-C TODO inventory: markers closed with the fixes; remaining markers
  are the M6-C4 pair (browser.rs 634/712).
- M6-C4 navigation state sync: DONE (c79b694 daemon + c56a8e6 desktop) -
  browser.navigate / browser.load-failed envelope methods, provider
  record_navigation, desktop URL-aware mirror + fail-open out-of-line
  reports; TODO(M6-C4) markers closed; C4 remainder (daemon-initiated
  navigate/snapshot, handover re-attach) filed in ROADMAP deferred.
- live-daemon-qa CI: evidence upload added to ci.yml (every main-push
  run keeps its QA evidence artifact).
- Squash-merged to main @ e08e8c6 (2026-09-08) and pushed to origin; feature branch deleted. WI-M9-STABILIZATION done. Next: verify CI on main, then 0.3.0 workbench Phase 1.

## Roadmap decided (2026-09-08)

See docs/roadmap/ROADMAP-V021-PLUS.md: 0.2.1 stabilization debt → 0.3.0
dev workbench Phase 1 (file manager + git panel, human-only, environment
linking) → 0.4.0 startup rollback (ADR first) → 0.5.0 workbench Phase 2
(SSH). Deferred: timer UI design, terminal automation, B2 multi-profile,
artwork commercial rights.

## v0.2.0 released (2026-09-07)

- Theme (3ec677b) + browser multi-session tabs (5569545) + stability
  fixes shipped; tag moved to 1a63c5d after fmt/clippy/specs/CI-sidecar
  gate fixes (all green); SLSA attestation enabled (repo public).
- Windows packages locally signed (self-signed cert 1B6A576C); SBOMs
  uploaded (12 files); Chinese release notes; release branch synced.
- Open backlog: 0.2.1 stable items (M6-C/C4 TODOs, live-daemon-qa CI),
  timer UI (WI-M9-TIMER-UI), startup rollback (WI-M9-STARTUP-ROLLBACK).

## Merged on main (2026-09-07)

- 3ec677b DeepSeek character theme (WI-M8-DEEPSEEK-UI) squash-merged to
  main; artwork stays CC BY-NC-SA (non-commercial) — release requires
  separate rights resolution.
- 5569545 browser multi-session tabs (WI-M9-BROWSER-TABS) squash-merged
  to main after rebasing onto the theme: per-session tab strip with
  document titles (title_changed events), open-URL-never-displaces
  semantics, orphan-session cleanup on list, daemon ownership release on
  disconnect, browser.close returns the closed report. All four daemon
  fixes verified live by the user.
- main @ 5569545 pushed; feature branches deleted.
- Remaining: M8-E v0.1.0 release gates; release branch sync; artwork
  rights / asset size (19MB PNGs) before any themed release.


- Phase：`shell-mvp`
- Milestone：M8 Stable Candidate（已合并 main @ 4c21489）+ **M8-E v0.1.0 发布（进行中，被两个发布门 blocker 门控）**
- Status：M1–M8 全部合并 main；wizard 分支 feat/wizard-repo-source 已 squash 合并 main @ 23c5027；ux-polish 分支进行中
- Implementation authorized：`true`
- External baseline verified：2026-08-25（dsh-std 刷新至 3df0543 / core rc.1）
- Last updated：2026-09-04T13:26:09Z

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
  ✅ daemon client 自动重连完成（2026-09-04）：AutoReconnectConnector 包装（daemon_client.rs，lib.rs 零改动）——连接类失败（Transport/Timeout/NotConnected）→ 单飞重连 + 重试一次 + 2s×2 退避至 30s；closed_result 错误分类 Remote→Transport 修正；152 Rust 测试全绿（含真 daemon e2e 断连恢复）。
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
  📌 2026-09-04 晚间收尾（阶段 1 首日并行推进，三线全收）：
  ✅ usage 接入 4e66765：desktop 快照并入 active 环境 cost-meter ledger（source=dsh、per-session、CNY、fail-open）；根因修正=掐断对话不影响记账（cost-meter 按调用记录，当日隔离实例 21 calls 全在录），问题是面板从未读该源；前端同刻 period 单显；Rust 147 全绿。
  ✅ 缩放修复 768c36a：CSS 弹性链（根因=shell-content max-width 1180px + dsh-surface-slot min-height 540px 钉死 → bounds 不变 → 后端 layout 不更新）；tsc/vitest 104 全绿；实机待 GUI。
  ✅ zh 润色 2f927c1：云端 worker（t-0043）润色 41 条 → 12 条改进仅落 zh 区。
  ✅ daemon 自动重连 3c0af85 + 214293e：AutoReconnectConnector（连接类失败→单飞重连+重试+2s×2 退避至 30s；closed_result 错误分类修正）；152 Rust 全绿（含真 daemon e2e）。
  剩余（阶段 1 续）：①GUI 综合实机验证（usage/缩放/终端/浏览器四项效果）②live-daemon-qa 入 CI（待设计）③M6-C×3（daemon lease 撤销、envelope 固定端口）④M6-C4×2（browser 导航状态上报）⑤release attestation（repo public 后启用）。
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