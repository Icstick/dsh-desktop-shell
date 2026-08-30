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
 * The zh locale intentionally preserves the exact copy the shell shipped
 * with today, so the default rendering is unchanged; en holds the English
 * equivalents. All copy is currently English, so both locales resolve to
 * the same strings (except typographic details such as rail.timer) — the
 * mechanism, switch and persistence are complete and the table is ready
 * for real zh copy.
 */

export type Lang = "zh" | "en";

export const LANG_STORAGE_KEY = "dsh-lang";

export const defaultLang: Lang = "zh";

type TranslationKey = string;

export const translations: Record<Lang, Record<TranslationKey, string>> = {
  zh: {
    // Rail
    "rail.dsh": "DSH",
    "rail.browser": "Browser",
    "rail.terminal": "Terminal",
    "rail.notifications": "Notifications",
    "rail.usage": "Usage",
    "rail.timer": "Timer（M3）",
    "rail.runtime": "Runtime",
    "rail.settings": "Settings",
    "rail.aria.surfaces": "Desktop surfaces",
    "rail.aria.brand": "DSH Desktop Shell",
    "lang.label": "Language",

    // Shell header
    "shell.eyebrow": "DSH Desktop Shell",
    "surface.dsh": "DSH Surface",
    "surface.browser": "Browser",
    "surface.terminal": "Persistent Terminal",
    "surface.runtime": "Runtime",
    "surface.settings": "Environment Settings",
    "surface.notifications": "Notifications",
    "surface.usage": "Usage",

    // Common
    "common.ok": "OK",
    "common.cancel": "Cancel",
    "common.close": "Close",
    "common.refresh": "Refresh",
    "common.none": "none",
    "common.loading": "loading",
    "common.notSelected": "not selected",

    // Browser panel
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

    // Terminal panel
    "terminal.aria": "Persistent terminal",
    "terminal.fallbackSession": "terminal",
    "terminal.unavailable": "Terminal unavailable: ",

    // Runtime panel
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

    // Diagnostics
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

    // Notifications
    "notifications.eyebrow": "Local-first audit trail (ADR-0016)",
    "notifications.loading": "Loading notifications…",
    "notifications.empty": "No notifications yet.",
    "notifications.deduplicated": "deduplicated",
    "notifications.dismiss": "Dismiss",
    "notifications.note":
      "Content follows the notification policy (ADR-0016): only explicit_body notifications carry a body, and every notification is recorded in the local AppData audit trail.",

    // Usage
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

    // Command error fallbacks (shown in callouts when the backend error has no message)
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
