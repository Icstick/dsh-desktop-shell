# PLAN-POST-WIZARD: 盲审修复 + 环境配置持久化（v0.1.0 路径）

> 2026-09-02 定稿；2026-09-03 更新：阶段 0 完成（main 23c5027）；1.1/1.2/1.4/1.7 完成（feat/ux-polish）；1.3 已满足；1.5/1.6 待人工验证；2.1 设计稿就绪待拍板。跟踪：work-continuity checkpoint（user-global 桶）；执行分批，可与云端 workbox 委托并行。
> 前置状态：feat/wizard-repo-source 分支 9 commits（Managed repository 已实机 healthy、Attached 可达）；
> 盲审（零背景子代理，OCR+色采样取证）输出 P1-P7 问题与 8 项待人工验证。

## 决策记录

- D1（用户 2026-09-02）：wizard 分支先不合并，定稿计划后分批推进（含收尾合并）。
- D2（用户 2026-09-02）：**环境配置 = 可读格式化配置文件**——配置好的环境固定保存到格式化文件，用户/agent 可直接编辑文件完成环境配置（文件是一等配置入口）。
- D3（用户 2026-09-02）：**二次启动自动恢复连接**——再次启动保持已连接状态，不要求每次重新配置。
- D4（用户 2026-09-02）：计划用 work-continuity checkpoint 跟踪；独立可验证工作项可委托云端 workbox。
- D5：盲审优先级：P1/P2（文案与页面身份）> P3（3989 疑点调查）> P4/P5（状态色/布局）> 待人工验证项。

## 阶段 0：分支收尾（小）

- 0.1 squash 合并 feat/wizard-repo-source → main（9 commits：WI-B/WI-A/两轮 review/audit 修复/执行层 recipe/auto-port 防呆），删除分支
- 0.2 本计划文档 + checkpoint 落档

## 阶段 1：盲审修复（新分支 feat/ux-polish，独立小步提交）

- **1.1 [调查] P3 exactOrigin 端口 3989 来源**：DSH Surface 策略页显示的 exactOrigin（http://127.0.0.1:3989）与环境的 3080 不一致——追 derive_policy/surface URL 源链（daemon? surface binding?），确认是 bug 还是设计（多实例时 origin 语义）；修复或加 UI 说明。
- **1.2 [文案] P1/P7 术语人话化**：HarnessSurface/RuntimePanel 的策略与说明句重写（Attached=只读连接、Managed=壳托管启动/停止、代次≈实例代号，首次出现给白话解释）；定术语策略（保留词清单）；zh/en 同步。
- **1.3 [页面身份] P2**：各页 H1 用页面名（DSH Surface / 环境：dev DSH / 设置…）；eyebrow 保留品牌但不再当主标题；导航 icon tooltip 核对补齐。
- **1.4 [状态语言] P4**：健康/成功统一绿色视觉；「已验证」类徽章加说明（验证了什么）；badge 文案本地化评估。
- **1.5 [布局] P5**：内容区合理最大列宽居中（消除右半大留白 + 底部全宽行归入页内）。
- **1.6 [人工验证项]**：导航图标形态与 tooltip、右上角徽章位置、按钮存在性、像素美感——用户实机确认或视觉后端恢复后截图复核（原 8 项清单在盲审报告）。

## 阶段 2：环境配置持久化（核心新功能；设计先行，独立工作项）

- **2.1 设计（ADR-0022）**：
  - 目标：①配置固定、可读、可编辑（用户与 agent 直接编辑文件 = 配置环境）②二次启动自动恢复已连接状态
  - 候选形态 A：catalog JSON 保留为内部真源 + 新增 human 友好配置文件（environments.yaml，注释/示例），保存=双写，启动=读 yaml → 校验 → catalog；外部编辑 → 检测/手动重载
  - 候选形态 B：catalog 直接迁移为 YAML 单真源（破坏既有读写路径，schema 改动大）
  - 评估项：位置（app 数据目录 / DSH_HOME?）、格式（YAML vs JSONC）、agent 编辑友好（schema 文档 + 校验错误可读）、与 daemon catalog 读侧兼容（daemon 读同一文件）
  - 倾向：A（内部真源不变 + 用户侧 YAML 投影/双写，风险低）——2.1 定稿前交叉讨论
- **2.2 实现（依 2.1 结论）**：文件读写桥 + 校验 + UI 保存写文件 + 外部编辑生效机制（watch 或显式重载按钮）
- **2.3 启动自动恢复**：shell 启动 → active env：attached → 自动 probe 显示连接态；managed → 上次 healthy 或 autoRestartOnCrash 策略 → 自动 start（新代次）→ 直接进入已连接/运行视图；失败态清晰（不吞错）
- **2.4 agent 编辑体验**：配置文件格式文档（示例文件 + schema 说明），错误编辑给出可读错误（行/字段），agent 改完 shell 重载即生效

## 阶段 3：云端委托评估（workbox）

- 适合云端（独立、自包含、可验证）：1.1 调查（代码只读分析出报告）、1.2 文案初稿（给定术语策略产出 zh/en 文本）、2.1 设计对比稿（A/B 论证 + 社区惯例调研）
- 保留本地（需本机验证/长迭代）：1.3-1.5 UI 改动、2.2-2.4 实现与验证、0.1 合并
- 每批任务卡走 prv-dsh-workbox inbox（自包含 prompt + 明确输出契约）

## 未决问题

- P3 的 3989 端口来源（1.1 调查目标）
- 盲审 8 项待人工验证（1.6）
- 阶段 2 形态 A/B 定稿（2.1 讨论目标；涉及用户拍板）
- zh 措辞润色、EnvironmentList 无样式（既有遗留，随 1.x 顺带或独立）
- 本会话无法直接调 work_state 工具——checkpoint 经旁路写库（保持 /checkpoint show 可读）

## 产物索引

- 分支：feat/wizard-repo-source（9 commits，含执行层 recipe 与 UI 防呆修复）
- 设计：docs/roadmap/PLAN-WIZARD-REPO-SOURCE.md、docs/decisions/ADR-0020-managed-repository-source.md
- 盲审报告：子代理 715c3cbc（OCR 取证，P1-P7 + 8 待验证）
- 跟踪：work-continuity /checkpoint show（user-global）
