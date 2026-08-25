# Architecture Invariants

以下规则只能通过新 ADR 明确替代，不能被实现细节隐式突破。

1. Desktop Shell 不拥有或分发 DSH Core。
2. Desktop 状态存入平台 AppData/Application Support，不写入 `DSH_HOME`。
3. Managed/Attached 是显式配置；端口存在不能推导 process ownership。
4. 上游 DSH UI 不 fork、不 DOM patch、不获得 native bridge。
5. Arbitrary Browser page 不获得 native bridge。
6. DSH-specific、Cordis-specific type 只存在于 Adapter 内。
7. Capability 使用独立 `apiVersion + kind`，不使用全局 DesktopProtocolVersion。
8. `dsh-std` 为 optional adapter，核心不 import 其 alpha types。
9. Agent native action 经 DSH permission/tool layer 和 Desktop grant 双重边界。
10. Terminal/Browser 的 Human Surface 与 Agent Automation 分权。
11. Plugin management、Agent Scheduler 和 usage semantics 继续属于 DSH。
12. Supervisor restart 是 HMR 的可靠兜底，Desktop 不实现 Cordis HMR。
13. Local transport 默认不可从 LAN 访问；authentication 不塞入业务 envelope。
14. 日志、诊断和 tracking 不存 credential、token 或原始 session 内容。
15. Compatibility failure 必须显式 degraded/unavailable，不猜测成功。
16. P0 Capability Broker 归 Desktop Supervisor boundary；Adapter 不能直接拥有 provider，也不能把 contract validation 当作授权。
17. 所有 custom Tauri commands 必须登记到 AppManifest 后再由最小 permission 与精确 Shell label 授权；invoke-handler-only command 禁止进入实现。
