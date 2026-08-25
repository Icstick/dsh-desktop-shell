# dsh-std Compatibility Policy

`verified_on: 2026-08-25`

dsh-std 当前将代码与提案标为 early drafts。项目采用 meta-protocol、独立版本、requires/supports、adapter、activation ownership 等概念，但：

- core 不依赖 `@dsh-std/*`。
- alpha type 不穿越 adapter。
- 不把私有 Desktop capability 冒充标准协议。
- 声明 conformance 必须绑定精确版本和 fixture。
- std absent/known/unknown 都有测试。
- Connection/wire 稳定后再评估 carrier mapping。

来源：[dsh-std repository](https://github.com/Yan-Zero/dsh-std)。
