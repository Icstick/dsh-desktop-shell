# Migration Policy

任何持久配置或 public protocol 变化必须：

1. 标识 source/target version。
2. 定义 forward 与 rollback。
3. 保留用户原数据并先备份。
4. 提供 dry-run/validation。
5. 更新 Schema、compat matrix、changelog、support。
6. 明确 Desktop state 与 DSH_HOME 分离。

P0 不迁移 DSH_HOME/Profile。未来 daemonization、transport 与 Environment schema 迁移需独立 ADR。

## M1 Environment Catalog v1

- Source：无 Desktop-owned persisted Environment；Target：`environment-catalog/v1`。
- Forward：首次成功保存时在平台 AppData/Application Support 创建 catalog；revision 从 0 递增，按 Environment ID upsert 并保留最多 128 条。
- Rollback：旧版本忽略 catalog；不得删除或迁移 `DSH_HOME`。写入实现必须保留可恢复 backup，读取主文件失败时只显式报告，不猜测或静默覆盖。
- Validation：保存前执行 `DshEnvironment` validation；active ID 必须引用 catalog 内记录；fixtures 与 Rust contract tests 同时通过。
- Discovery report 是瞬时只读 evidence，不持久化、不执行候选，因此没有运行时数据迁移。

## M1 DSH Surface Policy v1alpha1

- Source：只有文档级 WebView isolation invariant；Target：`webview.dsh-desktop.local/v1alpha1` 的 policy derivation 与 navigation decision contract。
- Forward：从 persisted fixed-loopback Environment 临时派生 policy；不写 catalog、DSH_HOME/Profile 或 WebView storage。
- Rollback：删除未消费的瞬时 descriptor/decision 即可；本切片不创建 WebView，也没有持久数据需要回滚。
- Validation：Schema/fixture matrix、Rust evaluator tests、Shell-only AppManifest/permission/capability inventory 和 `dsh-surface` 负向 ACL gate 同时通过。

## M1 Managed Runtime v1alpha1

- Source：Shell snapshot generation 固定为 0，Managed runtime 只显示 `stopped`；Target：瞬时 integrated Supervisor state，包含 generation、opaque instance ID、retained process-tree handle 与 verified endpoint publication。
- Forward：首次 Managed start 在内存中产生 generation 1；不修改 Environment Catalog、DSH_HOME/Profile 或 DSH installation。Shell 退出时 retained process tree 必须清理。
- Rollback：停止 owned process tree 并丢弃内存状态；没有持久 lifecycle 状态需要迁移或降级。
- Validation：request/report Schema、valid/invalid fixtures、structured argv、owned output candidate、bounded readiness、stale generation、Attached hard deny、process-tree cleanup 与 exact Shell-only ACL 同时通过。

### Authenticated bootstrap and repository Node recipe

- Source：Environment Catalog v1 已包含 optional `nodePath`，但 M1 初始 runtime 对任何 Node override fail closed，且只接受 credential-free `dsh web:` root；Target：ADR-0012 限定的 Managed Repository structured Node recipe 与 backend-only authenticated bootstrap。
- Forward：现有 executable Environment 无需改动。用户预构建 source checkout 可显式保存绝对 `nodePath` 与绝对 built `harness.path`；首次 start 只在内存中保存 current-generation bootstrap URL，catalog 不保存 token。
- Rollback：旧版本会对 non-empty `nodePath` 返回 unavailable，不会执行或改写 checkout。停止 owned generation 会删除 bootstrap credential；Environment record 可保留供新版再次使用，DSH_HOME/Profile 不迁移。
- Validation：Catalog v1 Schema 将 `nodePath` 收紧为 Managed Repository-only；valid/invalid fixtures、structured argv、legacy/authenticated candidate、token redaction、generation cleanup 与 real DSH smoke 同时通过。

## M1 DSH Surface Lifecycle v1alpha1

- Source：只有 policy evaluator 与 placeholder Surface；Target：generation-bound native mount/status/layout/reload/unmount 和 sanitized lifecycle state。
- Forward：Windows 从当前 verified Managed binding 创建 fixed-label `dsh-surface` child；不写 Environment Catalog、DSH_HOME/Profile 或 route state。macOS/Linux/other 返回 `unsupported_platform`，不创建持久状态。
- Rollback：unmount child 并丢弃内存 lifecycle state；不停止 Managed runtime，不删除 WebView profile或用户数据。
- Validation：request Schema 拒绝 caller endpoint/origin/URL/label/permission，status fixtures、stale generation、Windows permission/navigation/popup/download negative tests、非 Windows fail-closed 与 exact Shell-only ACL 同时通过。
- Implementation state：瞬时 lifecycle 已实现且不新增持久数据；rollback 仍是 generation-bound unmount。真实 Windows WebView2 smoke 与非 Windows host execution 未完成前，interface 保持 review，不能据此迁移或扩大 platform support。

## M0 v1alpha1 Pre-implementation Correction

- Source：初始 permissive Envelope draft；Target：kind-specific Hello/Agreement/Invocation/Result/Event constraints。
- Forward：Agreement 增加 `replyTo`；Result success payload 与 error 改为互斥；error 强制 correlation ID；CapabilityLease 禁止空 scope；ScheduleWake 改用独立 Schema。
- Rollback：M0 尚无 implementation/persisted message，不提供运行时 rollback；旧 fixture 必须显式标记 superseded。
- Validation：按 `protocol/fixtures/README.md` 的 valid/invalid matrix 校验，且不迁移 `DSH_HOME`。
