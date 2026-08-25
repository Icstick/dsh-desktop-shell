# DSH Discovery

优先级：

1. Environment 显式 path/command。
2. 明确配置的 `DSH_PATH`。
3. source checkout launch recipe。
4. PATH 中 `dsh`。
5. 已知 global install discovery。

不提供 private/bundled fallback。

Validation 产生 resolved launch plan，包含 source type、canonical path、cwd、command/args、Node override、DSH_HOME、Profile、endpoint 与可识别版本。Validation 不执行安装、不修改 Profile、不写 DSH_HOME。

未知自定义命令作为高级模式，保存用户原始输入与结构化风险提示，但不能绕过 ownership/path policy。
