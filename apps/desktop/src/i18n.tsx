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
