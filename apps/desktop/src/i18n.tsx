import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";

/**
 * Minimal zh/en UI localization (WI-M6-I18N).
 *
 * zh holds the real Simplified-Chinese UI copy (default locale); en holds the
 * English equivalents. Product-mode terms (DSH / Managed / Attached / DSH
 * Surface) stay in English for consistency with the docs; technical tokens
 * (URL / Surface / tokens) stay as-is. Placeholders ({name}) are verbatim.
 */

export type Lang = "zh" | "en";

export const LANG_STORAGE_KEY = "dsh-lang";

export const defaultLang: Lang = "zh";

type TranslationKey = string;

export const translations: Record<Lang, Record<TranslationKey, string>> = {
  zh: {
    // Rail
    "rail.dsh": "DSH",
    "rail.browser": "浏览器",
    "rail.terminal": "终端",
    "rail.notifications": "通知",
    "rail.usage": "用量",
    "rail.timer": "计时器（M3）",
    "rail.runtime": "运行时",
    "rail.settings": "设置",
    "rail.aria.surfaces": "桌面面板",
    "rail.aria.brand": "DSH Desktop Shell",
    "lang.label": "语言",

    // Shell header
    "shell.eyebrow": "DSH Desktop Shell",
    "surface.dsh": "DSH Surface",
    "surface.browser": "浏览器",
    "surface.terminal": "持久终端",
    "surface.runtime": "运行时",
    "surface.settings": "环境设置",
    "surface.notifications": "通知",
    "surface.usage": "用量",

    // Common
    "common.ok": "确定",
    "common.cancel": "取消",
    "common.close": "关闭",
    "common.refresh": "刷新",
    "common.none": "无",
    "common.loading": "加载中",
    "common.notSelected": "未选择",

    // Browser panel
    "browser.aria": "浏览器",
    "browser.noSession": "无浏览器会话",
    "browser.urlLabel": "浏览器 URL",
    "browser.open": "打开",
    "browser.reload": "重新加载",
    "browser.session": "会话",
    "browser.currentUrl": "当前 URL",
    "browser.state": "状态",
    "browser.error": "错误",
    "browser.pageLoadFailed": "页面加载失败",
    "browser.error.navigation": "浏览器导航不可用。",
    "browser.error.reload": "浏览器重新加载不可用。",
    "browser.error.close": "浏览器关闭不可用。",

    // Terminal panel
    "terminal.aria": "持久终端",
    "terminal.fallbackSession": "终端",
    "terminal.unavailable": "终端不可用：",

    // Runtime panel
    "runtime.eyebrow": "后端权威状态",
    "runtime.title": "运行时快照",
    "runtime.phase": "阶段",
    "runtime.state": "状态",
    "runtime.environment": "环境",
    "runtime.generation": "代次",
    "runtime.attached.eyebrow": "只读端点证据",
    "runtime.attached.title": "Attached 健康",
    "runtime.probing": "探测中…",
    "runtime.probeAgain": "再次探测",
    "runtime.reachability": "可达性",
    "runtime.identity": "身份",
    "runtime.processOwnership": "进程归属",
    "runtime.mutation": "变更",
    "runtime.endpoint": "端点",
    "runtime.latency": "延迟",
    "runtime.notAvailable": "不可用",
    "runtime.note.attached": "生命周期控制不可用。Attached 可达性不代表 DSH 身份或 Desktop 进程归属。",
    "runtime.attached.autoPortNeeded": "此 Attached 环境未配置具体端口（auto），无法探测。请在设置中编辑该环境，填写运行中 DSH 的实际端口。",
    "runtime.note.managed": "Managed 控制只作用于保留的进程树句柄。已验证代次可挂载平台门控的原生 DSH Surface。",
    "runtime.managed.eyebrow": "自有进程树证据",
    "runtime.managed.title": "Managed 运行时",
    "runtime.starting": "启动中…",
    "runtime.start": "启动 Managed DSH",
    "runtime.restarting": "重启中…",
    "runtime.restart": "重启 Managed DSH",
    "runtime.reviewStop": "确认 Managed 停止",
    "runtime.readiness": "就绪度",
    "runtime.instance": "实例",
    "runtime.stopDisposition": "停止方式",
    "runtime.recoveryCrashes": "恢复期崩溃",
    "runtime.recoveryState": "恢复状态",
    "runtime.recoverySafeStop": "安全停止",
    "runtime.recoveryBounded": "有界恢复",
    "runtime.verifiedEndpoint": "已验证端点：{endpoint}",
    "runtime.confirmStop.aria": "确认 Managed 停止",
    "runtime.confirmStop.body": "仅停止代次 {generation} 的保留进程树。不推断任何 PID 或端口归属。",
    "runtime.stopping": "停止中…",
    "runtime.confirmStop.action": "确认停止代次 {generation}",

    // Diagnostics
    "diagnostics.eyebrow": "免凭据快照 (AC-LOG-001)",
    "diagnostics.title": "诊断",
    "diagnostics.observed": "观测时间",
    "diagnostics.runtimeState": "运行时状态",
    "diagnostics.surface": "Surface",
    "diagnostics.process": "进程",
    "diagnostics.catalogRevision": "目录版本",
    "diagnostics.visible": " · 可见",
    "diagnostics.retained": "已保留",
    "diagnostics.notRetained": "未保留",
    "diagnostics.owned": " · 自有",
    "diagnostics.notAvailable": "诊断暂不可用。",

    // Notifications
    "notifications.eyebrow": "本地优先审计轨迹 (ADR-0016)",
    "notifications.loading": "加载通知中…",
    "notifications.empty": "暂无通知。",
    "notifications.deduplicated": "已去重",
    "notifications.dismiss": "关闭",
    "notifications.note": "内容遵循通知策略 (ADR-0016)：仅 explicit_body 通知携带正文，每条通知都会记录到本地 AppData 审计轨迹。",

    // Usage
    "usage.eyebrow": "本地优先用量账本 (ADR-0016)",
    "usage.inputTokens": "输入 tokens",
    "usage.outputTokens": "输出 tokens",
    "usage.estimates": "估算数",
    "usage.cost": "成本",
    "usage.loading": "加载用量中…",
    "usage.empty": "暂无用量记录。",
    "usage.estimate": "估算",
    "usage.inOut": "{input} 进 · {output} 出",
    "usage.note": "用量记录只携带来源、时段与 token 估算——绝不含终端输出或通知内容 (AC-USG-001)——且只保留在本机 (AC-USG-002)。",


    // Harness surface (HarnessSurface.tsx)
    "harness.bootstrap.eyebrow": "启动引导",
    "harness.bootstrap.reading": "正在读取运行时权威状态…",
    "harness.empty.eyebrow": "无特权 DSH 面板",
    "harness.empty.title": "选择现有 DSH 环境",
    "harness.empty.body": "Shell 托管原版 DSH：不做 DOM 注入、无原生桥接。先验证环境，才可考虑原生 Surface。",
    "harness.empty.openSettings": "打开环境设置",
    "harness.native.eyebrow": "原生生命周期",
    "harness.native.restoring": "正在恢复原生 DSH Surface…",
    "harness.native.loading": "正在加载原生 DSH Surface…",
    "harness.native.ready": "原生 DSH Surface 就绪",
    "harness.platformGate.eyebrow": "平台门控",
    "harness.platformGate.title": "原生 DSH Surface 未在 {platform} 上启用",
    "harness.platformGate.body": "平台专属的权限拒绝钩子尚未通过实现门禁。",
    "harness.generationGate.eyebrow": "代次门控",
    "harness.generationGate.title": "原生 Surface 绑定已过期",
    "harness.generationGate.body": "挂载新代次前请重启或刷新 Managed 运行时。",
    "harness.unmounted.title": "原生 DSH Surface 已卸载",
    "harness.layoutGate.eyebrow": "布局门控",
    "harness.layoutGate.title": "请放大窗口以显示原生 DSH",
    "harness.layoutGate.body": "原生 Surface 至少需要 320 × 240 可见 CSS 像素。",
    "harness.error.title": "原生 DSH Surface 需要处理",
    "harness.error.retry": "重试原生 Surface",
    "harness.error.retrying": "重试中…",
    "harness.footer.aria": "原生 Surface 策略",
    "harness.footer.ipcDenied": "原生 IPC 已拒绝",
    "harness.footer.permissionsDenied": "页面权限已拒绝",
    "harness.footer.exactOrigin": "仅限精确来源导航",
    "harness.attached.title": "Attached DSH 保持只读",
    "harness.attached.body": "Attached 健康只上报有界可达性，绝不授予进程归属或生命周期变更。",
    "harness.idle.title": "DSH 保持有意不启动",
    "harness.idle.body": "请用 Runtime 面板显式启动 Managed。恢复或保存环境时不会自动启动任何进程。",
    "harness.validated.eyebrow": "环境已验证",
    "harness.policy.eyebrow": "默认拒绝策略",
    "harness.policy.title": "DSH Surface 策略就绪",
    "harness.policy.body": "原生 Surface 需要经过验证的自有 Managed 代次。",
    "harness.policy.exactOrigin": "精确来源",
    "harness.policy.nativeIpc": "原生 IPC",
    "harness.policy.externalLinks": "外部链接",
    "harness.policy.automaticOpen": "自动打开",
    "harness.policy.userAction": "需用户操作",
    "harness.policy.allowed": "允许",
    "harness.policy.denied": "拒绝",
    "harness.policy.pendingTitle": "DSH Surface 策略待定。",
    "harness.policy.pendingBody": "等待持久化的固定回环端点。",
    "harness.meta.ownership": "归属",
    "harness.meta.profile": "Profile",
    "harness.meta.runtime": "运行时",
    "harness.meta.generation": "代次",
    "harness.error.operationFailed": "原生 Surface 操作失败。",
    // Environment list (EnvironmentList.tsx)
    "envlist.title": "环境",
    "envlist.activate": "激活",
    "envlist.switching": "切换中…",
    "envlist.errorActivate": "环境无法激活。",
    // Command error fallbacks (shown in callouts when the backend error has no message)
    "error.desktopUnavailable": "桌面后端不可用。",
    "error.attachedUnavailable": "Attached 端点健康不可用。",
    "error.managedUnavailable": "Managed 运行时状态不可用。",
    "error.diagnosticsUnavailable": "诊断不可用。",
    "error.surfacePolicyUnavailable": "DSH Surface 策略不可用。",
    "error.nativeSurfaceUnavailable": "原生 DSH Surface 不可用。",
    "error.surfaceStatusUnavailable": "原生 DSH Surface 状态不可用。",
    "error.managedStart": "Managed 启动不可用。",
    "error.managedStop": "Managed 停止不可用。",
    "error.managedRestart": "Managed 重启不可用。",
    "error.surfaceRetry": "原生 DSH Surface 重试失败。",
    "error.savedRefresh": "已保存，但运行时快照无法刷新。",
    "error.notificationsUnavailable": "通知不可用。",
    "error.notificationDismiss": "通知关闭失败。",
    "error.usageUnavailable": "用量快照不可用。",
    // Setup wizard (SetupWizard.tsx)
    "wizard.step.mode": "模式",
    "wizard.step.harness": "DSH 来源",
    "wizard.step.profile": "Profile",
    "wizard.step.advanced": "高级设置",
    "wizard.step.review": "确认",
    "wizard.step.finish": "保存并启动",
    "wizard.aria.steps": "设置向导步骤",
    "wizard.nav.back": "返回",
    "wizard.nav.next": "下一步",
    "wizard.mode.legend": "这个环境如何运行？",
    "wizard.mode.managed": "Managed",
    "wizard.mode.managed.desc": "从本机源码仓库启动并托管一个 DSH 进程（重启、健康、代次）。",
    "wizard.mode.attached": "Attached",
    "wizard.mode.attached.desc": "连接一个已在运行的 DSH 实例（只读生命周期）。",
    "wizard.source.intro": "DSH 以源码仓库形式分发。输入已 clone 的 deepseek-harness 仓库目录（官方推荐方式）。Shell 负责启动；依赖安装与构建需要仓库先就绪（见下方状态）。npx 属于手动 Attached 场景，不在向导内。",
    "wizard.source.placeholder": "C:\\path\\to\\deepseek-harness",
    "wizard.source.check": "仓库有效性检测",
    "wizard.source.checking": "检测中…",
    "wizard.source.probeFirst": "点击「仓库有效性检测」验证该目录是否可用。",
    "wizard.source.none": "未找到可用候选。",
    "wizard.source.repo": "源码仓库",
    "wizard.source.entry": "入口",
    "wizard.source.loader": "TS 加载器",
    "wizard.source.installMissing": "依赖未安装",
    "wizard.source.installReady": "依赖已安装",
    "wizard.source.buildMissing": "Web 资源未构建",
    "wizard.source.buildReady": "Web 资源已构建",
    "wizard.source.legacyBadge": "旧版可执行文件来源（兼容）",
    "wizard.source.legacyNote": "此环境使用旧版可执行文件来源（schema 兼容保留）。新环境请使用源码仓库。",
    "wizard.source.fileCandidate": "这是可执行文件而不是源码仓库——请粘贴仓库目录路径。",
    "wizard.browse": "浏览…",
    "wizard.source.version": "版本",
    "wizard.source.install": "依赖",
    "wizard.source.build": "Web 资源",
    "wizard.clone.title": "还没有 DSH 源码仓库？",
    "wizard.clone.body": "clone 完成后，首次使用前请先在仓库目录内执行 pnpm install 与 pnpm run build（Shell 当前不会自动执行）。就绪后把目录填到上方输入框即可。",
    "wizard.clone.command": "git clone --depth 1 https://github.com/deepseek-ai/deepseek-harness {target}",
    "wizard.clone.enterTarget": "先在上方输入要 clone 到的目录路径。",
    "wizard.identity.label": "Profile 名称",
    "wizard.identity.labelPlaceholder": "Local DSH",
    "wizard.identity.idAuto": "Profile-ID（自动生成）：{id}",
    "wizard.profile.intro": "Profile 位于 <DSH_HOME>/profiles/<name>。",
    "wizard.profile.homePlaceholder": "C:\\Users\\you\\.dsh",
    "wizard.profile.scan": "扫描",
    "wizard.profile.scanning": "扫描中…",
    "wizard.profile.none": "该 DSH_HOME 下未找到 Profile。",
    "wizard.profile.noConfig": "无 cordis.yml",
    "wizard.profile.new": "或指定 Profile 名称（作为 --profile 传入 DSH）：",
    "wizard.profile.namePlaceholder": "my-profile",
    "wizard.advanced.port": "DSH Web 端口",
    "wizard.advanced.portHint": "dsh web 监听端口；auto = 自动分配空闲端口。Managed 模式建议填固定端口（部分 DSH 版本不输出就绪标记，auto 会启动超时）。",
    "wizard.advanced.portAttachedHint": "Attached 模式必须填写运行中 DSH 的实际端口。",
    "wizard.advanced.portPlaceholder": "auto",
    "wizard.advanced.check": "检查",
    "wizard.advanced.checking": "检查中…",
    "wizard.advanced.portBusy": "端口 {port} 已被占用。",
    "wizard.advanced.portFree": "端口 {port} 空闲。",
    "wizard.advanced.nodePath": "Node.js 可执行文件",
    "wizard.advanced.nodePathHint": "留空则自动从 PATH 探测。",
    "wizard.advanced.cwd": "工作目录",
    "wizard.advanced.cwdHint": "留空默认为仓库根目录。",
    "wizard.advanced.args": "附加参数",
    "wizard.advanced.argsHint": "每行一个；--host/--port/--trusted-host/--no-open 由 Shell 托管，不可填写。",
    "wizard.review.mode": "模式",
    "wizard.review.source": "来源",
    "wizard.review.home": "DSH_HOME",
    "wizard.review.profile": "Profile",
    "wizard.review.port": "端口",
    "wizard.review.node": "Node",
    "wizard.review.cwd": "工作目录",
    "wizard.review.validate": "校验",
    "wizard.review.validating": "校验中…",
    "wizard.review.passed": "校验通过",
    "wizard.review.ready": "可以启动",
    "wizard.finish.saveManaged": "保存 {label}（Managed 环境）并启动 DSH。",
    "wizard.finish.saveAttached": "保存 {label}（Attached 环境）并验证运行中的 DSH。",
    "wizard.finish.action": "保存并启动",
    "wizard.finish.working": "处理中…",
    "wizard.finish.savedAt": "已保存到 catalog（revision {revision}）。",
    "wizard.finish.launch.managed": "环境已保存，DSH 正在启动。",
    "wizard.finish.launch.attachedOk": "环境已保存，Attached DSH 可达。",
    "wizard.finish.launch.attachedMiss": "环境已保存；Attached DSH 不可达。",
    "wizard.finish.launch.attachedAuto": "环境已保存。Attached 环境需在高级设置中指定运行中 DSH 的实际端口后才能验证连接。",
    "wizard.error.launch": "环境已保存，但 DSH 启动失败。",
    "wizard.error.attachProbe": "环境已保存，但 Attached 连接验证失败。",
    "wizard.error.discovery": "DSH 探测不可用。",
    "wizard.error.browse": "目录选择不可用。",
    "wizard.error.homeFirst": "请先输入 DSH_HOME 目录。",
    "wizard.error.profileScan": "Profile 扫描失败——请检查 DSH_HOME 路径。",
    "wizard.error.portInvalid": "端口必须是 1024–65535 之间的数字。",
    "wizard.error.idTaken": "该 Profile-ID 已被其他环境使用。请修改 Profile 名称以生成不同 ID，或先编辑已有环境。",
    "wizard.finish.launch.repoUnready": "环境已保存，但仓库依赖/Web 资源未就绪，未自动启动。请先在仓库目录执行 pnpm install 与 pnpm run build，再从运行时面板启动。",
    "wizard.finish.launch.repoAutoPort": "环境已保存，但端口为 auto 且当前 DSH 不输出就绪标记，无法自动启动。请在高级设置中填写固定端口（如 3081），再保存并启动。",
    "wizard.error.probe": "端口探测不可用。",
    "wizard.error.fixFields": "请先修正高亮字段。",
    "wizard.error.validation": "环境校验未通过——见下方问题。",
    "wizard.error.validationBackend": "环境校验后端不可用。",
    "wizard.error.save": "环境保存或启动失败。",
  },
  en: {
    "rail.dsh": "DSH",
    "rail.browser": "Browser",
    "rail.terminal": "Terminal",
    "rail.notifications": "Notifications",
    "rail.usage": "Usage",
    "rail.timer": "Timer (M3)",
    "rail.runtime": "Runtime",
    "rail.settings": "Settings",
    "rail.aria.surfaces": "Desktop surfaces",
    "rail.aria.brand": "DSH Desktop Shell",
    "lang.label": "Language",

    "shell.eyebrow": "DSH Desktop Shell",
    "surface.dsh": "DSH Surface",
    "surface.browser": "Browser",
    "surface.terminal": "Persistent Terminal",
    "surface.runtime": "Runtime",
    "surface.settings": "Environment Settings",
    "surface.notifications": "Notifications",
    "surface.usage": "Usage",

    "common.ok": "OK",
    "common.cancel": "Cancel",
    "common.close": "Close",
    "common.refresh": "Refresh",
    "common.none": "none",
    "common.loading": "loading",
    "common.notSelected": "not selected",

    "browser.aria": "Browser",
    "browser.noSession": "no browser session",
    "browser.urlLabel": "Browser URL",
    "browser.open": "Open",
    "browser.reload": "Reload",
    "browser.session": "Session",
    "browser.currentUrl": "Current URL",
    "browser.state": "State",
    "browser.error": "Error",
    "browser.pageLoadFailed": "Page failed to load",
    "browser.error.navigation": "Browser navigation is unavailable.",
    "browser.error.reload": "Browser reload is unavailable.",
    "browser.error.close": "Browser close is unavailable.",

    "terminal.aria": "Persistent terminal",
    "terminal.fallbackSession": "terminal",
    "terminal.unavailable": "Terminal unavailable: ",

    "runtime.eyebrow": "Canonical backend state",
    "runtime.title": "Runtime snapshot",
    "runtime.phase": "Phase",
    "runtime.state": "State",
    "runtime.environment": "Environment",
    "runtime.generation": "Generation",
    "runtime.attached.eyebrow": "Read-only endpoint evidence",
    "runtime.attached.title": "Attached health",
    "runtime.probing": "Probing…",
    "runtime.probeAgain": "Probe again",
    "runtime.reachability": "Reachability",
    "runtime.identity": "Identity",
    "runtime.processOwnership": "Process ownership",
    "runtime.mutation": "Mutation",
    "runtime.endpoint": "Endpoint",
    "runtime.latency": "Latency",
    "runtime.notAvailable": "not available",
    "runtime.note.attached":
      "Lifecycle controls remain unavailable. Attached reachability never implies DSH identity or Desktop process ownership.",
    "runtime.attached.autoPortNeeded": "This Attached environment has no concrete port (auto), so it cannot be probed. Edit the environment in Settings and enter the actual port of the running DSH.",
    "runtime.note.managed":
      "Managed controls act only on the retained process-tree handle. A verified generation may mount the platform-gated native DSH Surface.",
    "runtime.managed.eyebrow": "Owned process-tree evidence",
    "runtime.managed.title": "Managed runtime",
    "runtime.starting": "Starting…",
    "runtime.start": "Start Managed DSH",
    "runtime.restarting": "Restarting…",
    "runtime.restart": "Restart managed DSH",
    "runtime.reviewStop": "Review managed stop",
    "runtime.readiness": "Readiness",
    "runtime.instance": "Instance",
    "runtime.stopDisposition": "Stop disposition",
    "runtime.recoveryCrashes": "Recovery crashes",
    "runtime.recoveryState": "Recovery state",
    "runtime.recoverySafeStop": "safe stop",
    "runtime.recoveryBounded": "bounded recovery",
    "runtime.verifiedEndpoint": "Verified endpoint: {endpoint}",
    "runtime.confirmStop.aria": "Confirm managed stop",
    "runtime.confirmStop.body":
      "Stop only the retained process tree for generation {generation}. No PID or port ownership will be inferred.",
    "runtime.stopping": "Stopping…",
    "runtime.confirmStop.action": "Confirm stop generation {generation}",

    "diagnostics.eyebrow": "Credential-free snapshot (AC-LOG-001)",
    "diagnostics.title": "Diagnostics",
    "diagnostics.observed": "Observed",
    "diagnostics.runtimeState": "Runtime state",
    "diagnostics.surface": "Surface",
    "diagnostics.process": "Process",
    "diagnostics.catalogRevision": "Catalog revision",
    "diagnostics.visible": " · visible",
    "diagnostics.retained": "retained",
    "diagnostics.notRetained": "not retained",
    "diagnostics.owned": " · owned",
    "diagnostics.notAvailable": "Diagnostics are not available yet.",

    "notifications.eyebrow": "Local-first audit trail (ADR-0016)",
    "notifications.loading": "Loading notifications…",
    "notifications.empty": "No notifications yet.",
    "notifications.deduplicated": "deduplicated",
    "notifications.dismiss": "Dismiss",
    "notifications.note":
      "Content follows the notification policy (ADR-0016): only explicit_body notifications carry a body, and every notification is recorded in the local AppData audit trail.",

    "usage.eyebrow": "Local-first usage ledger (ADR-0016)",
    "usage.inputTokens": "Input tokens",
    "usage.outputTokens": "Output tokens",
    "usage.estimates": "Estimates",
    "usage.cost": "Cost",
    "usage.loading": "Loading usage…",
    "usage.empty": "No usage records yet.",
    "usage.estimate": "estimate",
    "usage.inOut": "{input} in · {output} out",
    "usage.note":
      "Usage records carry only source, period and token estimates — never terminal output or notification content (AC-USG-001) — and stay on this device (AC-USG-002).",


    // Harness surface (HarnessSurface.tsx)
    "harness.bootstrap.eyebrow": "Shell bootstrap",
    "harness.bootstrap.reading": "Reading canonical runtime state…",
    "harness.empty.eyebrow": "Unprivileged DSH surface",
    "harness.empty.title": "Choose an existing DSH environment",
    "harness.empty.body": "The Shell hosts upstream DSH without DOM injection or a native bridge. Validate an environment before a native Surface can be considered.",
    "harness.empty.openSettings": "Open Environment Settings",
    "harness.native.eyebrow": "Native lifecycle",
    "harness.native.restoring": "Restoring native DSH Surface…",
    "harness.native.loading": "Loading native DSH Surface…",
    "harness.native.ready": "Native DSH Surface ready",
    "harness.platformGate.eyebrow": "Platform gate",
    "harness.platformGate.title": "Native DSH Surface is not enabled on {platform}",
    "harness.platformGate.body": "The platform-specific permission-denial hooks have not passed their implementation gate.",
    "harness.generationGate.eyebrow": "Generation gate",
    "harness.generationGate.title": "The native Surface binding is stale",
    "harness.generationGate.body": "Restart or refresh the Managed runtime before mounting another generation.",
    "harness.unmounted.title": "The native DSH Surface is unmounted",
    "harness.layoutGate.eyebrow": "Layout gate",
    "harness.layoutGate.title": "Expand the window to show native DSH",
    "harness.layoutGate.body": "The native Surface requires at least 320 × 240 visible CSS pixels.",
    "harness.error.title": "Native DSH Surface needs attention",
    "harness.error.retry": "Retry native Surface",
    "harness.error.retrying": "Retrying…",
    "harness.footer.aria": "Native Surface policy",
    "harness.footer.ipcDenied": "Native IPC denied",
    "harness.footer.permissionsDenied": "Page permissions denied",
    "harness.footer.exactOrigin": "Exact-origin navigation only",
    "harness.attached.title": "Attached DSH remains read-only",
    "harness.attached.body": "Attached health can report bounded reachability, but it never grants process ownership or lifecycle mutation.",
    "harness.idle.title": "DSH launch remains intentionally idle",
    "harness.idle.body": "Use the Runtime surface for explicit Managed start. No process is launched automatically when an Environment is restored or saved.",
    "harness.validated.eyebrow": "Environment validated",
    "harness.policy.eyebrow": "Fail-closed policy",
    "harness.policy.title": "DSH Surface policy ready",
    "harness.policy.body": "A native Surface requires a verified, owned Managed generation.",
    "harness.policy.exactOrigin": "Exact origin",
    "harness.policy.nativeIpc": "Native IPC",
    "harness.policy.externalLinks": "External links",
    "harness.policy.automaticOpen": "Automatic open",
    "harness.policy.userAction": "user action",
    "harness.policy.allowed": "allowed",
    "harness.policy.denied": "denied",
    "harness.policy.pendingTitle": "DSH Surface policy pending.",
    "harness.policy.pendingBody": "Waiting for a persisted fixed loopback endpoint.",
    "harness.meta.ownership": "Ownership",
    "harness.meta.profile": "Profile",
    "harness.meta.runtime": "Runtime",
    "harness.meta.generation": "Generation",
    "harness.error.operationFailed": "The native Surface operation failed.",
    // Environment list (EnvironmentList.tsx)
    "envlist.title": "Environments",
    "envlist.activate": "Activate",
    "envlist.switching": "Switching…",
    "envlist.errorActivate": "The environment could not be activated.",
    "error.desktopUnavailable": "Desktop backend is unavailable.",
    "error.attachedUnavailable": "Attached endpoint health is unavailable.",
    "error.managedUnavailable": "Managed runtime status is unavailable.",
    "error.diagnosticsUnavailable": "Diagnostics are unavailable.",
    "error.surfacePolicyUnavailable": "DSH Surface policy is unavailable.",
    "error.nativeSurfaceUnavailable": "Native DSH Surface is unavailable.",
    "error.surfaceStatusUnavailable": "Native DSH Surface status is unavailable.",
    "error.managedStart": "Managed start is unavailable.",
    "error.managedStop": "Managed stop is unavailable.",
    "error.managedRestart": "Managed restart is unavailable.",
    "error.surfaceRetry": "Native DSH Surface retry failed.",
    "error.savedRefresh": "Saved, but the runtime snapshot could not be refreshed.",
    "error.notificationsUnavailable": "Notifications are unavailable.",
    "error.notificationDismiss": "Notification dismiss failed.",
    "error.usageUnavailable": "Usage snapshot is unavailable.",
    // Setup wizard (SetupWizard.tsx)
    "wizard.step.mode": "Mode",
    "wizard.step.harness": "DSH source",
    "wizard.step.profile": "Profile",
    "wizard.step.advanced": "Advanced",
    "wizard.step.review": "Review",
    "wizard.step.finish": "Save & launch",
    "wizard.aria.steps": "Setup wizard steps",
    "wizard.nav.back": "Back",
    "wizard.nav.next": "Next",
    "wizard.mode.legend": "How will this environment run?",
    "wizard.mode.managed": "Managed",
    "wizard.mode.managed.desc": "Launch a DSH process from a local source repository on this machine and supervise it (restart, health, generation).",
    "wizard.mode.attached": "Attached",
    "wizard.mode.attached.desc": "Connect to a DSH instance that is already running (read-only lifecycle).",
    "wizard.source.intro": "DSH is distributed as a source repository. Enter the directory of a deepseek-harness clone (the recommended way). The Shell handles launch; dependencies and web assets must be ready in the repo first (see the status below). npx belongs to manual Attached scenarios and is not covered here.",
    "wizard.source.placeholder": "C:\\path\\to\\deepseek-harness",
    "wizard.source.check": "Check repository",
    "wizard.source.checking": "Checking…",
    "wizard.source.probeFirst": "Click “Check repository” to verify this directory.",
    "wizard.source.none": "No usable candidates found.",
    "wizard.source.repo": "Source repository",
    "wizard.source.entry": "Entry",
    "wizard.source.loader": "TS loader",
    "wizard.source.installMissing": "Dependencies not installed",
    "wizard.source.installReady": "Dependencies installed",
    "wizard.source.buildMissing": "Web assets not built",
    "wizard.source.buildReady": "Web assets built",
    "wizard.source.legacyBadge": "Legacy executable source (compat)",
    "wizard.source.legacyNote": "This environment uses the legacy executable source (kept for schema compatibility). New environments should use a source repository.",
    "wizard.source.fileCandidate": "This is an executable file, not a source repository — paste the repository directory path instead.",
    "wizard.browse": "Browse…",
    "wizard.source.version": "Version",
    "wizard.source.install": "Dependencies",
    "wizard.source.build": "Web assets",
    "wizard.clone.title": "No DeepSeek Harness checkout yet?",
    "wizard.clone.body": "After cloning, run pnpm install and pnpm run build inside the repository before first use (the Shell does not run them yet). Then enter the directory above.",
    "wizard.clone.command": "git clone --depth 1 https://github.com/deepseek-ai/deepseek-harness {target}",
    "wizard.clone.enterTarget": "Enter the target directory path above first.",
    "wizard.identity.label": "Profile name",
    "wizard.identity.labelPlaceholder": "Local DSH",
    "wizard.identity.idAuto": "Profile ID (auto): {id}",
    "wizard.profile.intro": "Profiles live under <DSH_HOME>/profiles/<name>.",
    "wizard.profile.homePlaceholder": "C:\\Users\\you\\.dsh",
    "wizard.profile.scan": "Scan",
    "wizard.profile.scanning": "Scanning…",
    "wizard.profile.none": "No profiles found under this DSH_HOME.",
    "wizard.profile.noConfig": "no cordis.yml",
    "wizard.profile.new": "Or specify a profile name (passed to DSH as --profile):",
    "wizard.profile.namePlaceholder": "my-profile",
    "wizard.advanced.port": "DSH web port",
    "wizard.advanced.portHint": "Port for the dsh web service; auto picks a free port. Managed mode prefers a fixed port (some DSH builds print no readiness marker and auto would time out).",
    "wizard.advanced.portAttachedHint": "Attached mode needs the actual port of the running DSH.",
    "wizard.advanced.portPlaceholder": "auto",
    "wizard.advanced.check": "Check",
    "wizard.advanced.checking": "Probing…",
    "wizard.advanced.portBusy": "Port {port} is already in use.",
    "wizard.advanced.portFree": "Port {port} is free.",
    "wizard.advanced.nodePath": "Node.js executable",
    "wizard.advanced.nodePathHint": "Leave empty to probe PATH automatically.",
    "wizard.advanced.cwd": "Working directory",
    "wizard.advanced.cwdHint": "Defaults to the repository root when empty.",
    "wizard.advanced.args": "Extra arguments",
    "wizard.advanced.argsHint": "One per line; --host/--port/--trusted-host/--no-open are Shell-owned and cannot be set here.",
    "wizard.review.mode": "Mode",
    "wizard.review.source": "Source",
    "wizard.review.home": "DSH_HOME",
    "wizard.review.profile": "Profile",
    "wizard.review.port": "Port",
    "wizard.review.node": "Node",
    "wizard.review.cwd": "Working directory",
    "wizard.review.validate": "Validate",
    "wizard.review.validating": "Validating…",
    "wizard.review.passed": "Validation passed",
    "wizard.review.ready": "ready to launch",
    "wizard.finish.saveManaged": "Save {label} as a managed environment and start DSH.",
    "wizard.finish.saveAttached": "Save {label} as an attached environment and verify the running DSH.",
    "wizard.finish.action": "Save & launch",
    "wizard.finish.working": "Working…",
    "wizard.finish.savedAt": "Saved at catalog revision {revision}.",
    "wizard.finish.launch.managed": "Environment saved and DSH is starting.",
    "wizard.finish.launch.attachedOk": "Environment saved and the attached DSH is reachable.",
    "wizard.finish.launch.attachedMiss": "Environment saved; the attached DSH was not reachable.",
    "wizard.finish.launch.attachedAuto": "Environment saved. Attached verification needs a real port from the running DSH — set it in Advanced.",
    "wizard.error.launch": "Environment saved, but DSH failed to start.",
    "wizard.error.attachProbe": "Environment saved, but the attached health check failed.",
    "wizard.error.discovery": "DSH discovery is unavailable.",
    "wizard.error.browse": "Folder picker is unavailable.",
    "wizard.error.homeFirst": "Enter a DSH_HOME directory first.",
    "wizard.error.profileScan": "Profile scan failed — check the DSH_HOME path.",
    "wizard.error.portInvalid": "Port must be a number between 1024 and 65535.",
    "wizard.error.idTaken": "This Profile ID is already used by another environment. Change the Profile name to derive a different ID, or edit the existing environment.",
    "wizard.finish.launch.repoUnready": "Environment saved, but repo dependencies/web assets are not ready, so DSH was not started automatically. Run pnpm install and pnpm run build in the repository, then start it from the runtime panel.",
    "wizard.finish.launch.repoAutoPort": "Environment saved, but the port is auto and this DSH build prints no readiness marker, so it cannot auto-start. Set a fixed port (e.g. 3081) in Advanced, then save and launch again.",
    "wizard.error.probe": "Port probe is unavailable.",
    "wizard.error.fixFields": "Fix the highlighted fields before reviewing.",
    "wizard.error.validation": "The environment failed validation — see the issues below.",
    "wizard.error.validationBackend": "Environment validation backend is unavailable.",
    "wizard.error.save": "The environment could not be saved or launched.",

  },
};

export function resolveLang(value: string | null): Lang {
  return value === "en" ? "en" : "zh";
}

export function loadLang(): Lang {
  try {
    if (typeof window !== "undefined" && window.localStorage) {
      return resolveLang(window.localStorage.getItem(LANG_STORAGE_KEY));
    }
  } catch {
    // Storage unavailable (SSR, privacy mode): keep the default.
  }
  return defaultLang;
}

export function persistLang(lang: Lang): void {
  try {
    if (typeof window !== "undefined" && window.localStorage) {
      window.localStorage.setItem(LANG_STORAGE_KEY, lang);
    }
  } catch {
    // Best effort; the in-memory choice still applies for this session.
  }
}

export interface TranslateParams {
  [name: string]: string;
}

/**
 * Look up a key in the active locale, falling back to zh, then to the key
 * itself so a missing key renders as a visible placeholder instead of
 * throwing. {name} placeholders are replaced from params.
 */
export function translate(
  lang: Lang,
  key: string,
  params?: TranslateParams,
): string {
  const table = translations[lang];
  const value =
    (table && table[key]) ||
    (lang !== defaultLang ? translations[defaultLang][key] : undefined) ||
    key;
  if (!params) return value;
  return value.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? params[name] : match,
  );
}

export interface I18nContextValue {
  lang: Lang;
  t(key: string, params?: TranslateParams): string;
  setLang(lang: Lang): void;
}

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(loadLang);

  const setLang = useCallback((next: Lang) => {
    setLangState(next);
    persistLang(next);
  }, []);

  const t = useCallback(
    (key: string, params?: TranslateParams) => translate(lang, key, params),
    [lang],
  );

  const value = useMemo(() => ({ lang, t, setLang }), [lang, setLang, t]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

/**
 * Components may render without a provider (leaf-panel unit tests): they
 * then see the default locale with a no-op setLang instead of throwing.
 */
export function useI18n(): I18nContextValue {
  const context = useContext(I18nContext);
  if (context) return context;
  return {
    lang: defaultLang,
    t: (key, params) => translate(defaultLang, key, params),
    setLang: () => undefined,
  };
}
