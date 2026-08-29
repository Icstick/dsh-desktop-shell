# DSH Discovery

优先级：

1. Environment 显式 path/command。
2. 明确配置的 `DSH_PATH`。
3. source checkout launch recipe。
4. PATH 中 `dsh`。
5. 已知 global install discovery。

不提供 private/bundled fallback。

## M1 non-executing discovery contract

首轮 discovery 只读取显式 path、`DSH_PATH` 与 `PATH` 目录，并返回结构化 evidence；它不启动候选、不执行 `--version`、不调用 npm/pnpm、不解析 shell command string。已知 global install discovery 在 report 的 `deferredSources` 中显式标记，直到独立 adapter fixture 与供应链门禁就绪。

- 普通文件候选只证明路径存在，不证明 DSH 兼容；`version` 保持 `null`。
- 目录候选标记为 `requires_recipe`，必须由用户提供预构建 launch recipe。
- 显式缺失路径保留为 `missing` evidence，方便用户诊断；PATH 扫描只返回实际存在的候选。
- canonical path 用于去重，但 discovery 不把路径写入日志、tracking 或 error message。
- discovery report 是瞬时证据，不是 process ownership；选择候选后仍需执行 `DshEnvironment` validation。

规范真源见 `harness-discovery-request.schema.json` 与 `harness-discovery-report.schema.json`。

Validation 产生 resolved launch plan，包含 source type、canonical path、cwd、command/args、Node override、DSH_HOME、Profile、endpoint 与可识别版本。Validation 不执行安装、不修改 Profile、不写 DSH_HOME。

## Managed Launch Normalization

对当前已知 DSH Web profile，Supervisor 拥有以下保留参数，Environment 中的用户 `args` 不得重复或覆盖：

- `--host 127.0.0.1`：禁止 LAN/all-interface binding。
- `--port 0`：当 endpoint port 为 `auto` 时由 OS 分配；固定端口则传入已验证的数值。
- `--no-open`：Desktop 自己承载 Web Surface，Managed launch 不打开系统浏览器。

默认 Web profile 可使用 `dsh web`；命名 profile 使用结构化 `--profile <name>` recipe。启动后只把 DSH 输出的 loopback URL 当作 endpoint candidate，必须再验证 process identity、host、port 与 readiness probe，之后才发布 canonical endpoint。保留参数、URL 输出或 CLI 语法变化必须触发 adapter/fixture refresh，不能按版本号猜测。

Source checkout 必须由用户预先准备可运行产物与显式 recipe。Desktop 不运行 `pnpm install`、`pnpm run build`、package update 或任何隐式 bootstrap；缺少产物时返回 `UNAVAILABLE` 并给出诊断。

未知自定义启动作为高级模式，只保存 executable 与 argv 的结构化表示和风险提示；不得保存或执行 shell command string，不做变量、管道、重定向或命令替换。Credential 不得进入 argv，且高级模式不能绕过 ownership/path policy。
