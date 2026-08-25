# Research Synthesis

## 共识

三份报告与关联会话共同支持：

- 用户自有 DSH/DSH_HOME。
- Desktop 是 Shell + Native Capability Host + Supervisor。
- Managed/Attached 显式分权。
- 不 fork/patch upstream UI。
- Capability Broker 小而稳定。
- dsh-std compatible、not required。
- Plugin Market、Scheduler、Usage 语义继续属于 DSH。
- Persistent PTY、Shared Browser、Notification、Usage 是高价值后续能力。
- clean-room 与来源审计必须从第一天存在。

## 分歧

一份报告推荐 Electron/TypeScript 作为 MVP，以降低 Browser/PTY 首版成本；另外两份和工程实施报告推荐 Tauri 2 + React/TypeScript + Rust，以强化 Supervisor、process ownership 与长期安全边界。

## 决议

采用 Tauri 2 + React/TypeScript + Rust；Browser automation 通过独立 provider/CDP，避免让 system WebView 技术限制污染 Capability Contract。Electron 保留为已评估替代方案，不在 M0 双轨实现。见 ADR-0003。

## 证据政策

研究报告中的第三方数字、状态和许可证描述不会自动成为当前事实。规范只引用已在 2026-08-25 重新核验的官方 DSH、dsh-std、Tauri 与 Apache 来源；精确 revision、发行坐标和影响分析见 [External Baseline](EXTERNAL_BASELINE.md)。
