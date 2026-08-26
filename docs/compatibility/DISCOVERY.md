# DSH Discovery

优先级：

1. Environment 显式 path/command。
2. 明确配置的 `DSH_PATH`。
3. source checkout launch recipe。
4. PATH 中 `dsh`。
5. 已知 global install discovery。

不提供 private/bundled fallback。

Validation 产生 resolved launch plan，包含 source type、canonical path、cwd、command/args、Node override、DSH_HOME、Profile、endpoint 与可识别版本。Validation 不执行安装、不修改 Profile、不写 DSH_HOME。

## Managed Launch Normalization

对当前已知 DSH Web profile，Supervisor 拥有以下保留参数，Environment 中的用户 `args` 不得重复或覆盖：

- `--host 127.0.0.1`：禁止 LAN/all-interface binding。
- `--port 0`：当 endpoint port 为 `auto` 时由 OS 分配；固定端口则传入已验证的数值。
- `--no-open`：Desktop 自己承载 Web Surface，Managed launch 不打开系统浏览器。

默认 Web profile 可使用 `dsh web`；命名 profile 使用结构化 `--profile <name>` recipe。启动后只把 DSH 输出的 loopback URL 当作 endpoint candidate，必须再验证 process identity、host、port 与 readiness probe，之后才发布 canonical endpoint。保留参数、URL 输出或 CLI 语法变化必须触发 adapter/fixture refresh，不能按版本号猜测。

Source checkout 必须由用户预先准备可运行产物与显式 recipe。Desktop 不运行 `pnpm install`、`pnpm run build`、package update 或任何隐式 bootstrap；缺少产物时返回 `UNAVAILABLE` 并给出诊断。

未知自定义启动作为高级模式，只保存 executable 与 argv 的结构化表示和风险提示；不得保存或执行 shell command string，不做变量、管道、重定向或命令替换。Credential 不得进入 argv，且高级模式不能绕过 ownership/path policy。
