# dsh-std Compatibility Policy

`verified_on: 2026-08-25` · `verified_at: 2026-08-25T12:19:55Z`

dsh-std 在 `main@bb194ad53a72f4fa7da1286c88dcebb488b43eb9` 仍将代码与提案标为 early drafts。npm registry 同时给出 `latest=0.1.0-rc1` 与 `rc=0.1.1-rc.1`；标签指向是 registry policy，不能由 semver 大小推导。项目采用 meta-protocol、独立版本、requires/supports、adapter、activation ownership 等概念，但：

- core 不依赖 `@dsh-std/*`。
- alpha type 不穿越 adapter。
- 不把私有 Desktop capability 冒充标准协议。
- 声明 conformance 必须绑定精确 package version、artifact integrity 和 fixture；不得只写 `latest` 或 `rc`。
- std absent/known/unknown 都有测试。
- Connection/wire 稳定后再评估 carrier mapping。

来源：[immutable dsh-std README](https://github.com/Yan-Zero/dsh-std/blob/bb194ad53a72f4fa7da1286c88dcebb488b43eb9/README.md)；完整 registry 坐标见 [External Baseline](../research/EXTERNAL_BASELINE.md)。
