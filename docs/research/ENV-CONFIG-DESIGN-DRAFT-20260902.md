以下为决策稿草案（事实/建议已显式区分；最终由项目方拍板）。

# ADR-0022 草案：环境配置"配置文件即一等配置入口" + 启动自动恢复 —— 形态 A/B 决策稿

## 结论与推荐（建议）

1. **推荐形态 A**：`environment-catalog-v1.json` 保持唯一持久真源（daemon 读侧、revision/备份、fail-closed 语义全部零改动），另增用户/agent 可读可编辑的 **`environments.yaml`** 作为"声明式编辑入口"；保存=双写，外部编辑经**显式重载（v1）+ 启动时自动 reconcile（+ 后续可选 watch）**生效。
2. 位置建议：与 catalog 同目录 `%APPDATA%/dev.dsh.desktop-shell/environments.yaml`，GUI 提供"在编辑器中打开 / 从文件重载 / 导出示例"三按钮，Settings 页展示完整路径——路径固定、可发现，是 agent 编辑体验的关键。
3. 格式细节：YAML 采用**严格子集**（拒绝未知键，镜像现有 `deny_unknown_fields`），环境对象字段与 serde 模型一一对应（camelCase），语义校验**复用现有 `validate_environment_value`**，错误输出带 `文件:行:列` + `{environmentId, field, code, message}`，与 GUI 现有 issue 面板同构。
4. 形态 B（YAML 唯一真源）**否决**：破坏双侧读路径与全部 fail-closed 测试、引入一次性迁移与回滚复杂度，收益（少一份同步）不足以覆盖成本——见对比表。
5. 与"启动自动恢复"是**同一设计的两个机制**：YAML=声明意图（active 环境 + 每个环境的策略），启动恢复=运行时意图应用（Attached→probe、Managed→幂等 `runtime.start`），两者共用"active 环境"作为锚点，应同入 ADR-0022 定稿（与 PLAN-POST-WIZARD 2.1–2.3 的分工一致）。

## 现状约束摘要（事实，来自 main 分支代码）

- **F1｜单一真源与形状**：`%APPDATA%/dev.dsh.desktop-shell/environment-catalog-v1.json`（desktop `commands.rs::catalog_path` 与 daemon `credential::data_dir()` 同一目录）；serde camelCase + `deny_unknown_fields`，顶层 `{schemaVersion:1, revision, activeEnvironmentId?, environments[]}`，每环境 `{schemaVersion, id, label, harness{mode:repository|executable|command, path, cwd?, args[]}, dshHome, profile, nodePath?, endpoint{host, port:"auto"|1024..65535}, ownership:managed|attached, policy{autoRestartOnCrash?, allowNativeAdapter?}}`（`environment_store.rs` / `environment.rs` / `commands.rs` 三处同构）。
- **F2｜写读分权与写安全**：Shell 是**唯一 writer**（`save_environment`/`set_active_environment`，每次 bump revision、排序、校验后经 `next`+`bak` sidecar、fsync、0o600/0o700、失败回滚的原子写）；daemon 是**只读**，每次 `runtime.*` 调用现读现解析该文件（"per invocation"，改完即时可见、无需重启）；**损坏文件 fail-closed 拒绝，绝不静默覆盖**（有测试断言 `corrupt_catalog_is_not_silently_overwritten`）。
- **F3｜双层校验**：语义校验在 Shell（`validate_environment_value` 产出逐字段 `{field, code, message}`）与 managed-runtime（`is_valid()` 镜像：id 模式、label 长度、loopback-only、保留参数 `--host/--port/--no-open/--trusted-host`、`nodePath` 仅 Managed+repository 且绝对路径等）各一份；而 **catalog 级错误是二值的 `Corrupt`，无字段/行列信息**。
- **F4｜启动与进程存活现状**：Shell `.setup` 只后台连接 daemon（probe→spawn→credential），**不读 catalog、不自动恢复连接**；但 daemon 持有 Managed 进程树且**跨 Shell 重启存活**（ADR-0008/0019），`runtime.start` **幂等**（同环境已在运行则返回当前 report 而非二次 spawn）；`policy.autoRestartOnCrash` 仅是运行期崩溃恢复策略（有界预算，ADR-0013），**不负责"启动时拉起"**。
- **F5｜依赖与生效路径现状**：现有 Cargo 依赖**无 YAML 解析器、无文件系统 watcher crate**（仓库惯用 `=` 精确锁版）；外部编辑目前只能靠重启应用生效——需要新增依赖或显式重载机制。

## A/B 对比表 + 业界惯例小结

### A/B 对比（维度均为建议性评估）

| 维度 | A：JSON catalog 真源 + YAML 用户入口（双写） | B：YAML 唯一真源（破坏性迁移） |
|---|---|---|
| 用户/agent 编辑友好度 | ★★★★★：YAML+注释+示例；可整文件替换式编辑 | ★★★★★（同左；少一层"同步"心智） |
| 注释与示例支持 | 优（文件头注释 + 每环境注释模板；但 GUI 重导出会再生文件、手写注释不保留） | 同左，且注释语义就是真源的一部分 |
| 破坏性/迁移成本 | **低**：纯增量；无 on-disk 迁移 | **高**：双 crate 读侧、Store、diagnostics、全部 fixture/测试/UI 快照改写 + 一次性 JSON→YAML 迁移 + 回滚方案 |
| 与 daemon 读侧兼容 | **零改动**：daemon 继续读 JSON，reload 只重写 catalog | 破坏性：managed-runtime `load_catalog`/校验/错误映射全改（daemon 与 Shell 需同步发版一致） |
| 并发写入风险 | 有：双写分叉（GUI 保存覆盖外部编辑）→ 需"导入优先 + 冲突横幅"收敛 | 低（单文件），但多写者同文件竞争需文件锁/合并策略，未必更简单 |
| 错误可读性 | 好：解析错误带行列；语义错误可复用现有 `{field,code,message}` 体系 | 同左，但需要新写一套 YAML 语义错误通道（原 JSON 通道被删） |
| revision/backup 语义 | 保留在 catalog，YAML 携带只读 `syncedFromCatalogRevision` 头 | 需重建（revision、sidecar、回滚全搬到 YAML 格式） |
| 总结 | **推荐**：把新风险圈在 Shell 新增的桥内，守住既有 fail-closed 与 daemon 兼容 | 除非未来出现多消费者直读 YAML 的强需求，否则成本收益不成立 |

### 业界惯例小结（每条一行结论 + 来源）

- **VS Code / devcontainer.json**：仓库内 JSONC（注释+尾逗号）声明文件本身就是一等配置入口，配 JSON Schema 校验与示例，官方生态明确支持人/agent 直接编辑——"配置即入口"最正统的样板（[devcontainers overview](https://raw.githubusercontent.com/devcontainers/devcontainers.github.io/gh-pages/overview.md)、[GitHub Codespaces devcontainer 文档](https://docs.github.com/ko/codespaces/setting-up-your-project-for-codespaces/configuring-dev-containers/adding-features-to-a-devcontainer-file)、[spec DeepWiki](https://deepwiki.com/devcontainers/spec/2-dev-container-json-specification)）。
- **Multipass**：声明内容走 CLI + 每次 launch 时的 cloud-init YAML，本地常驻"配置文件"不是配置真源——**反面样本**：文件不完整、难以作为 agent 编辑入口（[Multipass cloud-init 文档](https://canonical.com/multipass/docs/latest/how-to-guides/manage-instances/launch-customized-instances-with-multipass-and-cloud-init/)、[Using cloud-init with Multipass](https://canonical.com/blog/using-cloud-init-with-multipass)）。
- **WSL `.wslconfig`**：固定用户路径（`%USERPROFILE%\.wslconfig`）的小型本地配置文件、允许注释、**每次启动时读取（无 watch）**——与"固定路径 + 启动读 + 显式生效"的桌面工具模式最接近的先例（[WSL 高级设置配置](https://learn.microsoft.com/zh-cn/windows/wsl/wsl-config)、[Ubuntu WSL 实例配置参考](https://ubuntu.com/wsl/docs/stable/reference/instance_configuration)）。
- **docker-compose.yml**：声明式 YAML 单文件真源，编辑后 `up`/`restart` reconcile（幂等重放），schema/规范文档成熟——"**文件即 API、命令 reconcile**"与"让 agent 改完文件即生效"的循环最契合（[Compose file reference](https://docs.docker.com/compose/compose-file/)、[compose-spec](https://github.com/compose-spec/compose-spec)）。
- **结论（建议）**：与"agent 直接编辑配置文件"最契合的是 **docker-compose 的声明式文件 + reconcile** 与 **devcontainer 的 schema 校验 + 示例化注释** 组合；`environments.yaml` 本质是该模式在本地工具上的变体（`.wslconfig` 证明了 Windows 桌面用户接受"固定路径、启动时读取"的小配置文件）。

## 推荐设计的文件格式示例（建议；含注释的真实示例片段）

```yaml
# DSH Desktop Shell — 环境配置（用户/agent 一等编辑入口）
# 文件位置: %APPDATA%/dev.dsh.desktop-shell/environments.yaml
# 机制: 保存 = 双写（catalog JSON 为内部真源 + 本文件投影）；
#       外部编辑 = 启动时自动导入 + 「设置 → 从文件重载」立即生效。
# 注意: GUI 保存会重新生成本文件，手写注释在重导出时不保留（注释在导入时被消费）；
#       完整字段与取值规则见 docs/environments-file.md（含每个字段的取值约束）。
# 内部管理字段（勿手改）: 上次同步的 catalog 修订号。
syncedFromCatalogRevision: 7

# 默认环境: 启动恢复连接的目标（必须存在于下方列表，对应 catalog activeEnvironmentId）。
active: dev-web

environments:
  # ── Managed：桌面 Shell 托管启动/停止/恢复（拥有进程树）──
  - id: dev-web                       # ^[a-z][a-z0-9-]{2,64}$
    label: Dev Web (repo)             # 1–128 字符
    ownership: managed                # managed | attached（attached 只读，禁 stop/restart）
    harness:
      mode: repository                # repository | executable | command
      path: D:/repos/deepseek-harness/apps/web/dist/main.js   # 必填
      cwd: D:/repos/deepseek-harness
      args: []                        # ≤64 项；禁止 Supervisor 保留参数 --host/--port/--no-open/--trusted-host
    dshHome: C:/Users/alice/.dsh      # 必填
    profile: default                  # 非 default 的 Managed 会附加 --profile <name>
    nodePath: C:/Program Files/nodejs/node.exe   # 仅 managed+repository 允许，须绝对路径
    endpoint:
      host: 127.0.0.1                 # 仅允许 loopback
      port: auto                      # auto | 1024..65535
    policy:
      autoRestartOnCrash: true        # 运行期崩溃有界自动重启（ADR-0013；不负责“启动时拉起”）
      allowNativeAdapter: false

  # ── Attached：连接既有 DSH（只读；启动时自动 probe 显示连接态）──
  - id: lab-runner
    label: Lab Runner (attached)
    ownership: attached
    harness:
      mode: executable
      path: D:/tools/dsh/dsh.exe
    dshHome: D:/lab/.dsh
    profile: default
    endpoint:
      host: 127.0.0.1
      port: 4317
```

关键格式决策（建议）：① **严格子集**——未知键整文件拒绝并报 `line:col`（与 `deny_unknown_fields` 同精神，防拼写错误静默吞掉）；② `schemaVersion`/`revision` 不暴露给用户文件，由桥补 `1` 与维护 `syncedFromCatalogRevision` 头；③ `active` 省略时保持 catalog 现 active 不变；④ YAML 解析器选**有维护的 fork**（`serde_yaml` 已停止维护，见 [rustsec advisory](https://github.com/rustsec/advisory-db/issues/2132)；评估 `serde_yaml_ng`/`serde_yml`，且该仓库惯用 `=` 锁版需走依赖决策），语法错误须带行列、语义错误复用 `validate_environment_value` 的 `{field,code,message}` 并加 `environmentId` 前缀；⑤ 相比 JSONC：Rust 栈无一流 JSONC 解析器（需注释剥离预处理、损失行列映射），YAML 原生带行列错误——这是选 YAML 而非 JSONC 的工程理由。

**与"启动自动恢复"的关系（同一设计的两个机制）**：YAML 是"声明意图"（`active` + 各环境 policy），启动恢复是"应用意图"——Shell 连上 daemon 后读 catalog active 环境：Attached → `probe_attached_environment` 显示连接态/失败态；Managed → 调幂等 `runtime.start`（若上一 Shell 会话的 generation 仍由 daemon 持有，直接返回其 report，天然"恢复已连接状态"，不会双 spawn）；失败一律清晰呈现不吞错。v1 恢复策略从 `active + ownership + autoRestartOnCrash + 上次状态`推导（对齐 PLAN-POST-WIZARD 2.3），不新增字段；如后续要 `launchOnStartup` 级细粒度，属 catalog schema v2 候选（单独 ADR）。

## 实施拆分建议（工作项级）+ 风险/开放问题

**工作项（建议顺序，均含测试/文档/evidence；语言均为事实性约束下的实现建议）**
- **WI-1（2.2a）YAML 桥模块**：YAML↔catalog 环境的双向映射 + 严格子集校验 + 错误契约（`{file,line,col}` / 语义 issues）；round-trip 与错误路径 fixture 测试；依赖引入决策（fork 选型 + `=` 锁版）。
- **WI-2（2.2b）命令与 UI**：`export_environments_file` / `reload_environments_from_file` tauri 命令 + `CommandError` 扩展；UI 增"打开配置文件/从文件重载/导出示例"与错误面板（复用现有 issue 渲染）+ zh/en 文案。
- **WI-3（2.3）启动自动恢复**：boot 序列（daemon 就绪→解析 active→Attached probe / Managed 幂等 start→状态入 snapshot）；失败态不吞错；用现有 fake daemon / attached probe 测试基建覆盖。
- **WI-4（2.4）文档与示例**：`environments-file.md`（字段取值约束逐条列）+ 首次运行生成带注释示例文件；同步 PLAN/ADR 落档。
- **WI-5（可选/后续）watch 机制**：`notify` crate（[docs.rs/notify](https://docs.rs/crate/notify/4.0.6/source/README.md)，v8 仍在维护）debounce 重载 + "检测到外部修改"横幅；v1 不做，保持显式重载。

**风险（建议 + 缓解）**
- 双写分叉：GUI 保存可能覆盖"打开中未重载"的外部编辑 → 重载前若 YAML 的 `syncedFromCatalogRevision`/mtime 更新则提示"先导入外部更改"，并文档化 last-writer-wins。
- 手写注释丢失（GUI 重导出再生文件）→ 头部模板固定、文档明示"注释被消费不持久"；agent 侧用示例文件而非注释传递语义。
- 新 YAML 依赖的供应链/维护风险 → 严格子集（禁 tags/anchors）、`=` 锁版、入库前安全评审。
- 信任面：文件可指向任意 `harness.path`（与向导同级信任，本机用户配置无新增威胁）；导入仍走既有校验（loopback/reserved args/nodePath），不放开任何规则。
- 双 schema 漂移（现有 Shell/daemon 两套校验）不因本设计扩大：导入只写 catalog 一条路径，daemon 读侧不变。

**开放问题（需项目方拍板）**
1. 文件位置：catalog 同目录（推荐，可发现）vs `DSH_HOME` vs 用户配置目录；是否预留环境变量覆盖。
2. `active`/`launchOnStartup` 细粒度恢复策略是否值得进 catalog schema v2（新增字段需双侧同步，独立小 ADR）。
3. v1 是否必须含 watch（决定 WI-5 是否提前）；显式重载 + 启动 reconcile 是否可接受为 MVP 语义。
4. 是否需要发布机器可读 JSON Schema（editor/agent 补全与校验，devcontainer 式），文档站点承载位置。
5. YAML 严格子集对未知键的拒绝策略：整文件拒绝 vs 逐环境跳过——我建议整文件拒绝（fail-closed，与现语义一致），但若用户高频手写文件可复议。

**事实/建议边界**：F1–F5 与"文件格式示例中字段约束"均为读取 main 分支代码所得事实；其余（推荐形态、对比评分、业界结论、工作项、风险缓解）为顾问建议，非项目承诺。
