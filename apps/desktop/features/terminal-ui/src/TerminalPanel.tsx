import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

import type { TerminalOutputEvent, TerminalReport } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";

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
  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sessionRef = useRef<TerminalReport | null>(session);
  sessionRef.current = session;

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

    // Start a session if none exists yet.
    if (!sessionRef.current) {
      void api
        .createTerminal({
          schemaVersion: 1,
          mode: "human_surface",
          cols: terminal.cols,
          rows: terminal.rows,
        })
        .then((report) => onSession(report))
        .catch((error: unknown) => {
          terminal.writeln("Terminal unavailable: " + String(error));
        });
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
    <section className="terminal-panel" aria-label="Persistent terminal">
      <div className="terminal-panel__chrome">
        <strong>{session ? session.sessionId : "terminal"}</strong>
        <span className="terminal-panel__state" data-state={session?.state ?? "created"}>
          {session?.state ?? "created"}
        </span>
        <button
          className="secondary-button"
          disabled={!session}
          onClick={() => {
            const current = sessionRef.current;
            if (!current) return;
            void api.closeTerminal({ schemaVersion: 1, sessionId: current.sessionId });
            onSession(null);
          }}
          type="button"
        >
          Close
        </button>
      </div>
      <div className="terminal-panel__host" ref={containerRef} />
    </section>
  );
}