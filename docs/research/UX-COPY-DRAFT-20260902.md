已完成取证（clone main 分支 → `apps/desktop/src/i18n.tsx` 全量 zh/en → `HarnessSurface.tsx`、`ShellApp.tsx` RuntimePanel/ManagedRuntimeSection/DiagnosticsSection 的使用上下文，并对照 `apps/desktop/src/contracts.ts` 核对后端枚举语义，避免改写丢安全语义；临时源码已清理，未改动仓库）。以下为两版文案表，直接作为修订建议提交。

# dsh-desktop-shell 界面文案人话化（zh/en 两版）

取证范围：`harness.*`、`runtime.*` 两组 i18n 键及其渲染上下文。改写原则：标签/字段名（状态、端点、延迟、代次等）不动；只改说明性长句、状态句与"标签+值"组合句；保留 `{placeholders}`、URL、`DSH`/`Managed`/`Attached` 等产品术语（首次出现括注白话，见术语表）。

## 术语对照表（建议全应用统一）

| 术语 | 建议 zh 统一写法 | 建议 en | 一句话白话 | 技术注 / 界面出现时写法 |
|---|---|---|---|---|
| Managed | 首现「Managed（本应用启动的实例）」，后续「本应用启动的实例」 | Managed instance — an instance this app started | 由本应用负责启动、停止、重启的那个 DSH 实例 | 应用持有其进程树句柄，启停只作用于该句柄、绑定具体代次；`lifecycleMutation=allowed`。界面作名词或定语，不复用做动词 |
| Attached | 首现「Attached（外部只读实例）」，后续「外部实例」 | Attached — an external instance connected read-only | 连接到一个在别处已经跑起来的 DSH 实例，只能看、不能在这里停/重启 | 只做只读探测；契约层恒为 `identity=unverified`、`processOwnership=external`、`lifecycleMutation=denied`，即"可达 ≠ 身份/归属"是写死的语义 |
| 代次（generation） | 「第 N 代（启动代号）」或「启动代号（代次）」 | generation — launch number | 同一环境每次启动编的号，启动一次 +1 | 端点验证绑定 `owned_generation_output_and_tcp`；停止确认、stale 提示等处建议保留"第 N 代"以便对照日志/后端 |
| Surface（DSH Surface） | 「DSH 界面」/「内嵌的 DSH 网页视图」 | DSH view — the embedded DSH web page | 窗口里嵌着显示的 DSH 官方网页界面 | 指原版 DSH 网页应用本体（native binding，exact-origin 验证后挂载）；不建议单独说"原生 Surface"，需要强调时用「原版 DSH 界面」 |
| 只读 | 「只读」+ 一句"只能查看" | read-only | 只能看和探测，不能通过本应用改变它 | Attached 的健康检查与生命周期禁止是产品不变量 |
| 策略 / 默认拒绝 | 「权限规则」「默认拒绝：没列出的都不允许」 | permission rules / deny by default | 一套"允许做什么"的清单；默认拒绝 = 清单外的动作一律不允许 | 对应 `DshSurfacePolicy`；fail-closed 是本产品核心语义，文案任何改写不得删去"默认拒绝"的表述 |
| 端点（endpoint） | 「访问地址」 | address | 形如 http://127.0.0.1:3989 的地址 | 对用户就是 URL；UI 内可用 `<code>` 保留原样 |
| 可达性（reachability） | 「能否连通」 | can it be reached | 探测某地址连不连得上 | 只有 reachable/refused/timeout/io_error 四种，改写成"能连上/被拒绝/超时"等值即可 |
| 进程归属（processOwnership） | 「是不是本应用启动的」 | whether this app started it | 该进程是归本应用管，还是外部进程 | 枚举 owned/none/external；展示给用户的是"本应用启动 / 外部进程" |
| 生命周期变更（mutation） | 「停止、重启等操作」 | stop / restart actions | 改变运行状态的那些操作 | 附属于 run/stop/restart 的能力集合 |
| 就绪/已验证（readiness/verified） | 「已验证」 | verified | 本应用确认过地址确实属于当前这代实例 | readiness=verified 且 processOwnership=owned 才允许挂载 DSH 界面 |

## 改写表

### DSH Surface 主区（HarnessSurface.tsx）

| 区块 | i18n key | 现值（zh） | 建议 zh | 建议 en | 改动理由（一句话） |
|---|---|---|---|---|---|
| 引导 | harness.bootstrap.reading | 正在读取运行时权威状态… | 正在读取运行时状态… | Reading the latest runtime state… | "权威/canonical"是内部契约腔，删掉不失语义 |
| 空态卡 | harness.empty.eyebrow | 无特权 DSH 面板 | 尚未连接 DSH 环境 | No DSH environment connected yet | "无特权"是权限模型行话，与用户此时要做的"选环境"无关 |
| 空态卡 | harness.empty.body | Shell 托管原版 DSH：不做 DOM 注入、无原生桥接。先验证环境，才可考虑原生 Surface。 | 本应用只展示 DSH 自己的网页界面：不改写页面内容，也不向页面开放本机能力。请先在「环境设置」中选择一个环境并完成验证。 | This app shows DSH's own web UI as-is: it never modifies the page and never gives the page access to your machine. Choose an environment in Settings and validate it first. | 原句三个术语连排（DOM 注入/原生桥接/原生 Surface），用户不知道在说什么 |
| 原生态 | harness.native.eyebrow | 原生生命周期 | DSH 界面 | DSH view | "原生生命周期"无可操作含义；此 eyebrow 实际只是内嵌区的标题帽 |
| 原生态 | harness.native.loading | 正在加载原生 DSH Surface… | 正在加载 DSH 界面… | Loading the DSH view… | "原生+Surface"双术语叠墙；白话统一为 DSH 界面 |
| 原生态 | harness.native.restoring | 正在恢复原生 DSH Surface… | 正在恢复 DSH 界面… | Restoring the DSH view… | 同上（hidden→restoring 的恢复态） |
| 原生态（仅读屏） | harness.native.ready | 原生 DSH Surface 就绪 | DSH 界面已就绪 | DSH view is ready | 与 loading/restoring 同一套白话 |
| 平台门控 | harness.platformGate.title | 原生 DSH Surface 未在 {platform} 上启用 | {platform} 上暂不支持内嵌 DSH 界面 | Embedding the DSH view is not supported on {platform} yet | 把契约词（原生/启用/平台门控）换成"在哪台机器上能不能内嵌"（含 {platform} 占位） |
| 平台门控 | harness.platformGate.body | 平台专属的权限拒绝钩子尚未通过实现门禁。 | 必需的平台权限检查尚未就绪，为安全起见不显示 DSH 界面。 | The required permission checks are not in place on this system yet, so the DSH view is kept closed for safety. | "拒绝钩子/实现门禁"是开发者黑话；用户只需知道"为什么看不到、是否安全" |
| 代次门控 | harness.generationGate.title | 原生 Surface 绑定已过期 | 当前 DSH 界面已过期 | The current DSH view is out of date | "绑定已过期"无主语且用词抽象 |
| 代次门控 | harness.generationGate.body | 挂载新代次前请重启或刷新 Managed 运行时。 | 请重启或刷新 Managed（本应用启动的）实例，界面会按最新代次重新加载。 | Restart or refresh the managed instance to reload the DSH view for its latest generation. | "挂载/代次"契约腔；改为明确的下一步动作 |
| 卸载态 | harness.unmounted.title | 原生 DSH Surface 已卸载 | DSH 界面当前未显示 | The DSH view is not shown right now | "卸载"是组件术语；白话只描述当前事实，不猜原因 |
| 布局门控 | harness.layoutGate.body | 原生 Surface 至少需要 320 × 240 可见 CSS 像素。 | DSH 界面至少需要 320×240 像素的空间才能显示。 | The DSH view needs at least 320 × 240 pixels of space. | "可见 CSS 像素"技术术语；单位与阈值本身保留 |
| 错误态 | harness.error.title | 原生 DSH Surface 需要处理 | DSH 界面出现问题 | There is a problem with the DSH view | zh 译文"需要处理"语义空洞；下方有具体错误信息 |
| 错误态 | harness.error.retry | 重试原生 Surface | 重新加载 DSH 界面 | Reload the DSH view | 按钮混排术语、无明确动作对象 |
| 错误态 | harness.error.operationFailed | 原生 Surface 操作失败。 | DSH 界面操作失败。 | The DSH view operation failed. | 兜底文案同样去掉"原生 Surface"术语墙 |
| 外接卡 | harness.attached.body | Attached 健康只上报有界可达性，绝不授予进程归属或生命周期变更。 | 这里只显示"能否连通"的探测结果。它是只读信息：不表示该实例由本应用管理，本应用也绝不会借此停止或重启它。 | This only shows whether the external instance responds on its address. It is read-only: it never means this app owns the instance, and this app never stops or restarts it based on it. | 把"有界可达性/进程归属/生命周期变更"拆成两个普通句，保留"只读+绝不控制"的双重安全语义 |
| 待启动卡 | harness.idle.title | DSH 保持有意不启动 | DSH 不会自动启动 | DSH won't start automatically | "保持有意不启动"生硬拗口；此句实为"不会自动拉起的保证" |
| 待启动卡 | harness.idle.body | 请用 Runtime 面板显式启动 Managed。恢复或保存环境时不会自动启动任何进程。 | 需要时请在左侧「运行时」面板手动启动 Managed（本应用启动的实例）。切换或保存环境都不会自动启动任何进程。 | Start the managed instance from the Runtime panel when you need it. Switching or saving an environment never starts any process on its own. | 术语统一 + 明确"在哪里点、不会自动发生什么"；后半个安全保证原样保留 |
| 策略卡 | harness.policy.eyebrow | 默认拒绝策略 | 权限默认拒绝 | Denied by default | 短标签保留原则但去掉"策略"二字补"默认拒绝"，配合下列清单自解释 |
| 策略卡 | harness.policy.title | DSH Surface 策略就绪 | DSH 界面权限 | DSH view permissions | "策略就绪"无主语；标题应说明"下面列的是 DSH 界面的权限" |
| 策略卡 | harness.policy.body | 原生 Surface 需要经过验证的自有 Managed 代次。 | 只有本应用启动（Managed）并通过验证的实例，才允许内嵌显示 DSH 界面。 | Only a DSH instance started by this app (Managed) and verified may show its view here. | 一句里叠了"验证/自有/Managed/代次/原生"五个契约词；改为条件句 |
| 策略待定 | harness.policy.pendingTitle | DSH Surface 策略待定。 | 权限规则尚未就绪。 | Permission rules are not ready yet. | 同上：标题应说"什么没准备好" |
| 策略待定 | harness.policy.pendingBody | 等待持久化的固定回环端点。 | 正在等待保存固定的本机（127.0.0.1）访问地址。 | Waiting for the app's fixed local (127.0.0.1) address to be saved. | "持久化/回环端点"术语；"本机 127.0.0.1 地址"用户一眼懂 |
| 底部权限条 | harness.footer.ipcDenied | 原生 IPC 已拒绝 | 已禁止页面调用本机功能 | The page cannot use native app features | 芯片要表达的是"页面没有本机权限"，不是"某个叫 IPC 的东西被拒" |
| 底部权限条 | harness.footer.permissionsDenied | 页面权限已拒绝 | 页面权限申请已被拒绝 | Page permission requests are denied | zh 缺主语/读起来像系统报错；语义本身不动 |
| 底部权限条 | harness.footer.exactOrigin | 仅限精确来源导航 | 只能访问 DSH 自身的页面地址 | Only the DSH's own address can be opened | "精确来源"是安全模型术语；用户视角是"这个页面不能乱跳转" |

### 运行时面板（ShellApp.tsx RuntimePanel / ManagedRuntimeSection）

| 区块 | i18n key | 现值（zh） | 建议 zh | 建议 en | 改动理由（一句话） |
|---|---|---|---|---|---|
| 面板帽 | runtime.eyebrow | 后端权威状态 | 来自本机后端 | Reported by the local backend | "权威"是 canonical 的直译契约腔；帽子仅说明数据来源即可 |
| 外接健康区 | runtime.attached.eyebrow | 只读端点证据 | 外部实例的只读探测结果 | Read-only probe results for the external instance | "端点证据"四字组合无用户含义，区块内容实为探测结果 |
| Managed 区 | runtime.managed.eyebrow | 自有进程树证据 | 本应用启动并管理的实例 | Instance started and managed by this app | "进程树证据"是所有权证明行话；用户需要知道的只是"这实例归本应用管" |
| 面板脚注（Attached 时） | runtime.note.attached | 生命周期控制不可用。Attached 可达性不代表 DSH 身份或 Desktop 进程归属。 | 此实例是在别处启动的（Attached），本应用只读连接，不能在这里停止或重启。能连上只代表地址可访问：不保证它就是当前环境，也不代表它归本应用管理。 | This instance was started elsewhere and is attached read-only, so it cannot be stopped or restarted here. Reachability only means the address responds — not that it is the expected environment or owned by this app. | 保留"可达≠身份、可达≠归属"双重否定语义，但把术语连缀拆成人话 |
| 面板脚注（Managed 时） | runtime.note.managed | Managed 控制只作用于保留的进程树句柄。已验证代次可挂载平台门控的原生 DSH Surface。 | 这里只能控制由本应用启动的实例：停止或重启只作用于它自己启动的那套进程，不影响其他程序。实例通过验证后，它的 DSH 界面即可内嵌显示（视系统支持而定）。 | Only instances started by this app can be controlled here: stop and restart affect only the processes this app launched. Once an instance is verified, its DSH view can be embedded here where the platform allows it. | "保留句柄/挂载/平台门控"换成用户动作语言；"不误伤其他进程"的保证显式化 |
| 停止确认 | runtime.confirmStop.body | 仅停止代次 {generation} 的保留进程树。不推断任何 PID 或端口归属。 | 将停止本应用为第 {generation} 代实例启动的进程；本应用不会凭 PID 或端口去关停其他程序。 | This stops only the processes this app started for generation {generation}. Other programs are never stopped based on guessed PID or port ownership. | "保留进程树/推断归属"无主语契约腔；改为"停什么、绝不停什么"（含 {generation}） |
| 端点确认 | runtime.verifiedEndpoint | 已验证端点：{endpoint} | 已确认的实例访问地址：{endpoint} | Confirmed address of this instance: {endpoint} | "端点"→"访问地址"，与术语表一致（含 {endpoint}） |

## 组合展示行（"标签+值"联动，需渲染层配合，非单个 key 可解决）

以下每一行现状即盲审例句（如「精确来源 http://127.0.0.1:3989」「外部链接…需用户操作」「自动打开：拒绝」）。dd 值来自 `DshSurfacePolicy` / 后端枚举，只改 dt 文案不够，需在渲染处加"值→本地化"映射或改写固定搭配：

| 出现处 | 现值（渲染结果） | 建议（渲染结果） | 说明 |
|---|---|---|---|
| 策略卡 dl：`harness.policy.exactOrigin` + `allowedOrigin` | 精确来源 http://127.0.0.1:3989 | DSH 页面地址：http://127.0.0.1:3989 | dt 改为"DSH 页面地址"，整行读作"允许加载这个地址"（en：DSH page address） |
| 策略卡 dl：`harness.policy.nativeIpc` + `privilegedIpc` | 原生 IPC denied | 调用本机功能：拒绝 | dt 白话为"调用本机功能"，dd 走枚举映射 denied→拒绝（en：Native app access / denied）；不要只改 dt 留英文 raw 值 |
| 策略卡 dl：`harness.policy.externalLinks` + `harness.policy.userAction` | 外部链接 需用户操作 | 打开外部链接：需你确认后才会打开 | dd 由 t(userAction) 改为自解释短句（en：opens only after your confirmation） |
| 策略卡 dl：`harness.policy.automaticOpen` + allowed/denied | 自动打开 拒绝 | 自动打开外部链接：不会（每次都要你手动） | dt 补"外部链接"限定，dd 给"不会自动打开"而非孤立"拒绝"（en：opens automatically：no） |
| 策略卡头部说明 | （无现成引导句） | 建议在 dl 前加一行小字：以下权限默认全部拒绝，只放行明确允许的项。 | 让"默认拒绝"原则不被误读成"全都不许用"（en：All permissions below are denied by default; only the listed exceptions are granted） |
| 运行状态字段 dd | 可达性：reachable；身份：unverified；进程归属：external；变更：denied（zh 界面出现英文 raw 枚举） | 可达性：可连通；身份：无法确认；进程归属：外部进程；停止/重启：不允许 | 需要一个渲染层枚举映射字典（zh/en），否则标签就算改白话，行内仍是中英混排；state/readiness/ownership/reachability/identity/stopDisposition 全部同理 |
| DSH Surface chrome | `<code>{origin}</code>` 旁 raw 状态字 `state`（mounting/ready/stale…） | 状态字同样走映射：加载中/就绪/已过期… | HarnessSurface.tsx:143 直接渲染 raw enum，i18n 表改不到 |

## 风险提示（改了可能误导安全语义，需人工复核）

1. **默认拒绝原则的落点**：`harness.policy.eyebrow/title` 从"策略就绪+默认拒绝策略"拆成"权限默认拒绝 + DSH 界面权限"，deny-by-default 只出现在 eyebrow 一处。复核时必须确认改动后页面上"未列出一律拒绝"仍显著可见，否则安全性展示降级为误导（en 同样要保留 deny by default 字样）。
2. **Attached 的"身份"语义**：`runtime.note.attached`、`harness.attached.body` 的白话版把"身份"说成"不保证是当前环境"。契约层 identity 恒为 unverified（`contracts.ts:170`），措辞成立；但要复核"可达不代表身份"没有被缩写成"可达只代表能连"——后半句丢失即变成"只要连上就是它"的错误暗示。
3. **停止范围承诺**：`runtime.confirmStop.body` 建议语"不会凭 PID 或端口去关停其他程序"是对原句"不推断任何 PID 或端口归属"的口语化。若后端实际会按端口占用结束进程（或文档有"端口冲突即停"的恢复逻辑），此承诺就过度了——需按 ManagedRuntimeStop 实现逐条核对后再定稿。
4. **"不会自动启动"的措辞边界**：`harness.idle.title/body` 保证的是"切换/保存环境不自动启动"，不是"永不自动启动"（恢复/自启策略可能在别处）。白话版不得扩写成"任何时候都不会自动启动"。
5. **平台门控的时态**：`harness.platformGate.title/body` 建议加"暂/尚未"，隐含"以后可能支持"。若该平台属硬性不支持（surface_create 不存在的平台），应去掉"暂"字，避免给用户虚假预期。
6. **移除"原生/native"的语义损失**：native 在本产品指"原版上游 DSH 应用 + owned/verified 绑定"，与 Attached 静态卡片、浏览器面板形成对照。全部删掉"原生"会让用户分不清"完整 DSH 应用视图"和"状态卡片"。建议只在正文叙事删，标题里如需区分仍保留"原版 DSH 界面"。
7. **"代次"不宜全删**：停止确认按钮与日志/诊断仍用 generation。术语表统一为"第 N 代（启动代号）"，确认文案与后端报错能对上号。
8. **策略 dl 行的语义细节**：`externalLinks`（externalHttpNavigation=delegate_with_user_action）与 `automaticOpen`（automaticExternalOpen=false）的建议措辞假设"外部链接需用户确认后交系统浏览器处理、绝不自动弹出"。这两项的准确 UX（delegate 去向、新窗口策略 newWindow=deny）需对照 DshSurfaceNavigationDecision 与对应 ADR 复核后再定稿。

## 范围外但同病（非 harness.\*/runtime.\* 组，建议同一轮顺手处理）

- `diagnostics.eyebrow`「免凭据快照 (AC-LOG-001)」→「诊断信息（本地生成，不含凭据）」——内部验收码 AC-* 直接进 UI 是盲审重灾区；`notifications.eyebrow/note`（ADR-0016）、`usage.eyebrow/note`（ADR-0016/AC-USG-*）同理，建议全部去掉括号内编号。
- `error.attachedUnavailable`「Attached 端点健康不可用。」→「无法获取外部实例的连通状态。」；`error.managedUnavailable`「Managed 运行时状态不可用。」→「无法获取本应用管理实例的状态。」；`error.surfacePolicyUnavailable` / `error.nativeSurfaceUnavailable` 系列照此统一。
- 后端直出的 `evidence[0].message`（ShellApp.tsx 两处 callout）是英文契约文案，直接展示在 zh 界面；需要后端按 locale 下发或前端映射，否则上述 i18n 全部改完仍会残留一处英文术语。
