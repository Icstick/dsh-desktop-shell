# Review Checklist

- 是否违反 architecture invariant？
- Public contract 是否有 Schema、version、fixture、migration？
- Attached 是否可能触发 mutation？
- WebView/Agent 是否获得过宽权限？
- Resource 是否绑定 lease/owner/generation 并可回收？
- Error 是否 fail closed 且不泄漏内部数据？
- Windows/macOS/Linux 行为是否有证据而非推断？
- Third-party 来源、license、copied/adapted 状态是否登记？
- Agent 生成代码是否经过相应人工安全 review？
- Tracking、evidence、handoff 是否更新？
