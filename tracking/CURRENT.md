# Current Project State

- Phase：`shell-mvp`
- Milestone：M7 Setup Wizard + Multi-Profile (B1) —— 实现完成（向导 + 环境列表切换），live QA 6/6，待独立评审
- Status：M1-M6 全部合并 main；M7 实现完成（分支 codex/wi-m7-wizard @ 待评审）
- Implementation authorized：`true`
- External baseline verified：2026-08-25（dsh-std 刷新至 3df0543 / core rc.1）
- Last updated：2026-08-30T13:30:00Z

## 当前状态

- M0/M1/M2/M3：done（均已接受并 squash 合并 main；local-transport 非阻塞 socket 修复 e352b0d 也在 main）。
- M4 Shared Browser（codex/wi-m4-browser @ 45bfdf3，已推送）：
  - M4-A 契约冻结：ADR-0017（Browser 与 DSH Surface 分权、human_surface only、profile 隔离、provider 抽象）+ specs/browser/ 6 schemas + 14 fixtures（59/69 ALL PASS）+ AC-BRW-001/003/004 细化、AC-BRW-002 延 M5。
  - M4-B 双 PoC（POC-M4B-REPORT.md）：WebView2 embedded 与 Edge+CDP 全部 PASS；maintainer 拍板 WebView2 默认、CDP 搁置（M6 revisit）。
  - M4-C 实现：crates/browser-provider（30 tests）+ desktop 桥 browser.rs（33 ACL、11 tests、WebView2 独立 profile data_directory、webview2-com 权限拦截、browser://event）+ Browser UI（13 vitest）；全量门禁 173 Rust / 43 vitest / 33 ACL / 59-69 specs / fmt+clippy 全绿。
  - 关键决策（ADR-0017 决策 6）：wry 0.56 升级不可行（tauri 2.11.5 锁定 wry 0.55.1），permission/capture 用 webview2-com 0.38.2 直调（M1 同款模式）。
- 环境（2026-08-29 补齐）：Rust 1.98.0（rustup）+ MSVC 14.51 + Windows SDK（D:\\Windows Kits，26100/22621）+ 前端依赖恢复；本机可完整跑 cargo/vitest 门禁。

## remaining

- **M7**：配置向导（WI-M7-SETUP-WIZARD：6 步引导 + discover_profiles + probe_port）、
  多 profile 切换 B1（WI-M7-MULTI-PROFILE-B1：环境列表面板 + 单活跃切换）。
- **M8 候选**：Stable Candidate（三平台加固、签名/SBOM；含 named mutex/ADR 修订评估）。
- **M9/M10 候选**：B2 并发多 profile（per-environment supervisor + 端口分配器 → 多 surface tab），
  详见 PLAN-B2-MULTI-PROFILE-CONCURRENT.md。
- M6 遗留：browser 状态上报 daemon（Shell browser.rs TODO(M6-C4)）、handover 会话接管
  （daemon TODO(M6-C)）、envelope 固定端口（TODO(M6-C)）、dsh-surface bootstrap token 验证。
- flaky 根治（M6-E，2026-08-30）：① local-transport concurrency_limit —— Windows RST 竞态（reject_busy 未读 hello 即关闭 → RST 清空 reply），修复为 reject 前先短超时读帧；② local-transport malformed_handshake_rejected —— 计数顺序竞态（rejected_auth 在 reply 后才 +1），修复为所有 stats 计数先于可观察信号（reply/槽位释放）；③ diagnostics ac_log_001 —— C4 重写为 mock fixture 后消除。三个均 20-40 次压力验证 0 失败。
- M3 remaining：TerminalPanel 前端自动化用例恢复；macOS/Linux target-host 证据；diagnostics 专项 UI。
- 待跟进（用户 2026-08-29 提及）：DSH 自身偶发崩溃 exit 3221226505（=0xC0000139 STATUS_ENTRYPOINT_NOT_FOUND，与 GUI 测试崩溃同码，疑原生模块依赖系统 DLL 入口点问题）。

## 当前门禁

`implementation_authorized: true` 允许在已认领工作项范围内进入实现，但不豁免 branch/session/lease、接口优先、ADR、模块安全审查、clean-room 与验证证据要求。

## 下一动作

M7 独立评审（REVIEW-M7）→ 验收合并 → M8 Stable Candidate 规划（三平台、签名/SBOM、
named mutex 评估）。B2 并发多 profile 定位 M9/M10（见 PLAN-B2）。