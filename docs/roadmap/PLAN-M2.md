# M2 Reliable Runtime — Execution Plan (2026-08-28)

> 规划先行：本计划先冻结范围与切片，每个切片按 contract-first（ADR/Schema/fixtures → 实现 → 门禁 → 证据 → tracking）执行。
> 本阶段明确搁置：macOS/Linux target-host 测试；需要交互的 GUI/WebView2 smoke（保留 driver 与既有证据，不重跑）。

## M2 退出标准（来自 M2.yaml / ROADMAP / ACCEPTANCE.md）

1. Supervisor / process ownership / health / restart 通过 chaos（AC-OWN、AC-RUN-001..006、AC-REC-001）。
2. Native IPC 与 fallback 通过 security tests（AC-IPC-001/002）。
3. Safe Stop 与 diagnostics 可用（AC-REC-001、AC-LOG-001）。

## 切片划分（每片独立 commit + 门禁 + evidence）

### M2-A Supervisor restart / recovery / Safe Stop（AC-RUN-001/002、AC-REC-001）
- 契约：ADR-0013（restart/recovery 策略：crash-loop fuse、恢复预算、Safe Stop、generation 语义）；managed-runtime-report schema 增加 recovery/safeStop 字段；restart request schema；fixtures；IF-RUNTIME-CONTROL operations += restart。
- 实现（apps/desktop/src-tauri/src/managed_runtime.rs）：crash 检测（child exit + readiness 状态）、auto-restart with budget（复用 policy.autoRestartOnCrash）、crash-loop fuse → Safe Stop、restart 命令（同 environment 新 generation，Surface binding 恢复）、重复 stop/restart 幂等、并发 start CONFLICT、端口占用/抢占/延迟释放、stale PID/PID reuse/foreign process、orphan children 清理。
- 测试：chaos 单元/集成测试（fake child 脚本模拟 crash loop、慢 readiness、恶意输出、foreign endpoint）。
- 前端：Runtime UI 展示 recovery/safe_stop 状态与 evidence。

### M2-B Diagnostics + 安全审计（AC-LOG-001）
- 契约：diagnostics report schema + golden corpus（已知 secret 输入 → 断言 redaction）；ADRs 或文档同步。
- 实现：诊断命令（runtime/surface/catalog/process 证据，统一 redaction，token/cookie/凭据不泄漏）。
- 测试：golden corpus redaction 测试、Schema fixture 校验。

### M2-C P0 Capability Broker + local transport（AC-IPC-001/002、AC-LEASE-001）
- 契约：capability-contracts（envelope/lease/coordinate 语义，DSH-neutral）与 local-transport（framing/credential/ACL/fallback）两个 crate；IF-NEGOTIATION/INVOCATION/LEASE 实现授权。
- 实现：
  - crates/local-transport：长度前缀 framing（max frame）、ephemeral credential 认证、ACL/mode、deadline/concurrency 限制、loopback TCP fallback、cleanup。
  - packages/capability-contracts（TS）：envelope/lease/coordinate 类型与校验（若为 npm package 则按包约定）。
  - supervisor 内 P0 Broker：grant/lease/scope/generation enforcement + provider dispatch 骨架（最小可用，服务 AC-IPC/AC-LEASE）。
- 测试：invalid/replay/stale credential、oversized/slow client、lease revocation 测试。

### M2-D Supervisor/process-manager crate 抽取（ADR-0008 P0 API 门禁）
- 将 managed_runtime 的 tauri-agnostic 核心（launch spec、candidate 解析、process tree、generation/ownership 状态机）抽取到 crates/process-manager 与 crates/supervisor；apps/desktop 保留 tauri 胶水。
- 要求：现有 Schema/generation/ownership 语义与 M1 测试原样保留（模块文档要求）。
- 若抽取风险过高，以最小可验证增量进行并在 handoff 中如实记录剩余。

## 收尾审查与加固（2026-08-28 完成）

- 4 份独立 REVIEW（REVIEW-M2-HANDOFF-CONSISTENCY / REVIEW-M2-HARDENING-SECURITY / REVIEW-M2-HARDENING-REDUNDANCY / REVIEW-M2-HARDENING-DOCS）。
- 安全 must-fix 已修复：FH-1（CSPRNG credential，getrandom）、FH-2（credential TTL 清理、有界队列、拒绝路径不建线程）、FM-1（HTTP 级 readiness 探测防端口抢占）、FM-4（status/binding environment 交叉校验）；新增回归测试。
- 工具证据：cargo audit 0 漏洞（9 条传递依赖警告均为 unmaintained/仅 Linux 构建）；tauri-doctor 2 MED 均核验为误报（校验逻辑/测试代码）；clippy pedantic 作为审查输入。
- 精简：死代码 `_policy_is_present` 移除；文档-代码一致性 21 项统一（schema/ADR/README/tracking/数字口径）。
- 注释：安全不变量注释（verified_surface_binding、deny-order、record_crash、broker double-check、local-transport # Errors/# Panics、命令 async 理由）。
- 遗留 backlog（medium/low，已记入 REVIEW-M2-HARDENING-SECURITY）：FM-2 策略参数顺序、FM-5 restart stop 失败、FM-6 endpoint 释放 TOCTOU、FM-7 WebView2 profile 隔离、FM-8 start 重置预算等。

## 执行顺序

1. M2-A 契约冻结（ADR-0013 + Schema + fixtures）→ 提交。
2. M2-A 实现 + chaos 测试 + 门禁 → 提交。
3. M2-B 与 M2-C 并行（子代理，文件隔离）→ 各自门禁 → 提交。
4. M2-D 抽取（若可行）→ 全仓门禁。
5. 汇总：ACL、前端、全量测试、tracking（WI/MOD/IF/CURRENT/project）、handoff、review。

## 明确不做的（本阶段）

- macOS/Linux unsupported_platform target-host 实测（保留结构性证据）。
- WebView2 交互式 GUI smoke 重跑（保留 26/26 证据与 driver）。
- M6 daemon、M3 之后的功能。
