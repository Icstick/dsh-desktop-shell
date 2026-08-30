import { useEffect, useRef, useState } from "react";

import type { BrowserEvent, BrowserReport } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";

interface BrowserPanelProps {
  api: DesktopApi;
}

/**
 * Minimal human browser surface (MOD-BROWSER-UI, ADR-0017).
 *
 * The WebView lives in the Desktop backend as an independent window
 * (label "browser", isolated profile). This panel only drives navigation
 * through the backend commands and mirrors state from browser://event;
 * the arbitrary page never gets Desktop IPC (AC-BRW-001). Outside Tauri
 * (unit tests, plain browsers) the event bridge is absent and the panel
 * stays silent instead of rejecting — same degradation as TerminalPanel.
 */
export function BrowserPanel({ api }: BrowserPanelProps) {
  const [session, setSession] = useState<BrowserReport | null>(null);
  const [urlInput, setUrlInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sessionRef = useRef<BrowserReport | null>(session);
  sessionRef.current = session;
  const urlInputRef = useRef<HTMLInputElement | null>(null);

  // Recover a live backend session when the panel remounts (surface switch).
  useEffect(() => {
    let current = true;
    api
      .listBrowsers()
      .then((reports) => {
        if (!current || sessionRef.current) return;
        const live = reports.find((report) => report.state !== "closed");
        if (live) {
          sessionRef.current = live;
          setSession(live);
          if (live.currentUrl) setUrlInput(live.currentUrl);
        }
      })
      .catch(() => {
        // Backend unavailable: the first Open surfaces the command error.
      });
    return () => {
      current = false;
    };
  }, [api]);

  // Live push channel: browser://event (navigation_changed / load_failed / closed).
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<BrowserEvent>("browser://event", (event) => {
          const payload = event.payload;
          const current = sessionRef.current;
          if (!current || payload.sessionId !== current.sessionId) return;
          if (payload.kind === "navigation_changed") {
            setSession((current) =>
              current
                ? {
                    ...current,
                    state: "ready",
                    currentUrl: payload.url,
                    lastActivityUnixMs: payload.occurredAtUnixMs,
                    error: null,
                  }
                : current,
            );
            // Follow the page in the URL bar, but never fight the user's typing.
            const input = urlInputRef.current;
            if (payload.url && input && document.activeElement !== input) {
              setUrlInput(payload.url);
            }
          } else if (payload.kind === "load_failed") {
            const message = `Page failed to load${payload.url ? `: ${payload.url}` : ""}.`;
            setSession((current) =>
              current
                ? {
                    ...current,
                    state: "error",
                    lastActivityUnixMs: payload.occurredAtUnixMs,
                    error: message,
                  }
                : current,
            );
            setError(message);
          } else if (payload.kind === "closed") {
            sessionRef.current = null;
            setSession(null);
            setError(null);
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

  const open = async () => {
    const raw = urlInput.trim();
    if (!raw || busy) return;
    const url = normalizeUrl(raw);
    setBusy(true);
    setError(null);
    try {
      if (!sessionRef.current) {
        const created = await api.createBrowser({
          schemaVersion: 1,
          mode: "human_surface",
        });
        sessionRef.current = created;
        setSession(created);
      }
      const report = await api.navigateBrowser({
        schemaVersion: 1,
        sessionId: sessionRef.current.sessionId,
        url,
      });
      sessionRef.current = report;
      setSession(report);
      if (report.currentUrl) setUrlInput(report.currentUrl);
    } catch (cause) {
      setError(errorMessage(cause, "Browser navigation is unavailable."));
    } finally {
      setBusy(false);
    }
  };

  const reload = async () => {
    const current = sessionRef.current;
    if (!current?.currentUrl || busy) return;
    setBusy(true);
    setError(null);
    try {
      const report = await api.navigateBrowser({
        schemaVersion: 1,
        sessionId: current.sessionId,
        url: current.currentUrl,
      });
      sessionRef.current = report;
      setSession(report);
    } catch (cause) {
      setError(errorMessage(cause, "Browser reload is unavailable."));
    } finally {
      setBusy(false);
    }
  };

  const close = async () => {
    const current = sessionRef.current;
    if (!current || busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.closeBrowser({ schemaVersion: 1, sessionId: current.sessionId });
      sessionRef.current = null;
      setSession(null);
    } catch (cause) {
      setError(errorMessage(cause, "Browser close is unavailable."));
    } finally {
      setBusy(false);
    }
  };

  const state = session?.state ?? "created";

  return (
    <section className="browser-panel" aria-label="Browser">
      <div className="browser-panel__chrome">
        <strong className="browser-panel__session">
          {session ? session.sessionId : "no browser session"}
        </strong>
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
          Browser URL
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
          Open
        </button>
      </form>

      <div className="button-row browser-panel__actions">
        <button
          className="secondary-button"
          disabled={busy || !session?.currentUrl}
          onClick={() => void reload()}
          type="button"
        >
          Reload
        </button>
        <button
          className="secondary-button"
          disabled={busy || !session}
          onClick={() => void close()}
          type="button"
        >
          Close
        </button>
      </div>

      <dl className="definition-grid definition-grid--health">
        <div>
          <dt>Session</dt>
          <dd>{session?.sessionId ?? "none"}</dd>
        </div>
        <div>
          <dt>Current URL</dt>
          <dd className="browser-panel__url-value">{session?.currentUrl ?? "none"}</dd>
        </div>
        <div>
          <dt>State</dt>
          <dd>{state}</dd>
        </div>
        <div>
          <dt>Error</dt>
          <dd>{session?.error ?? "none"}</dd>
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
