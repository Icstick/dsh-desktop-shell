import { useEffect, useMemo, useRef, useState } from "react";

import type { BrowserEvent, BrowserReport } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { useI18n } from "../../../src/i18n";

interface BrowserPanelProps {
  api: DesktopApi;
}

interface BrowserTab {
  report: BrowserReport;
  /** Document title from title_changed events (host window state). */
  title: string | null;
}

/**
 * Human browser surface (MOD-BROWSER-UI, ADR-0017) with multi-session
 * tabs (WI-M9-BROWSER-TABS).
 *
 * Each backend session owns an isolated WebView window; this panel is a
 * tab strip + navigation console over every live session. Opening a URL
 * navigates the ACTIVE tab only, so an agent driving one page never
 * displaces a human researching in another tab. Titles arrive on
 * browser://event (title_changed); after a remount the tab labels fall
 * back to the URL host until the next title event.
 *
 * Outside Tauri (unit tests, plain browsers) the event bridge is absent
 * and the panel stays silent instead of rejecting — same degradation as
 * TerminalPanel.
 */
export function BrowserPanel({ api }: BrowserPanelProps) {
  const { t } = useI18n();
  const [tabs, setTabs] = useState<BrowserTab[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [urlInput, setUrlInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const tabsRef = useRef<BrowserTab[]>(tabs);
  tabsRef.current = tabs;
  const activeIdRef = useRef(activeId);
  activeIdRef.current = activeId;
  const urlInputRef = useRef<HTMLInputElement | null>(null);
  const tRef = useRef(t);
  tRef.current = t;

  const active = useMemo(
    () => tabs.find((tab) => tab.report.sessionId === activeId) ?? null,
    [tabs, activeId],
  );

  // Recover every live backend session when the panel remounts (surface
  // switch): each session becomes a tab; the first is activated.
  useEffect(() => {
    let current = true;
    api
      .listBrowsers()
      .then((reports) => {
        if (!current || tabsRef.current.length > 0) return;
        const live = reports.filter((report) => report.state !== "closed");
        if (live.length === 0) return;
        setTabs(live.map((report) => ({ report, title: null })));
        const first = live[0];
        setActiveId(first.sessionId);
        if (first.currentUrl) setUrlInput(first.currentUrl);
      })
      .catch(() => {
        // Backend unavailable: the first Open surfaces the command error.
      });
    return () => {
      current = false;
    };
  }, [api]);

  // Live push channel: browser://event. Events route per sessionId so one
  // tab's navigation never mutates another tab.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<BrowserEvent>("browser://event", (event) => {
          const payload = event.payload;
          const sessionId = payload.sessionId;
          if (payload.kind === "navigation_changed") {
            setTabs((current) =>
              current.map((tab) =>
                tab.report.sessionId === sessionId
                  ? {
                      ...tab,
                      report: {
                        ...tab.report,
                        state: "ready",
                        currentUrl: payload.url,
                        lastActivityUnixMs: payload.occurredAtUnixMs,
                        error: null,
                      },
                    }
                  : tab,
              ),
            );
            // Follow the active page in the URL bar, but never fight the
            // user's typing.
            const input = urlInputRef.current;
            if (
              sessionId === activeIdRef.current &&
              payload.url &&
              input &&
              document.activeElement !== input
            ) {
              setUrlInput(payload.url);
            }
          } else if (payload.kind === "title_changed") {
            setTabs((current) =>
              current.map((tab) =>
                tab.report.sessionId === sessionId
                  ? { ...tab, title: payload.title }
                  : tab,
              ),
            );
          } else if (payload.kind === "load_failed") {
            const message = `${tRef.current("browser.pageLoadFailed")}${
              payload.url ? `: ${payload.url}` : ""
            }.`;
            setTabs((current) =>
              current.map((tab) =>
                tab.report.sessionId === sessionId
                  ? {
                      ...tab,
                      report: {
                        ...tab.report,
                        state: "error",
                        lastActivityUnixMs: payload.occurredAtUnixMs,
                        error: message,
                      },
                    }
                  : tab,
              ),
            );
            setError(message);
          } else if (payload.kind === "closed") {
            const next = tabsRef.current.filter(
              (tab) => tab.report.sessionId !== sessionId,
            );
            setTabs(next);
            if (sessionId === activeIdRef.current) {
              const index = tabsRef.current.findIndex(
                (tab) => tab.report.sessionId === sessionId,
              );
              const nextActive =
                next[index]?.report.sessionId ??
                next[index - 1]?.report.sessionId ??
                null;
              setActiveId(nextActive);
              const tab = next.find(
                (candidate) => candidate.report.sessionId === nextActive,
              );
              if (tab?.report.currentUrl) setUrlInput(tab.report.currentUrl);
            }
          }
        }),
      )
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // No Tauri event bridge in this environment; stay silent.
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [api]);

  /** Create a fresh session (isolated WebView window) and activate it. */
  const createTab = async (): Promise<string | null> => {
    if (busy) return null;
    setBusy(true);
    setError(null);
    try {
      const created = await api.createBrowser({
        schemaVersion: 1,
        mode: "human_surface",
      });
      const next = [...tabsRef.current, { report: created, title: null }];
      setTabs(next);
      setActiveId(created.sessionId);
      setUrlInput("");
      return created.sessionId;
    } catch (cause) {
      setError(errorMessage(cause, t("browser.error.navigation")));
      return null;
    } finally {
      setBusy(false);
    }
  };

  /**
   * Open a URL. A freshly created tab that never navigated is filled in
   * place; otherwise the URL opens in a NEW tab so existing pages are
   * never displaced (user report: an agent driving one page must not be
   * bumped when the human opens another site).
   */
  const open = async () => {
    const raw = urlInput.trim();
    if (!raw || busy) return;
    const url = normalizeUrl(raw);
    setBusy(true);
    setError(null);
    try {
      const activeTab = tabsRef.current.find(
        (tab) => tab.report.sessionId === activeIdRef.current,
      );
      const targetId =
        activeTab && !activeTab.report.currentUrl
          ? activeTab.report.sessionId
          : null;
      let sessionId: string;
      if (targetId) {
        sessionId = targetId;
      } else {
        const created = await api.createBrowser({
          schemaVersion: 1,
          mode: "human_surface",
        });
        sessionId = created.sessionId;
        setTabs((current) => [...current, { report: created, title: null }]);
        setActiveId(created.sessionId);
      }
      const report = await api.navigateBrowser({
        schemaVersion: 1,
        sessionId,
        url,
      });
      setTabs((current) =>
        current.map((tab) =>
          tab.report.sessionId === sessionId ? { ...tab, report } : tab,
        ),
      );
      if (report.currentUrl) setUrlInput(report.currentUrl);
    } catch (cause) {
      setError(errorMessage(cause, t("browser.error.navigation")));
    } finally {
      setBusy(false);
    }
  };

  const reload = async () => {
    const current = active;
    if (!current?.report.currentUrl || busy) return;
    setBusy(true);
    setError(null);
    try {
      const report = await api.navigateBrowser({
        schemaVersion: 1,
        sessionId: current.report.sessionId,
        url: current.report.currentUrl,
      });
      setTabs((tabs) =>
        tabs.map((tab) =>
          tab.report.sessionId === report.sessionId ? { ...tab, report } : tab,
        ),
      );
    } catch (cause) {
      setError(errorMessage(cause, t("browser.error.reload")));
    } finally {
      setBusy(false);
    }
  };

  const activateTab = (sessionId: string) => {
    setActiveId(sessionId);
    const tab = tabsRef.current.find(
      (candidate) => candidate.report.sessionId === sessionId,
    );
    if (tab?.report.currentUrl) {
      const input = urlInputRef.current;
      if (!input || document.activeElement !== input) {
        setUrlInput(tab.report.currentUrl);
      }
    }
  };

  const closeTab = async (sessionId: string) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.closeBrowser({ schemaVersion: 1, sessionId });
      const current = tabsRef.current;
      const index = current.findIndex(
        (tab) => tab.report.sessionId === sessionId,
      );
      const next = current.filter(
        (tab) => tab.report.sessionId !== sessionId,
      );
      setTabs(next);
      if (sessionId === activeIdRef.current) {
        const nextActive =
          next[index]?.report.sessionId ??
          next[index - 1]?.report.sessionId ??
          null;
        setActiveId(nextActive);
        const tab = next.find(
          (candidate) => candidate.report.sessionId === nextActive,
        );
        if (tab?.report.currentUrl) setUrlInput(tab.report.currentUrl);
      }
    } catch (cause) {
      setError(errorMessage(cause, t("browser.error.close")));
    } finally {
      setBusy(false);
    }
  };

  const state = active?.report.state ?? "created";
  const sessionLabel = active ? tabLabel(active, t) : t("browser.noSession");

  return (
    <section className="browser-panel" aria-label={t("browser.aria")}>
      {tabs.length > 0 && (
        <div className="browser-panel__tabs" role="tablist" aria-label={t("browser.tabsLabel")}>
          {tabs.map((tab) => {
            const id = tab.report.sessionId;
            const isActive = id === activeId;
            return (
              <div
                className={"browser-panel__tab" + (isActive ? " is-active" : "")}
                data-active={isActive}
                key={id}
                role="presentation"
              >
                <button
                  aria-current={isActive ? "page" : undefined}
                  aria-selected={isActive}
                  className="browser-panel__tab-select"
                  onClick={() => activateTab(id)}
                  role="tab"
                  title={tab.report.currentUrl ?? id}
                  type="button"
                >
                  {tabLabel(tab, t)}
                </button>
                <button
                  aria-label={t("browser.closeTab")}
                  className="browser-panel__tab-close"
                  disabled={busy}
                  onClick={() => void closeTab(id)}
                  title={t("browser.closeTab")}
                  type="button"
                >
                  ×
                </button>
              </div>
            );
          })}
          <button
            aria-label={t("browser.newTab")}
            className="browser-panel__new-tab"
            disabled={busy}
            onClick={() => void createTab()}
            title={t("browser.newTab")}
            type="button"
          >
            ＋
          </button>
        </div>
      )}

      <div className="browser-panel__chrome">
        <strong className="browser-panel__session">{sessionLabel}</strong>
        <span className="browser-panel__state" data-state={state}>
          {state}
        </span>
      </div>

      <form
        className="browser-panel__form"
        onSubmit={(event) => {
          event.preventDefault();
          void open();
        }}
      >
        <label className="sr-only" htmlFor="browser-url-input">
          {t("browser.urlLabel")}
        </label>
        <input
          autoCapitalize="none"
          autoCorrect="off"
          className="browser-panel__url"
          disabled={busy}
          id="browser-url-input"
          inputMode="url"
          onChange={(event) => setUrlInput(event.target.value)}
          placeholder="https://example.com"
          ref={urlInputRef}
          spellCheck={false}
          type="text"
          value={urlInput}
        />
        <button
          className="primary-button"
          disabled={busy || urlInput.trim() === ""}
          type="submit"
        >
          {t("browser.open")}
        </button>
      </form>

      <div className="button-row browser-panel__actions">
        <button
          className="secondary-button"
          disabled={busy || !active?.report.currentUrl}
          onClick={() => void reload()}
          type="button"
        >
          {t("browser.reload")}
        </button>
        <button
          className="secondary-button"
          disabled={busy || !active}
          onClick={() => active && void closeTab(active.report.sessionId)}
          type="button"
        >
          {t("common.close")}
        </button>
      </div>

      <dl className="definition-grid definition-grid--health">
        <div>
          <dt>{t("browser.session")}</dt>
          <dd className="browser-panel__session-id">
            {active?.report.sessionId ?? t("common.none")}
          </dd>
        </div>
        <div>
          <dt>{t("browser.currentUrl")}</dt>
          <dd className="browser-panel__url-value">
            {active?.report.currentUrl ?? t("common.none")}
          </dd>
        </div>
        <div>
          <dt>{t("browser.state")}</dt>
          <dd>{state}</dd>
        </div>
        <div>
          <dt>{t("browser.error")}</dt>
          <dd>{active?.report.error ?? t("common.none")}</dd>
        </div>
      </dl>

      {error && (
        <div className="callout callout--danger" role="alert">
          {error}
        </div>
      )}
    </section>
  );
}

/** Tab label: document title, else URL host, else the opaque session id. */
function tabLabel(tab: BrowserTab, t: (key: string) => string): string {
  const title = tab.title?.trim();
  if (title) return title;
  const url = tab.report.currentUrl;
  if (url) {
    try {
      const parsed = new URL(url);
      if (parsed.hostname) return parsed.hostname;
    } catch {
      // fall through to the raw URL
    }
    return url;
  }
  return tab.report.sessionId || t("browser.noSession");
}

function normalizeUrl(raw: string): string {
  const trimmed = raw.trim();
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
}

function errorMessage(error: unknown, fallback: string) {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof (error as { message?: unknown }).message === "string"
  ) {
    return (error as { message: string }).message;
  }
  return fallback;
}
