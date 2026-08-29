---
id: ADR-0012
status: accepted
date: 2026-08-28
owner_role: architecture-owner
---

# ADR-0012: Authenticated Managed Web Bootstrap

## 背景

真实 user-owned DSH source checkout `master@cd5ef8148158c3a752a658978873241fdf8e2bbc` 在 Windows 上验证出两项兼容性变化：预构建 CLI 入口是需要 Node 启动的 `apps/cli/lib/bin.js`；`dsh web:` readiness URL 是 `http://127.0.0.1:<port>/?token=<ephemeral-token>`。DSH 只在根路径交换该 token，设置 authority-bound signed cookie 后重定向到不含 query 的根页面。

既有 M1 Supervisor 正确拒绝了 query candidate，也拒绝 `nodePath`，因此没有泄漏或误发布 endpoint；但它无法启动当前预构建 checkout，也无法把当前 generation 的认证 bootstrap 交给原版 DSH Web UI。用 wrapper、caller-supplied URL、DOM injection 或日志复制 token 都会破坏 ownership、clean-room 或 WebView boundary。

上述外部事实于 2026-08-28 核验，精确 revision、blob 与 registry 坐标见 [External Baseline](../research/EXTERNAL_BASELINE.md) 和 [Source Register](../compliance/SOURCE_REGISTER.yaml)。

## 决策

1. `DshEnvironment.nodePath` 只允许用于 `ownership=managed`、`harness.mode=repository` 的用户预构建 JavaScript 入口。Supervisor 以 `nodePath` 为 executable、以 `harness.path` 为第一个 literal argument，再追加自己拥有的 profile、loopback、port 与 `--no-open` 参数；不经过 shell、package manager、install、build 或 bootstrap。
2. Node executable 与预构建入口都必须是用户显式配置的绝对现存文件。其他 source mode 携带 `nodePath`、相对路径、缺失文件或未构建 checkout 均 fail closed。
3. Owned child 的 `dsh web:` candidate 只接受两种根 URL：legacy credential-free `http://127.0.0.1:<port>/`，或当前 authenticated `http://127.0.0.1:<port>/?token=<43-char-base64url>`。任何其他 query、重复 token、fragment、username/password、另一 host/scheme/path、fixed-port mismatch 或 overlong line 均拒绝。
4. 完整 authenticated bootstrap URL 只保存在 Supervisor 当前 generation 的私有内存中，不进入 Runtime report、Surface status、Shell IPC、catalog、日志、诊断、tracking 或 error。公开 endpoint 继续只有 scheme、host 与 port。
5. `VerifiedSurfaceBinding` 可把该 backend-owned bootstrap URL直接交给 fixed-label `dsh-surface` 的首次 native navigation。它不接受 caller URL，不赋予 privileged IPC，也不通过 initialization script、page eval 或 DOM injection 传递 credential。
6. WebView2 在远程 load 前继续安装 permission/autofill/password deny。初始 token exchange 与 DSH 的 clean-root redirect 都必须保持 verified exact origin；cross-origin、popup、download 与 permission 继续拒绝。
7. bootstrap credential 与 owned process tree、instance 和 generation 同生命周期。stop、exit、failed readiness、generation change 或 Supervisor drop 必须同时丢弃；stale generation 不得复用。

## 替代方案

- 要求用户创建 `.cmd` wrapper：会把 Windows shell quoting 和 wrapper ownership 推给用户，且绕过已有结构化 `nodePath` 字段，拒绝。
- 继续只接受 credential-free URL：无法加载当前 DSH authenticated Web UI，且会诱导关闭上游认证，拒绝。
- 把 token 返回 Shell 再拼 URL：扩大 IPC、日志和前端暴露面，拒绝。
- 删除 query 后直接加载根页面：DSH 会在 RPC 前返回 401，不能伪装为兼容。
- 持久化 cookie/token 供重启恢复：credential 属于单一 process generation，拒绝。

## 后果

- Windows 可直接消费用户已构建的 DSH checkout，而 Desktop 仍不安装、构建或分发 DSH。
- Runtime/Surface public serialization 不变；Environment Catalog v1 仅收紧已存在 `nodePath` 的 mode/ownership 语义。
- backend 内部 binding 从 sanitized origin 扩展为 sanitized endpoint + 私有 bootstrap URL，需要 secret-lifecycle、negative parser 和 redaction review。
- legacy credential-free release fixture 与 current authenticated advisory checkout 可同时进入 compatibility matrix，不能按版本号猜测行为。

## 验证门禁

- Schema/validation 拒绝非 Managed Repository 的 `nodePath`，并覆盖绝对/缺失 Node 与入口文件。
- structured launch test 证明 argv 顺序为 `node <built-entry> [--profile ...] web --host 127.0.0.1 --port ... --no-open`，没有 shell/package manager。
- candidate tests覆盖 legacy root、exact token root、重复/畸形 token、其他 query、fragment、credential、host/port mismatch 与 stale generation。
- Runtime report、Surface status、error/evidence 和 tracking 的序列化测试证明不含 token、query 或完整 bootstrap URL。
- 真实 user-owned DSH Windows smoke 证明 token exchange 后进入 clean exact-origin page，并完成 resize/hide/show/reload/unmount；其余 WebView2 negative matrix仍按 ADR-0011 执行。

## 受影响模块

- `MOD-SUPERVISOR`
- `MOD-HARNESS-SURFACE`
- `MOD-SHELL-UI`

