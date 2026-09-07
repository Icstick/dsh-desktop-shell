# PLAN-ENV-QUICK-EDIT: 环境卡片化 + 分区编辑 + 移除（隔离测试前置）

> 2026-09-03 定稿（用户拍板 B 方案）。分支 `feat/env-quick-edit`（main dd47129 开出）。
> 目标：①配置好的环境以 configure 卡片呈现，点环境≠重配；②编辑不强制向导顺序——每一环节可独立修改；③支持移除（含 active/运行中提示）；④为 managed 隔离测试（独立 dshHome）铺路。

## 背景与问题（用户反馈 2026-09-03）

- 设置页顶部 SetupWizard **常驻且自动预填当前激活环境**：打开设置页即处于「编辑当前环境」第 1 步，误保存会覆盖现有配置（已发生 rev17 事故：attached 3080 配置被覆盖）。
- 编辑体验被 6 步向导强顺序约束：改 profile/dshHome 等任意环节都要从 step 1 逐级 next，回到 step 1 观感是「全部重来」。
- 隔离测试（第二阶段）需要给 managed 环境改 dshHome 为独立目录——目前唯一途径是走向导，且容易误触。

## 设计决策

- **D1 设置页布局**：SetupWizard 不再常驻。设置页 = 环境卡片区 +「＋ 添加环境」按钮（点按钮打开**空态向导**走 6 步创建新环境）。打开设置页永远不预填编辑态。
- **D2 configure 卡片**（EnvironmentList 增强）：每卡片显示 label（用户可改）+ id（只读唯一键）+ ownership/profile/端点 + active 标记；操作：激活（stop→activate→start 编排不变）、编辑、移除。**点卡片本身不进入任何编辑**。
- **D3 分区编辑表单**（EnvironmentEditForm，替代「向导编辑模式」）：编辑对话框按环节分区平铺，无步骤顺序，每区可独立修改：
  - 名称区：label（可改）；id 只读展示
  - 来源区：harness.path（repository/executable 路径，目录选择；cwd 只读）
  - 数据区：dshHome（目录选择）、profile（输入 + 重新扫描）
  - 端点区：endpoint.port（固定端口或 auto）
  - 高级区：nodePath（仅 managed+repository 显示可改）；policy 只读展示
  - ownership（managed/attached 模式）**v1 不可改**——改模式=移除重建（避免 rev17 式覆盖错乱）；UI 注释说明
  - 底部：后端校验结果 + 保存（沿用 save_environment upsert-by-id 语义；保存后若编辑的是 active 环境则刷新 validatedEnvironment/catalog/snapshot）
- **D4 移除（remove_environment）**：
  - 后端新命令 `remove_environment(app, environmentId)`：环境必须存在于 catalog；移除 + 若被删者是 active → activeEnvironmentId 置 null + revision bump + 原子写 + bak（沿用 store 语义）；返回更新后 catalog
  - UI 确认对话框（卡片内联确认，仿 confirmingManagedStop）：
    - active 环境：提示「这是当前激活的环境」
    - managed 且运行中（generation≥1 healthy）：提示「运行中的 DSH 会先被停止」
    - 确认后流程 = 若运行中先 stop → remove → ShellApp 回空态（validatedEnvironment=null、catalog active=null）
  - active 删除后 ShellApp 状态：snapshot 环境置空、surface 显示「选择环境」空态（现有 empty-state 路径）
- **D5 向导保留新建专用**：SetupWizard 仅「＋ 添加环境」入口（initialEnvironment=null）；编辑不再复用向导。相关文案区分「创建环境」/「编辑环境」。

## 文件清单

- `apps/desktop/src-tauri/src/commands.rs`：remove_environment 命令 + CommandError 分支（NotInCatalog→unavailable 系）
- `apps/desktop/src-tauri/src/environment_store.rs`：remove_environment(&path, id)（原子写语义 + 测试）
- `apps/desktop/src-tauri/src/lib.rs`：invoke_handler 注册
- `apps/desktop/src/desktop-api.ts`：removeEnvironment(environmentId) + 类型
- `apps/desktop/features/environment-settings/src/EnvironmentEditForm.tsx`（新）：分区编辑表单（D3）
- `apps/desktop/features/environment-settings/src/EnvironmentList.tsx`：卡片操作（激活/编辑/移除 + 内联确认）
- `apps/desktop/features/environment-settings/src/SetupWizard.tsx`：仅新建模式（空态）；初始环境语义移除
- `apps/desktop/features/shell-ui/src/ShellApp.tsx`：设置区重构（wizardOpen 状态、编辑对话框状态、删除编排）
- `apps/desktop/src/i18n.tsx`：zh/en 文案（卡片操作、确认对话框、表单分区标签）
- 样式：shell.css / environment-settings 样式
- 测试：EnvironmentList.test、ShellApp.test（向导常驻断言改触发式）、environment_store remove 测试

## 提交序列

1. docs: 本计划
2. feat(backend): remove_environment（store + command + tests）
3. feat(ui): 设置页卡片化 + 向导触发式（布局/文案/测试）
4. feat(ui): EnvironmentEditForm 分区编辑 + 移除确认流
5. 门禁 + 实机验证（编辑 dev-repo dshHome → 独立目录 → managed 隔离启动验证）

## 验收

- 打开设置页：只见卡片与「＋ 添加环境」，无预填向导
- 点卡片：只切换激活，不进入编辑
- 编辑：分区表单任意区独立改（label/dshHome/profile/port），保存即生效；active 环境编辑后 UI 状态刷新
- 移除：非 active 直接确认删；active 有「当前激活」提示；运行中 managed 有「会先停止」提示；删 active 后回空态
- 隔离测试：dev-repo dshHome 指向 `D:\DSH_workspace\.dsh-isolated` → 启动 3081 → 全新会话，与 3080 无共享

## 范围外

- environments.yaml / 启动自动恢复（PLAN-POST-WIZARD 阶段 2.2/2.3）
- 模式转换（managed↔attached 编辑）、policy 编辑、删除时 daemon 残留进程树兜底（daemon 重启清空，可接受）
- 卡片状态灯/启停按钮（在 Runtime 面板职责内）
