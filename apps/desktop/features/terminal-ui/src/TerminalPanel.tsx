import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

import type { TerminalOutputEvent, TerminalReport } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { useI18n } from "../../../src/i18n";

interface TerminalPanelProps {
  api: DesktopApi;
  session: TerminalReport | null;
  onSession(session: TerminalReport | null): void;
}

/**
 * Persistent terminal surface (MOD-TERMINAL-UI, ADR-0015).
 *
 * The PTY lives in the Desktop backend and is independent of the Managed
 * DSH process tree (AC-PTY-001): DSH restart never closes this surface.
 * Output arrives over the terminal://output Tauri event (AC-TERM-002);
 * input is written through the backend command.
 */
export function TerminalPanel({ api, session, onSession }: TerminalPanelProps) {
  const { t } = useI18n();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sessionRef = useRef<TerminalReport | null>(session);
  sessionRef.current = session;
  const tRef = useRef(t);
  tRef.current = t;
  const [starting, setStarting] = useState(false);
  // The mount effect auto-creates a session only once (first visit). After
  // the user closes the terminal the panel stays mounted (kept hidden by the
  // ShellApp) and offers an explicit Open button instead of reopening on its
  // own.
  const autoCreatedRef = useRef(false);

  const createSession = async () => {
    if (starting || sessionRef.current) return;
    setStarting(true);
    try {
      const report = await api.createTerminal({
        schemaVersion: 1,
        mode: "human_surface",
        cols: terminalRef.current?.cols ?? 80,
        rows: terminalRef.current?.rows ?? 24,
      });
      autoCreatedRef.current = true;
      onSession(report);
    } catch (error: unknown) {
      terminalRef.current?.writeln(
        tRef.current("terminal.unavailable") + String(error),
      );
    } finally {
      setStarting(false);
    }
  };

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const terminal = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      scrollback: 2000,
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(container);
    fit.fit();
    terminalRef.current = terminal;
    fitRef.current = fit;

    const onData = (data: string) => {
      const current = sessionRef.current;
      if (!current) return;
      void api.writeTerminal({ schemaVersion: 1, sessionId: current.sessionId, data });
    };
    const onResize = () => {
      const current = sessionRef.current;
      const dims = terminalRef.current;
      if (!current || !dims) return;
      void api.resizeTerminal({
        schemaVersion: 1,
        sessionId: current.sessionId,
        cols: dims.cols,
        rows: dims.rows,
      });
    };
    terminal.onData(onData);
    const resizeObserver = new ResizeObserver(() => fitRef.current?.fit());
    resizeObserver.observe(container);
    window.addEventListener("resize", onResize);

    // First mount with no live session: auto-create (first visit UX).
    if (!sessionRef.current && !autoCreatedRef.current) {
      void createSession();
    }

    // Output stream: terminal://output events emitted by the backend.
    const unlistenPromise = import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<TerminalOutputEvent>("terminal://output", (event) => {
          const current = sessionRef.current;
          if (!current || event.payload.sessionId !== current.sessionId) return;
          terminal.write(event.payload.data);
        }),
      )
      // Outside Tauri (unit tests, browsers) there is no IPC bridge; the
      // panel simply stays silent instead of rejecting.
      .catch(() => () => {});

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", onResize);
      terminal.dispose();
      terminalRef.current = null;
      void unlistenPromise.then((unlisten) => unlisten());
    };
    // The effect binds one terminal lifetime per mounted panel.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <section className="terminal-panel" aria-label={t("terminal.aria")}>
      <div className="terminal-panel__chrome">
        <strong>{session ? session.sessionId : t("terminal.fallbackSession")}</strong>
        <span className="terminal-panel__state" data-state={session?.state ?? "created"}>
          {session?.state ?? "created"}
        </span>
        {session ? (
          <button
            className="secondary-button"
            onClick={() => {
              const current = sessionRef.current;
              if (!current) return;
              void api.closeTerminal({ schemaVersion: 1, sessionId: current.sessionId });
              onSession(null);
            }}
            type="button"
          >
            {t("common.close")}
          </button>
        ) : (
          <button
            className="primary-button"
            disabled={starting}
            onClick={() => void createSession()}
            type="button"
            data-testid="terminal-open"
          >
            {t("terminal.open")}
          </button>
        )}
      </div>
      <div className="terminal-panel__host" ref={containerRef} />
    </section>
  );
}