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