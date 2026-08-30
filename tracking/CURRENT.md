# Current Project State

- Phase：`shell-mvp`
- Milestone：M4 Shared Browser —— done（2026-08-30 maintainer 验收，待合并 main）
- Status：M1/M2/M3 已合并 main；M4 验收完成；下一里程碑 M5 Interop（ready）
- Implementation authorized：`true`
- External baseline verified：2026-08-25
- Last updated：2026-08-30T03:00:00Z

## 当前状态

- M0/M1/M2/M3：done（均已接受并 squash 合并 main；local-transport 非阻塞 socket 修复 e352b0d 也在 main）。
- M4 Shared Browser（codex/wi-m4-browser @ 45bfdf3，已推送）：
  - M4-A 契约冻结：ADR-0017（Browser 与 DSH Surface 分权、human_surface only、profile 隔离、provider 抽象）+ specs/browser/ 6 schemas + 14 fixtures（59/69 ALL PASS）+ AC-BRW-001/003/004 细化、AC-BRW-002 延 M5。
  - M4-B 双 PoC（POC-M4B-REPORT.md）：WebView2 embedded 与 Edge+CDP 全部 PASS；maintainer 拍板 WebView2 默认、CDP 搁置（M6 revisit）。
  - M4-C 实现：crates/browser-provider（30 tests）+ desktop 桥 browser.rs（33 ACL、11 tests、WebView2 独立 profile data_directory、webview2-com 权限拦截、browser://event）+ Browser UI（13 vitest）；全量门禁 173 Rust / 43 vitest / 33 ACL / 59-69 specs / fmt+clippy 全绿。
  - 关键决策（ADR-0017 决策 6）：wry 0.56 升级不可行（tauri 2.11.5 锁定 wry 0.55.1），permission/capture 用 webview2-com 0.38.2 直调（M1 同款模式）。
- 环境（2026-08-29 补齐）：Rust 1.98.0（rustup）+ MSVC 14.51 + Windows SDK（D:\\Windows Kits，26100/22621）+ 前端依赖恢复；本机可完整跑 cargo/vitest 门禁。

## remaining

- **M4-D**：live desktop QA（GUI 实际操作：开 browser 窗口/导航/关闭、profile 隔离运行时证据、AC-BRW-001 三层闭合复核）→ 独立评审 → maintainer 验收 → 合并 main。
- 已知 flaky（M2 模块，隔离运行通过）：diagnostics ac_log_001（并行负载下偶发）、local-transport limits（concurrency_limit）、local-transport malformed_handshake_rejected（transport.rs 时序断言）。
- M3 remaining：TerminalPanel 前端自动化用例恢复；macOS/Linux target-host 证据；diagnostics 专项 UI。
- 待跟进（用户 2026-08-29 提及）：DSH 自身偶发崩溃 exit 3221226505（=0xC0000139 STATUS_ENTRYPOINT_NOT_FOUND，与 GUI 测试崩溃同码，疑原生模块依赖系统 DLL 入口点问题）。

## 当前门禁

`implementation_authorized: true` 允许在已认领工作项范围内进入实现，但不豁免 branch/session/lease、接口优先、ADR、模块安全审查、clean-room 与验证证据要求。

## 下一动作

M4-D 收尾（live desktop QA + 评审 + 验收 + 合并 main）→ M5 Interop 规划。
