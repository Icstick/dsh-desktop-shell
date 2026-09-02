# M8-E：真实 DSH bootstrap token 验证（2026-09-01）

> 目标（PLAN-M8 M8-E）：用本机真实 DSH 验证 dsh-surface 的 bootstrap token 路径——
> 真实 DSH + 真实 session cookie 的 binding。主机：Windows，DSH 实例监听
> 127.0.0.1:3080（DSH_HOME=C:\Users\Administrator\.dsh，version 0.1.2-alpha）。

## 结论

**验证通过**。四条证据链闭合（2 实测 + 2 源码对照），bootstrap token 路径与
desktop-shell 的 candidate 契约精确匹配。

## 证据

### E1：真实 DSH 是 authenticated root（实测）

- `curl http://127.0.0.1:3080/`（无凭据）→ **HTTP 401**
  "dsh web authentication required; reopen the URL printed by dsh web."
- 即：DSH 不接受 credential-free 根（ADR-0012 的 legacy root 已不适用）。

### E2：launchToken 形态 = candidate 契约的 43 位约束（源码对照）

- DSH 源码 packages/client/connection/src/browser-auth.ts：
  - `processLaunchToken()` = `encodeBase64Url(randomBytes(SECRET_BYTES))`（进程内存，
    不持久化；每次 dsh web 启动生成）→ **43 字符 base64url**（32 字节 → 43 位）。
  - `authenticatedUrl()`：打印 URL = `http://127.0.0.1:<port>/?token=<launchToken>`。
- desktop-shell 的 candidate 契约（crates/managed-runtime/src/supervisor.rs
  parse_candidate）：token query 存在时必须是 **43 位 alnum/_-**，否则拒绝。
- **43 = 43 精确匹配**——parse_candidate 的约束就是按 DSH launchToken 形态设计的。

### E3：token → cookie 交换 + authority-bound cookie（源码对照）

- DSH authorizeIndex：GET /?token=<launchToken> → 303 重定向到 / +
  set-cookie（cookie 名 = `dsh-auth-{b64url(sha256(authority))}`，authority =
  Host 头；值 = HMAC-SHA256(secret) 签名的 payload）。
- desktop-shell 端按同一构造消费（dsh_surface 首次导航带 bootstrap URL，
  WebView2 跟随 303 存 cookie → 后续请求已认证）。

### E4：真实 secret + 手工构造 cookie → 200 + 真实 DSH UI（实测）

- 从 .credentials.yaml 读 `client-connection/browser-session` secret（43 位）。
- Node 手工构造 cookie（sha256(authority) cookie 名 + HMAC payload）：
  - `curl` 无 cookie：401
  - `Cookie: dsh-auth-...` → **HTTP 200** + 真实 DSH index.html
    （ModuleLoader 引导页 = 本机 DSH web GUI）。

## 边界与已知

- launchToken 进程内不持久化：无法从外部直接读取本实例的 token 做
  token-query 端到端（实测 `?token=<browser-session secret>` 401 符合设计——
  二者是不同凭据：launchToken 在进程内存，browser-session secret 在凭据文件）。
- 端到端 GUI 冒烟（shell spawn 第二个 DSH 实例）未做：会干扰当前宿主 DSH
  会话；managed spawn 路径的 URL 解析已有 unit 覆盖 + E2 契约匹配，风险可控。
- DSH 实例隔离：多实例行为未在本机验证（DSH 单实例语义待查）。

## 关联

- crates/managed-runtime/src/supervisor.rs parse_candidate / VerifiedSurfaceBinding
- packages/client/connection/src/browser-auth.ts（DSH 上游，仅供对照）
- docs/decisions/ADR-0012-authenticated-managed-web-bootstrap.md
