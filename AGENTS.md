# Agent Operating Contract

本文件适用于本仓库全部 Agent、自动化和人类贡献者。子目录 `AGENTS.md` 只能收紧规则，不能放宽根级安全与架构不变量。

## 启动协议

1. 读取 `START_HERE.md`、`tracking/project.yaml`、`tracking/CURRENT.md`。
2. 读取当前 milestone、目标模块及相关 ADR。
3. 只认领一个主 `WI-*`；记录 session、branch/worktree 和 24 小时 advisory lease。
4. 验证依赖已满足，确认 `implementation_authorized`。

## 当前 M1 实现门禁

- `implementation_authorized: true` 只解除 M0 的全局禁码门禁，不豁免工作项认领、module boundary、ADR、security review 与 evidence 要求。
- 新增 `.rs`、`.ts`、`.tsx`、构建清单、锁文件或 workflow 必须属于已认领的 M1 工作项，并在独立 branch/worktree 中提交。
- 依赖安装、代码生成、构建与测试只能作为已认领实现任务的可追溯步骤执行；不得在状态转换 session 中顺带运行。
- 继续执行 user-owned External Core 与 clean-room/no-copy 边界；Desktop 不安装、构建或打包 DSH Core。

## 架构不变量

- User-owned External Core；Desktop 不管理 DSH 发行。
- Managed/Attached 显式分权；Attached 默认拒绝 stop/restart/kill。
- DSH WebView 与 Browser WebView 无 privileged native bridge。
- 所有 Agent native action 先经过 DSH tool/policy，再进入 Capability Broker。
- Capability 独立版本化；DSH-specific type 不穿越 Adapter boundary。
- `dsh-std` 是 optional adapter，不是 core dependency。
- Terminal Surface/Automation 与 Browser Surface/Automation 必须分权。
- 状态真源在 `tracking/`，接口真源在 `specs/`，原因真源在 ADR。

## 变更协议

- Interface、Schema、状态机、transport、trust boundary 或 ownership 变化：先提 ADR 或更新现有 ADR。
- 公开协议变化：更新 Schema、changelog、compatibility fixture 计划和 migration note。
- 一个工作项一个主 owner；并行工作通过独立模块或独立工作项拆分。
- 不把聊天、模型记忆或未提交实现当作项目真源。

## 证据要求

完成声明必须链接可复查证据：测试输出、Schema 校验、文档链接、review 记录或发布 artifact。未验证时使用 `review`，不得使用 `verified` 或 `done`。

**平台矩阵的本地验证（2026-09-13 教训）**：在 Windows-only 开发机上改动 `cfg(unix)` 分支后，
`cargo clippy` 只看 host——cfg(unix) 里的 dead_code / unused import / unused_mut 会让 CI 的
ubuntu/macos 矩阵红而本地全绿。提交前做交叉检查：

```
rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin
cargo clippy -p <crate> --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
cargo clippy -p <crate> --target aarch64-apple-darwin --all-targets -- -D warnings
```

Windows-only 的测试文件加文件级 `#![cfg(windows)]`。依赖系统库的 crate（tauri/webkit2gtk）
无法交叉编译，其 Unix 分支只能靠 CI 验证，改动时要额外小心。

**WSL 上的真实 Linux 运行（2026-09-13）**：交叉 clippy 只能证明 Unix 分支可编译，证明不了行为。
WSL（Ubuntu）里装一份 rustc 后可直接跑纯 Rust crate 的测试——这是本机唯一能真跑 Unix 行为
（UDS、SO_PEERCRED、/proc）的通道；依赖 tauri/webkit2gtk 的 crate 仍然只能靠 CI。
提交前至少跑一遍 `dsh-local-transport` 与 `dsh-daemon` 的 Linux 测试。

**两个跨平台陷阱（2026-09-13 实测踩到）**：

- `std::fs::canonicalize` 在 Windows 返回 `\\?\C:\...`（verbatim）形式，而内核 API
  （`QueryFullProcessImageNameW`）从不这样报。跨进程路径比对（peer identity）**不要** canonicalize，
  否则对真进程永远不匹配——而且「冒充者负向测试仍然通过」会掩盖它，只有正向腿会红。
- 库函数不要对**已存在**的目录调用 `set_permissions`：在 `/tmp` 上直接 EPERM，以 root 跑会真的
  改坏共享目录。只对自己创建的目录收紧权限。

**WSL mirrored 网络会占住宿主端口（2026-09-13 实测）**：本机 `.wslconfig` 为 `networkingMode=mirrored`，
WSL 运行时 Linux 临时端口段（32768–60999）在宿主侧被保留——Windows 进程绑定该区间的端口会直接
WSAEADDRINUSE，而 `netstat` 看不到任何监听者（desktop 测试的固定端口 39101 正是这样挂掉的）。
跑 Windows 门禁前先 `wsl --shutdown`；跑 WSL 的 Linux 测试时不要同时跑 Windows 测试套件。

## Handoff

Session 结束前：

1. 更新 `WI-*` 的状态、evidence、blocked_by、next_action。
2. 更新相关 `MOD-*` / `IF-*`。
3. 新建 `HANDOFF-*`，列出已完成、未完成、验证、风险、精确下一步。
4. 释放或延长 claim；过期 claim 的回收必须留下记录。
