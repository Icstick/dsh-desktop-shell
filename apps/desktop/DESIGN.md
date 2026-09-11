# DESIGN.md — dsh-desktop-shell 视觉规格

> 给**设计/UI agent** 读的文件：本项目应该*长什么样*。
> 与 `AGENTS.md`（编码 agent：项目*怎么构建*）、`README.md`（人：项目是什么）三者分工不重叠。
>
> **本文所有数值都从现有实现读出**（`apps/desktop/features/shell-ui/src/shell.css`，68KB / 537 条选择器），不是设计意图的转述。
> 生成新界面照此执行；若发现实现与本文冲突，**先改实现或先改本文**，不让两者长期分叉。

---

## 适用范围（先读这条）

- **适用**：外层 Shell 的自有界面——`shell-ui` / `environment-settings` / `runtime-diagnostics` / `terminal-ui` / `browser-ui` / `timer-ui` / `usage-ui`。
- **不适用**：承载的上游 DSH Web UI。按 **ADR-0004（Unmodified Upstream DSH Web UI）** 原样承载，**禁止** DOM injection、renderer fork、样式注入，或任何对上游 DOM / router / CSS 的 patch。
- 一句话：**这份规格约束的是我们自己的外壳，不是别人家的页面。**

## 视觉主张

一个**深蓝衬白的工程仪表盘**：冷色底 + 单一金色强调，密度高、字号小、边框细、留白省。像仪器面板，不像消费级产品页。
视觉重量由三样东西承担——**分层底色、1px 细线、一块大而软的投影**；不用鲜艳色块，不用多层阴影堆叠，不用大圆角卡片墙。

## 承载面配方（外壳的三件套，直接照抄）

```css
/* .surface-frame / .surface-state 的既有写法 */
border: 1px solid #ceddeb;
border-radius: 14px;
background: linear-gradient(180deg, #ffffff, #f5f8fc);
box-shadow: inset 0 1px rgb(255 255 255 / 3%), 0 18px 50px rgb(0 0 0 / 22%);
overflow: hidden;
```

要点：① 面板**有**投影，且是单片大范围软影（50px 模糊 / 22% 黑）——不要改成小阴影或用描边替代；
② 渐变极浅（white → paper，几乎看不出），作用是让面板边缘立起来，不是装饰；
③ 圆角固定在 14px 档，见下节三档规则。

## 颜色

### 已 token 化的调色板（CSS 变量，全部 `--marine-` 前缀）

| 变量 | 值 | 用途 |
|---|---|---|
| `--marine-ink` | `#233a63` | 主文字 / 深底 / 描边主体。**全场最高频色（91 次）** |
| `--marine-blue` | `#467caf` | 主交互色（主按钮、激活态、链接） |
| `--marine-gold` | `#b89759` | **唯一强调色**：重点标记、当前项。一处一块，不要刷屏 |
| `--marine-mist` | `#e8f0f9` | 次级底 / hover 底 |
| `--marine-paper` | `#f5f8fc` | 页面底色 |
| `--marine-white` | `#ffffff` | 面板底 |

### 事实上已成体系、但尚未 token 化的色（下次一并提成变量）

| 值 | 出现 | 语义 |
|---|---|---|
| `#ceddeb` | 85 | **分隔线 / 细边框专用**（强烈建议提为 `--marine-line`） |
| `#586d89` | 28 | 次要文字 / 说明（建议 `--marine-muted`） |
| `#216b52` / `#edf7f2` | 17 / 13 | 成功：文字 / 浅底 |
| `#a6324b` / `#e8bbc4` | 16 / 12 | 危险：文字 / 浅底 |
| `#805a16` | 12 | 警示：文字 |
| `#fff0f3` | 22 | 温和提示底（粉） |

**纪律**：不引入第七种主色；强调只能靠 `--marine-gold`。现状 115 个独立色值 / 583 次出现（另 38 处 `rgba()`），多数是上表的重复书写——**新增样式写变量名，不要再写十六进制**。

## 状态视觉（本仓库契约的硬要求）

`apps/desktop/AGENTS.md` 要求 UI 明确 `unavailable` / `degraded` / `attached` 状态并保持可键盘操作。实现里已有对应词汇表，**别自造新状态名**：

- **权威机制是属性选择器 `[data-state=...]`**（39 处使用），不是类名。现有取值 20 个：
  `loading` `ready` `running` `starting` `stopping` `mounting` `attached` `detached` `healthy` `degraded` `stale` `unavailable` `unsupported` `crashed` `error` `closed` `hidden` `viewport`
- **局部状态用 `is-*` 类**（`.is-active` 10 处；`is-empty` / `is-busy` / `is-on` / `is-off` / `is-done` / `is-broken` / `is-missing` / `is-unverified` 等各 1–2 处）。历史遗留 `is-warning` 与 `is-warn` 两个同义类，**新代码只用 `is-warning`**
- **健康与徽标**：`.runtime-badge`（48 处，最常用的状态承载）、`.attached-health`、`.policy-badge`、`.candidate-status`
- **空态 / 异常态**：`.surface-state` 是标准件——`min-height: 590px` + grid 居中 + `48px` 内边距 + 文字居中，与 `.surface-frame` 共用上面的承载面配方

## 字体

```css
/* 界面正文 */
font-family: "Bahnschrift", "Microsoft YaHei UI", "PingFang SC", "Segoe UI", sans-serif;
/* 代码 / 运行时 / 终端类界面 */
font-family: ui-monospace, "SFMono-Regular", Consolas, monospace;
```

**字号实测分布——整体偏小，这是本项目最鲜明的特征**：

| 字号 | 出现 | 场景 |
|---|---|---|
| 9–10px | 20 | 角标 / 极次要注释 |
| **11px** | 29 | 次级说明、表格元数据 |
| **12px** | 44 | **正文默认** |
| 13px | 18 | 小标题、强调正文 |
| 14–16px | 13 | 区块标题 |
| 20–32px | 6 | 页面级大标题（罕见，仅首屏/向导） |

→ **默认 12px**。需要层级时优先用字重与颜色（ink → muted），不要靠放大字号。

## 圆角（三档就够）

| 值 | 出现 | 用途 |
|---|---|---|
| `999px` | 19 | **胶囊**：徽标、状态点、开关、筛选片 |
| `8–10px` | 38 | 按钮、输入框、小卡片（历史混用 8/9/10，**新代码统一 10px**） |
| `12–14px` | 21 | 面板 / 抽屉 / 承载面（**新代码统一 14px**） |
| `50%` | 2 | 圆点指示器 |

## 间距

`gap` 实测集中在 **6/7/8/9/10/12/16/18px**（2px 步进，主体 6–12px）；内边距以小为常态：`3px 7px`、`4px 10px`、`0 11px`、`13px 14px` 是高频形态。

→ **这是个紧凑界面**：默认 `gap: 8px`；分区之间 16–18px 已经算大留白。

## 命名与结构

- **BEM 变体**：`.block__element--modifier`，例 `.shell-header__description`、`.shell-header--compact`
- **状态类**用 `is-` 前缀；**状态值**用 `data-state`
- 每个 feature 自带前缀：`shell-*` / `environment-*` / `browser-*` / `terminal-*` / `runtime-*`
- 样式集中在 `shell-ui/src/shell.css`；新增 feature 前先确认能否复用既有 `.panel` / `.field` / `.eyebrow` / `.runtime-badge`

## 组件清单（可直接复用的既有件）

| 组件 | 类名 | 说明 |
|---|---|---|
| 应用骨架 | `.shell-app` / `.surface-frame` | 外壳与内容框 |
| 页头 | `.shell-header` / `--compact` | 紧凑态是常态 |
| 侧栏 | `.activity-rail` | 图标导航 |
| 面板 | `.panel` / `.surface-native` / `.surface-policy` | 三种承载面 |
| 空 / 异常态 | `.surface-state` | 见「状态视觉」 |
| 按钮 | `.primary-button` / `.secondary-button` | 主蓝 / 次白底描边 |
| 字段 | `.field` | 表单行 |
| 徽标 | `.runtime-badge` / `.eyebrow` / `.policy-badge` | 胶囊态小标签 |
| 列表 | `.environment-list` / `.notification-item` / `.ownership-card` | 行式列表 |
| 键值网格 | `.definition-grid` | 属性对 |
| 专用区 | `.browser-panel` / `.terminal-panel` / `.setup-wizard` | 各自 feature 内 |

## 生成新界面时的检查表

1. 底色 `--marine-paper`、面板 `--marine-white`——**两者之差本身就是分组手段，先别急着加边框**
2. 文字默认 12px / `--marine-ink`；次要信息降到 `#586d89`，别细到看不清
3. 强调只给一个元素上 `--marine-gold`；同屏两个金色元素就是失误
4. 圆角从「胶囊 / 10px / 14px」三档里选，不加第四档
5. 面板照抄承载面配方（1px `#ceddeb` + 14px + 极浅渐变 + 单片大软影）
6. 状态用 `data-state`，取值从既有 20 个里挑，不新造名字
7. 键盘可达、状态可见——这是契约要求，不是可选项

## 已知欠账（写给未来的自己）

- 6 个变量 vs 115 个硬编码色值：调色板事实上存在，只是没被约束。**建议把 line / muted / 成功 / 危险 / 警示正式提为变量**
- 圆角 8/9/10/11/12 五档并存、字号 9–32px 共 11 档：历史累积，新代码按本文收敛
- `is-warning` 与 `is-warn` 同义并存，应合并

> 生成或评审界面时，把这份文件当作**验收标准**，不是参考读物。
