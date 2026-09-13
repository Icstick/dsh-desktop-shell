import { useRef, useState } from "react";

import type { DesktopApi } from "../../../src/desktop-api";
import { FileManagerPanel } from "../../file-manager-ui/src/FileManagerPanel";
import { GitPanel } from "../../git-panel-ui/src/GitPanel";
import { useI18n } from "../../../src/i18n";

interface WorkbenchPanelProps {
  api: DesktopApi;
}

type WorkbenchTab = "files" | "git";

const TABS: Array<{ id: WorkbenchTab; labelKey: string }> = [
  { id: "files", labelKey: "workbench.tab.files" },
  { id: "git", labelKey: "workbench.tab.git" },
];

/**
 * The workbench surface: a Files | Git tab strip over one body
 * (docs/roadmap/PLAN-GIT-M1.md). Only the active tab is mounted, so switching
 * to Git stops the file tree from polling and vice versa; the tab strip itself
 * is a real roving-focus tablist rather than two toggle buttons.
 */
export function WorkbenchPanel({ api }: WorkbenchPanelProps) {
  const { t } = useI18n();
  const [tab, setTab] = useState<WorkbenchTab>("files");
  const buttons = useRef<Array<HTMLButtonElement | null>>([]);

  const move = (from: number, delta: number) => {
    const next = (from + delta + TABS.length) % TABS.length;
    setTab(TABS[next].id);
    buttons.current[next]?.focus();
  };

  return (
    <section className="workbench" aria-label={t("surface.workbench")}>
      <div aria-label={t("workbench.tabs")} className="workbench__tabs" role="tablist">
        {TABS.map((entry, index) => (
          <button
            aria-controls="workbench-panel"
            aria-selected={tab === entry.id}
            className="workbench__tab"
            data-testid={"wb-tab-" + entry.id}
            id={"workbench-tab-" + entry.id}
            key={entry.id}
            onClick={() => setTab(entry.id)}
            onKeyDown={(event) => {
              if (event.key === "ArrowRight") {
                event.preventDefault();
                move(index, 1);
              } else if (event.key === "ArrowLeft") {
                event.preventDefault();
                move(index, -1);
              } else if (event.key === "Home") {
                event.preventDefault();
                move(index, -index);
              } else if (event.key === "End") {
                event.preventDefault();
                move(index, TABS.length - 1 - index);
              }
            }}
            ref={(node) => {
              buttons.current[index] = node;
            }}
            role="tab"
            tabIndex={tab === entry.id ? 0 : -1}
            type="button"
          >
            {t(entry.labelKey)}
          </button>
        ))}
        <span className="workbench__subtitle" data-testid="wb-subtitle">
          {tab === "files" ? t("fs.subtitle") : t("git.subtitle")}
        </span>
      </div>
      <div
        aria-labelledby={"workbench-tab-" + tab}
        className="workbench__panel"
        id="workbench-panel"
        role="tabpanel"
      >
        {tab === "files" ? <FileManagerPanel api={api} /> : <GitPanel api={api} />}
      </div>
    </section>
  );
}
