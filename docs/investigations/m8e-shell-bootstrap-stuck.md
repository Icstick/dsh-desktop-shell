# M8-E 调查：Shell GUI 卡在 bootstrap（Reading canonical runtime state）

> 2026-09-02 暂停记录。分支 main @ 0e25ac8 / release @ 47c92a8。状态：暂停，待续。

## 症状

- 本地 release 构建的 dsh-desktop-shell.exe 启动后，GUI 永远停在
  "Shell bootstrap / Reading canonical runtime state…"（HarnessSurface snapshot null）。
- 终端可见 [daemon-client] 日志：initial connect 失败（找不到 daemon）→ retry → connected。
- 界面不进入环境选择/设置向导（snapshot 从未到达）。

## 已确认的事实（证据）

1. **daemon 功能完好**：scripts/qa/live-daemon-qa.mjs 本机 25/25 PASS
   （A1-A18 + B1-B7：握手/协商/ping/status/PTY/browser/scheduler/Shell E2E 全过）。
2. **协商可复现**：手动起 daemon + 裸 socket 诊断（握手 → Hello → Agreement）两次中
   一次通过、一次卡（NO AGREEMENT after 5s）——**偶发**。
3. **daemon 正常监听双端口**：envelope 端口（credential.port，动态）+ claim 37771。
4. 诊断卡住时 daemon 线程全部 Wait（无 100% CPU）= 死锁/阻塞等待，非死循环。
5. 单独 QA 流程（自己 spawn daemon + 全流程）稳定通过——**spawn 后立刻连的路径 OK**。
6. release 与 debug daemon 同样偶发卡（非构建配置差异）。
7. 偶发的一次 QA 跑出 exit -1073740791（0xC0000409 fastfail，疑似与手动杀进程撞车），
   重跑 25/25 过——**非稳定复现**。

## 尚未验证的假设（下一步从这里继续）

A. **Shell 启动竞态**：shell spawn daemon 后立即 connect——daemon 的 broker/registry
   初始化未完成时第一个 Hello 到达 → broker lock 或 activation 路径偶发卡。
   （QA spawn 后 waitPort + 轮询 credential 有 15s 缓冲；shell 的 connect 时序更紧？
   看 daemon_client.rs 的 connect_shell 时序。）
B. **两个连接并发**：shell 连接 + 外部诊断连接同时协商 → broker single-owner grant
   Conflict 路径死锁（handle_hello 的 broker_grant_from_negotiation）。
C. **AppData data dir 残留**：daemon-credential.json 被旧实例写/清 → credential 错位。
   （已做过一次 clean restart 仍卡——概率性。）
D. **前端 snapshot 路径**：getShellSnapshot 的 tauri invoke 在 daemon connector 未就绪时
   永挂（connector() None → Err 应显示错误，但 UI 无错误分支？查 ShellApp snapshotError UI）。

## 调试建议（下次）

1. 复现时抓 daemon 线程栈：Windows 上 `dotnet-dump` / procdump / VS 附加——
   定位死锁点（谁持锁）。
2. shell spawn daemon 加日志：看 daemon_client connect_shell 的精确时序
   （spawn → credential wait → connect → negotiate 各阶段时间戳）。
3. 复现时立即对 daemon 做第二次裸 socket Hello——如果第二个 Hello 也卡 →
   全局死锁；如果第二个过 → 第一个连接持有状态卡住。
4. 试 shell 手动连接（DSH_DAEMON_EXE 指向 QA 已验证的 daemon + 等 daemon 完全就绪
   2s 后再启动 shell）——若稳定过，确认是启动竞态（假设 A）。

## 附带发现（用户报告，待办）

- **i18n**：界面切中文后文字仍英文（M6 的 i18n 未完全生效？设置里语言切换后
  未重渲染/未持久化/缺 zh 文案——待查 ShellApp 语言切换与 t() 实现）。
- **发布**：draft v0.1.0 已就绪（17 assets + 中文 notes）；externalBin 打包 daemon
  改动已提交（0e25ac8）但**本地重建安装包验证被打断**（下一步：tauri build --bundles
  nsis 验证安装包含 daemon → 更新 draft → publish）。
- **repo public**：用户操作中（public 后 attestation 可启用；release workflow 已留
  checksums 替代注释）。
---

## 2026-09-02 更新：根因实证 + 修复（BLOCK-M8E-BOOTSTRAP-STUCK）

### 根因（实证，非推测）

**bootstrap credential 死锁**：daemon 只在连接断开（serve_connection 退出）时 reissue
credential 文件；Shell 只在 credential 有效时才能建立连接。当 daemon 存活且长时间无连接
（token 在 registry 与文件里同时过期——lease 3600s），Shell 启动读文件 → connect →
`stale` 拒绝 → 2s 重试循环**永远失败**（没有任何路径触发 reissue）→ GUI 永久停在 bootstrap。

现场证据（本机 2026-09-02）：daemon PID 31348 存活（14:04 启动，14:04:56 曾 reissue），
credential 文件 token 于 07:04:56Z 过期；裸 socket 诊断握手回复
`{"accepted":false,"reason":"stale"}`——文件 token 已过期且 daemon 无刷新路径。

「偶发」的解释：daemon 刚启动/刚 reissue 时 token 新鲜 → Shell 连接成功（QA 场景秒级完成
永远新鲜）；daemon 存活超过 lease 且无连接 → 卡。QA 25/25 稳定过正是因为它每次 spawn
新 daemon 立即连接，测不到「长时间空闲」路径。

「手动裸 socket 两次一次卡」同样解释：同一 token 一次性（AC-IPC-001），第二次连接
replay/stale 被拒（QA A6 正是断言这一点）。

排除项：spawn 子进程 stdout 未 drain 导致 pipe 阻塞（假设 E）——supervisor.rs
spawn_output_reader 双线程读到 EOF，与生态参考实现（AleCyriaco backend.rs）同级，无此问题。

### 修复

daemon 周期维护 bootstrap credential 文件（`DaemonServer::maintain_bootstrap_credential`，
serve loop 每 1s 检查）：
- 文件 token 剩余寿命 < 60s（BOOTSTRAP_REFRESH_LEAD）→ 立即 reissue 新 token（全 lease）；
- 文件缺失/不可读 → 立即补发；
- token 新鲜时零操作。

效果：**「daemon 存活 + 文件 token 过期」在修复后不可能发生**；Shell 重试循环最坏在
下一维护周期内拿到新 token，无需 Shell 侧改动。

验证：
- 单测 ×2（crates/daemon/tests/credential_reissue.rs）：短 TTL 触发刷新 + 文件缺失恢复，
  均通过；daemon 全量测试、fmt、clippy 全绿。
- 待办：GUI 实机连续启动验证（发布门验收时执行）；修复前已存在的旧 daemon 需重启一次
  （新版 daemon 自带维护）。

### 遗留

- mid-session credential 消费后的断线重连仍是 daemon 侧 TODO(M6-C)（本次修复覆盖启动路径，
  不覆盖「连接中断后同文件 token 已消费」的即时重连窗口——现有断开 reissue 已处理该场景）。
### 第二层根因（2026-09-02 补充，GUI 实机确认）

credential 修复后 daemon 协商稳定 connected，**但 GUI 仍停在 bootstrap**——第二层问题在前端：

ShellApp mount 时只执行一次 `load()`（getShellSnapshot + getEnvironmentCatalog + validate）。
daemon connector 由后台线程安装（失败 2s 重试），mount 时的 getShellSnapshot 在 connector
就绪前调用必然失败 → catch 只 setSnapshotError → **snapshot 永远 null、无重试** →
HarnessSurface（!snapshot 分支）永久渲染 "Reading canonical runtime state…"。

注意 QA B6（Shell restart reconnect）只断言 stderr 出现 `[daemon-client] connected`，
**从未验证 GUI 离开 bootstrap**——因此 25/25 全绿与 GUI 卡点并存。

修复（ShellApp.tsx）：load 失败后 2s 自动重试（与 daemon 连接重试同节奏），成功即停；
组件卸载清理定时器。回归测试：getShellSnapshot 先拒后成 → UI 离开 bootstrap（vitest 通过）。

