# DSH Discovery

优先级：

1. Environment 显式 path/command。
2. 明确配置的 `DSH_PATH`。
3. source checkout launch recipe。
4. PATH 中 `dsh`。
5. 已知 global install discovery。

不提供 private/bundled fallback。

Validation 产生 resolved launch plan，包含 source type、canonical path、cwd、command/args、Node override、DSH_HOME、Profile、endpoint 与可识别版本。Validation 不执行安装、不修改 Profile、不写 DSH_HOME。

未知自定义启动作为高级模式，只保存 executable 与 argv 的结构化表示和风险提示；不得保存或执行 shell command string，不做变量、管道、重定向或命令替换。Credential 不得进入 argv，且高级模式不能绕过 ownership/path policy。
