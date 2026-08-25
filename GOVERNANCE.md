# Governance

## 决策层级

1. Apache-2.0 与安全/法律要求。
2. 已接受 ADR 和 `docs/architecture/INVARIANTS.md`。
3. `specs/` 中的版本化公开契约。
4. Milestone acceptance criteria。
5. 模块级文档与工作项。

发生冲突时按上述顺序处理，并通过 ADR 修正，而不是在实现中绕过。

## 角色

- Maintainer：合并、发布、里程碑和治理最终责任。
- Architecture Owner：ADR、接口与模块边界。
- Runtime Owner：Supervisor、process、transport。
- UI Owner：Shell 与 Desktop surfaces。
- Interop Owner：DSH 和 dsh-std adapter。
- Security Owner：trust boundary、权限与供应链。
- Test Owner：fixture、compatibility、chaos 与 release evidence。

实际 GitHub 账号未确定前只维护 `CODEOWNERS.template`，不创建无效 CODEOWNERS。

## 争议处理

- 先记录可验证事实、受影响不变量和备选方案。
- 高影响争议创建 ADR；安全或许可问题默认 fail closed。
- 无法及时解决时将工作项标记为 `blocked`，不得把未决假设固化进接口。
