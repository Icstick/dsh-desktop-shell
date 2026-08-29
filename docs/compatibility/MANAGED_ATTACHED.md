# Managed and Attached Behavior

| Operation | Managed | Attached |
|---|---|---|
| Connect/render | allow | allow |
| Health/probe | allow | allow |
| Start | allow | not applicable |
| Graceful stop | allow | deny |
| Restart/recovery | allow | deny |
| Process-group force kill | last resort | deny |
| Adapter negotiation | optional | optional |
| Ownership handover | future protocol | future protocol |

UI 和 API 永远返回明确 ownership。Attach endpoint 丢失进入 Detached/Retry，不转换为 Managed。

## M1 Attached health contract

- 请求只携带 persisted `environmentId`；host、port 与 750 ms deadline 由 Desktop backend 解析或固定，调用方不能构造任意 probe target。
- 只对 `ownership=attached` 且 host=`127.0.0.1`、port 为固定整数的 Environment 执行一次 TCP connect；不扫描端口，不发送 HTTP bytes，不执行 Harness。
- connect 成功返回 `state=attached`、`reachability=reachable`，但固定返回 `identity=unverified`、`processOwnership=external` 与 `lifecycleMutation=denied`。
- refused、timeout 或其他 I/O failure 返回 `state=detached` 和最小结构化 evidence；原始 OS error、path、command 与 credential 不跨越 IPC。
- `port=auto`、Managed Environment 或未知 Environment 以标准错误拒绝，不退化为端口搜索或 ownership inference。

规范真源见 `specs/runtime/attached-health-request.schema.json` 与 `attached-health-report.schema.json`。

## M1 Managed lifecycle/readiness contract

- 当前官方 DSH advisory revision 在 Loader tree settle 后输出 `dsh web:` canonical URL；`--no-open` 关闭系统浏览器，`--port 0` 允许 OS 分配端口。`master@cd5ef8148158c3a752a658978873241fdf8e2bbc` 的根 URL携带 fresh process token，根请求交换 authority-bound signed cookie 后重定向到 clean root（`verified_on: 2026-08-28`，[official pinned README](https://github.com/deepseek-ai/deepseek-harness/blob/cd5ef8148158c3a752a658978873241fdf8e2bbc/packages/bundle/web-app/README.md)）。
- Desktop 只接受该 owned child 的 bounded output marker；candidate 必须是 legacy credential-free root，或 exact `http://127.0.0.1:<port>/?token=<43-char-base64url>` authenticated root。完整 bootstrap URL只留在 current Supervisor generation，公开 report 仍只发布 sanitized endpoint；两者都必须通过 process ownership、generation/instance、fixed-port agreement 与 bounded TCP readiness。
- 此 baseline readiness 证明“当前由 Desktop 启动并持有的 Web process 已发布该 loopback endpoint”，不是通用 DSH wire 身份协议，也不能用于 Attached ownership elevation。
- Source checkout 缺少用户预构建入口、Node executable、reserved arg collision或输出/探测超时均返回 `UNAVAILABLE`/`UNAUTHORIZED`。Windows prebuilt JavaScript checkout 只允许 ADR-0012 的结构化 `nodePath + harness.path` recipe；Desktop 不 install、build、调用 package manager或猜测 endpoint。

规范真源见 `specs/runtime/managed-runtime-*.schema.json`。
