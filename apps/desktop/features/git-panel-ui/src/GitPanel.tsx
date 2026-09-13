import { useCallback, useEffect, useMemo, useState } from "react";

import type { DesktopApi } from "../../../src/desktop-api";
import type {
  GitBranchesReport,
  GitDiffReport,
  GitLogReport,
  GitMutationReport,
  GitStatusEntry,
  GitStatusReport,
} from "../../../src/contracts";
import { useI18n } from "../../../src/i18n";

interface GitPanelProps {
  api: DesktopApi;
}

/** The three facets the status list is grouped by. */
type Group = "staged" | "unstaged" | "untracked";

interface ChangeRow {
  key: string;
  path: string;
  group: Group;
  /** The porcelain column that explains this facet. */
  marker: string;
}

/**
 * Rendering cap: the backend already caps a diff at 512 KiB, which can still be
 * tens of thousands of lines. We stop drawing before the DOM becomes the
 * bottleneck and say so instead of silently shortening the file.
 */
const MAX_DIFF_LINES = 4000;

/** How many commits the history section asks for. */
const LOG_LIMIT = 50;

function errorCode(error: unknown): string | null {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    return typeof code === "string" ? code : null;
  }
  return null;
}

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message?: unknown }).message);
  }
  return String(error);
}

/**
 * Split the status entries into the three facets. A partially staged file is
 * genuinely in two of them at once (porcelain reports it as \`MM\`), so it is
 * listed twice rather than silently folded into one bucket.
 */
function changeRows(entries: GitStatusEntry[]): ChangeRow[] {
  const rows: ChangeRow[] = [];
  for (const entry of entries) {
    if (entry.untracked) {
      rows.push({ key: "untracked:" + entry.path, path: entry.path, group: "untracked", marker: "?" });
      continue;
    }
    if (entry.staged) {
      rows.push({
        key: "staged:" + entry.path,
        path: entry.path,
        group: "staged",
        marker: entry.indexStatus.trim() || "M",
      });
    }
    if (entry.unstaged) {
      rows.push({
        key: "unstaged:" + entry.path,
        path: entry.path,
        group: "unstaged",
        marker: entry.worktreeStatus.trim() || "M",
      });
    }
  }
  return rows;
}

type DiffLineKind = "meta" | "hunk" | "added" | "removed" | "context";

function diffLineKind(line: string): DiffLineKind {
  if (line.startsWith("+++") || line.startsWith("---")) return "meta";
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+")) return "added";
  if (line.startsWith("-")) return "removed";
  return "context";
}

const GROUPS: Array<{ group: Group; labelKey: string }> = [
  { group: "staged", labelKey: "git.group.staged" },
  { group: "unstaged", labelKey: "git.group.unstaged" },
  { group: "untracked", labelKey: "git.group.untracked" },
];

/**
 * GIT-M1 read + GIT-M2 write, per docs/roadmap/PLAN-GIT-M1.md and
 * PLAN-GIT-M2.md and the workbench visual spec. The repository is always the
 * environment-linked root - this surface never names one - so every call is
 * root-scoped and carries only a repo-relative path.
 *
 * The mutating verbs follow one rule: nothing that can lose work happens on a
 * single click. Staging and committing are ordinary buttons; discarding an
 * edit is the one that asks the user to type a word first.
 */
export function GitPanel({ api }: GitPanelProps) {
  const { t } = useI18n();
  const [status, setStatus] = useState<GitStatusReport | null>(null);
  const [log, setLog] = useState<GitLogReport | null>(null);
  const [branches, setBranches] = useState<GitBranchesReport | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [failure, setFailure] = useState<{ code: string | null; message: string } | null>(null);
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<{ path: string; group: Group } | null>(null);
  const [diff, setDiff] = useState<GitDiffReport | null>(null);
  const [diffFailure, setDiffFailure] = useState<string | null>(null);
  const [indexScope, setIndexScope] = useState(false);
  const [showLog, setShowLog] = useState(false);
  const [showBranches, setShowBranches] = useState(false);
  const [pending, setPending] = useState(false);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [commitMessage, setCommitMessage] = useState("");
  const [discardTarget, setDiscardTarget] = useState<string | null>(null);
  const [discardWord, setDiscardWord] = useState("");

  const refresh = useCallback(async () => {
    try {
      // One shot for the whole repository: a failure here is a property of the
      // repository (missing, not a repo, git absent), so it belongs to all three.
      const [nextStatus, nextLog, nextBranches] = await Promise.all([
        api.gitStatus({ schemaVersion: 1 }),
        api.gitLog({ schemaVersion: 1, limit: LOG_LIMIT }),
        api.gitBranches({ schemaVersion: 1 }),
      ]);
      setStatus(nextStatus);
      setLog(nextLog);
      setBranches(nextBranches);
      setFailure(null);
    } catch (cause) {
      setStatus(null);
      setLog(null);
      setBranches(null);
      setFailure({ code: errorCode(cause), message: errorText(cause) });
    } finally {
      setLoaded(true);
    }
  }, [api]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const loadDiff = useCallback(
    async (path: string, staged: boolean) => {
      try {
        const report = await api.gitDiff({ schemaVersion: 1, path, staged });
        setDiff(report);
        setDiffFailure(null);
      } catch (cause) {
        setDiff(null);
        setDiffFailure(errorText(cause));
      }
    },
    [api],
  );

  /**
   * Run one mutation and fold its answer back in. Every mutation reports the
   * status it produced, so the list updates from the response instead of a
   * second round trip; only a commit has to re-read the history.
   */
  const applyMutation = useCallback(
    async (run: () => Promise<GitMutationReport>): Promise<boolean> => {
      setPending(true);
      try {
        const report = await run();
        setStatus(report.status);
        setMutationError(null);
        if (report.operation === "commit") {
          const [nextLog, nextBranches] = await Promise.all([
            api.gitLog({ schemaVersion: 1, limit: LOG_LIMIT }),
            api.gitBranches({ schemaVersion: 1 }),
          ]);
          setLog(nextLog);
          setBranches(nextBranches);
        }
        return true;
      } catch (cause) {
        setMutationError(errorText(cause));
        return false;
      } finally {
        setPending(false);
      }
    },
    [api],
  );

  const stage = (path: string) => void applyMutation(() => api.gitStage({ schemaVersion: 1, path }));
  const stageAll = () => void applyMutation(() => api.gitStage({ schemaVersion: 1, all: true }));
  const unstage = (path: string) =>
    void applyMutation(() => api.gitUnstage({ schemaVersion: 1, path }));
  const unstageAll = () =>
    void applyMutation(() => api.gitUnstage({ schemaVersion: 1, all: true }));

  const commit = () => {
    const message = commitMessage.trim();
    if (message === "" || stagedCount === 0) return;
    void applyMutation(async () => {
      const report = await api.gitCommit({ schemaVersion: 1, message });
      setCommitMessage("");
      return report;
    });
  };

  const discard = async () => {
    const path = discardTarget;
    if (path === null) return;
    const ok = await applyMutation(() => api.gitDiscard({ schemaVersion: 1, path }));
    if (ok) {
      setDiscardTarget(null);
      setDiscardWord("");
    }
  };

  const rows = useMemo(() => changeRows(status?.entries ?? []), [status]);
  const stagedCount = rows.filter((row) => row.group === "staged").length;
  const canCommit = stagedCount > 0 && commitMessage.trim() !== "";

  const selectedKey = selected ? selected.group + ":" + selected.path : null;
  const selectedRow = rows.find((row) => row.key === selectedKey) ?? null;

  const needle = filter.trim().toLowerCase();
  const visible = needle ? rows.filter((row) => row.path.toLowerCase().includes(needle)) : rows;

  const select = (row: ChangeRow) => {
    setSelected({ path: row.path, group: row.group });
    setIndexScope(row.group === "staged");
    void loadDiff(row.path, row.group === "staged");
  };

  const setScope = (staged: boolean) => {
    setIndexScope(staged);
    if (selected) void loadDiff(selected.path, staged);
  };

  const diffLines = useMemo(() => {
    if (!diff || diff.text === "") return [];
    return diff.text.replace(/\n$/, "").split("\n");
  }, [diff]);

  const shownLines = diffLines.slice(0, MAX_DIFF_LINES);
  const branchLabel = status?.detached
    ? t("git.detached")
    : (status?.branch ?? t("git.noBranch"));

  return (
    <section className="panel git-panel" aria-label={t("surface.workbench")}>
      <div className="git-panel__toolbar" data-testid="git-toolbar">
        <button className="git-panel__action" onClick={() => void refresh()} type="button">
          {t("git.toolbar.refresh")}
        </button>
        <span className="file-manager__meta file-manager__hint">{t("git.toolbar.hint")}</span>
      </div>

      <div className="git-panel__docbar" data-testid="git-docbar">
        <code>{status ? status.root : "—"}</code>
        <span className="file-manager__chips">
          <span className="file-manager__chip" data-testid="git-branch" data-state={status?.detached ? "warn" : "normal"}>
            {branchLabel}
          </span>
        </span>
      </div>

      {mutationError !== null && (
        <p className="git-panel__error" data-testid="git-mutation-error" role="alert">
          {mutationError}
        </p>
      )}

      {discardTarget !== null && (
        <div
          aria-label={t("git.discard.title")}
          className="git-panel__dialog"
          data-testid="git-discard-dialog"
          role="alertdialog"
        >
          <strong>{t("git.discard.title")}</strong>
          <p>
            <code>{discardTarget}</code>
          </p>
          <p>{t("git.discard.body")}</p>
          <label className="git-panel__confirm">
            {t("git.discard.typeHint").replace("{word}", t("git.discard.word"))}
            <input
              data-testid="git-discard-word"
              onChange={(event) => setDiscardWord(event.target.value)}
              value={discardWord}
            />
          </label>
          <div className="git-panel__dialog-actions">
            <button
              className="git-panel__action git-panel__action--danger"
              data-testid="git-discard-confirm"
              disabled={pending || discardWord.trim() !== t("git.discard.word")}
              onClick={() => void discard()}
              type="button"
            >
              {t("git.discard.confirm")}
            </button>
            <button
              className="git-panel__action"
              onClick={() => {
                setDiscardTarget(null);
                setDiscardWord("");
              }}
              type="button"
            >
              {t("git.discard.cancel")}
            </button>
          </div>
        </div>
      )}

      {failure && (
        <div className="git-panel__error" data-testid="git-error" role="alert">
          <strong>
            {failure.code === "UNAVAILABLE" ? t("git.unavailable.title") : t("git.error.title")}
          </strong>
          <p>{failure.message}</p>
        </div>
      )}

      <div className="git-panel__body">
        <div className="git-panel__changes">
          <input
            aria-label={t("git.filter.placeholder")}
            className="git-panel__filter"
            data-testid="git-filter"
            onChange={(event) => setFilter(event.target.value)}
            placeholder={t("git.filter.placeholder")}
            value={filter}
          />
          <div className="git-panel__section-header">
            <span>{t("git.changes.title")}</span>
            <span className="git-panel__section-actions">
              {needle !== "" && (
                <span className="file-manager__meta" data-testid="git-filter-count">
                  {t("git.filter.count")
                    .replace("{shown}", String(visible.length))
                    .replace("{total}", String(rows.length))}
                </span>
              )}
              <button
                className="git-panel__action"
                data-testid="git-stage-all"
                disabled={pending || rows.length === 0}
                onClick={stageAll}
                type="button"
              >
                {t("git.action.stageAll")}
              </button>
              <button
                className="git-panel__action"
                data-testid="git-unstage-all"
                disabled={pending || stagedCount === 0}
                onClick={unstageAll}
                type="button"
              >
                {t("git.action.unstageAll")}
              </button>
            </span>
          </div>

          {!loaded && !failure && <p className="panel__note">{t("git.loading")}</p>}
          {loaded && !failure && rows.length === 0 && (
            <p className="panel__note" data-testid="git-clean">
              {t("git.clean")}
            </p>
          )}

          {status !== null &&
            GROUPS.map(({ group, labelKey }) => {
              const groupRows = visible.filter((row) => row.group === group);
              return (
                <div className="git-panel__group" key={group}>
                  <div className="git-panel__section-header">
                    <span>{t(labelKey)}</span>
                    <span className="file-manager__meta" data-testid={"git-count-" + group}>
                      {groupRows.length}
                    </span>
                  </div>
                  <ul className="git-panel__rows">
                    {groupRows.map((row) => (
                      <li className="git-panel__row-item" key={row.key}>
                        <button
                          aria-current={row.key === selectedKey}
                          className="git-panel__row"
                          onClick={() => select(row)}
                          type="button"
                        >
                          <span aria-hidden="true" className="git-panel__marker" data-kind={row.group}>
                            {row.marker}
                          </span>
                          <span className="git-panel__path">{row.path}</span>
                        </button>
                        <span className="git-panel__row-actions">
                          {row.group === "staged" ? (
                            <button
                              aria-label={t("git.action.unstage") + " " + row.path}
                              className="git-panel__action"
                              disabled={pending}
                              onClick={() => unstage(row.path)}
                              type="button"
                            >
                              {t("git.action.unstage")}
                            </button>
                          ) : (
                            <button
                              aria-label={t("git.action.stage") + " " + row.path}
                              className="git-panel__action"
                              disabled={pending}
                              onClick={() => stage(row.path)}
                              type="button"
                            >
                              {t("git.action.stage")}
                            </button>
                          )}
                          {row.group !== "untracked" && (
                            <button
                              aria-label={t("git.action.discard") + " " + row.path}
                              className="git-panel__action git-panel__action--danger"
                              disabled={pending}
                              onClick={() => {
                                setDiscardTarget(row.path);
                                setDiscardWord("");
                              }}
                              type="button"
                            >
                              {t("git.action.discard")}
                            </button>
                          )}
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
              );
            })}

          {needle !== "" && visible.length === 0 && (
            <p className="panel__note" data-testid="git-filter-empty">
              {t("git.filter.empty")}
            </p>
          )}

          {status?.truncated && <p className="panel__note">{t("git.truncated.status")}</p>}

          <div className="git-panel__group">
            <div className="git-panel__section-header">
              <button
                aria-expanded={showLog}
                className="git-panel__disclosure"
                onClick={() => setShowLog((current) => !current)}
                type="button"
              >
                <span aria-hidden="true">{showLog ? "▾" : "▸"}</span> {t("git.group.recent")}
              </button>
              <span className="file-manager__meta">{log ? log.entries.length : 0}</span>
            </div>
            {showLog && (
              <div data-testid="git-log">
                {log === null || log.entries.length === 0 ? (
                  <p className="panel__note">{t("git.log.empty")}</p>
                ) : (
                  <ul className="git-panel__rows">
                    {log.entries.map((entry) => (
                      <li className="git-panel__log-row" key={entry.hash}>
                        <code>{entry.hash.slice(0, 8)}</code> {entry.subject}
                        <span className="file-manager__meta">{entry.author}</span>
                      </li>
                    ))}
                  </ul>
                )}
                {log?.truncated && <p className="panel__note">{t("git.truncated.log")}</p>}
              </div>
            )}
          </div>

          <div className="git-panel__group">
            <div className="git-panel__section-header">
              <button
                aria-expanded={showBranches}
                className="git-panel__disclosure"
                onClick={() => setShowBranches((current) => !current)}
                type="button"
              >
                <span aria-hidden="true">{showBranches ? "▾" : "▸"}</span> {t("git.group.branches")}
              </button>
              <span className="file-manager__meta">{branches ? branches.branches.length : 0}</span>
            </div>
            {showBranches && (
              <div data-testid="git-branches">
                {branches === null || branches.branches.length === 0 ? (
                  <p className="panel__note">{t("git.branches.empty")}</p>
                ) : (
                  <ul className="git-panel__rows">
                    {branches.branches.map((name) => (
                      <li
                        aria-current={name === branches.current}
                        className="git-panel__branch-row"
                        key={name}
                      >
                        {name}
                        {name === branches.current && (
                          <span className="file-manager__meta">{t("git.branch.current")}</span>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}
          </div>
        </div>

        <div className="git-panel__view">
          <div className="git-panel__diff-body">
          {selectedRow === null ? (
            <p className="panel__note git-panel__empty" data-testid="git-diff-select">
              {t("git.diff.select")}
            </p>
          ) : (
            <>
              <div className="git-panel__diff-header">
                <code>{selectedRow.path}</code>
                <span className="file-manager__chips">
                  <span className="file-manager__chip" data-testid="git-scope">
                    {t("git.diff.scope." + (diff ? diff.scope : indexScope ? "staged" : "worktree"))}
                  </span>
                  {diff && (
                    <>
                      <span className="file-manager__chip" data-state="ok" data-testid="git-additions">
                        {t("git.diff.additions").replace("{count}", String(diff.additions))}
                      </span>
                      <span className="file-manager__chip" data-state="bad" data-testid="git-deletions">
                        {t("git.diff.deletions").replace("{count}", String(diff.deletions))}
                      </span>
                    </>
                  )}
                  <label className="file-manager__toggle">
                    <input
                      checked={indexScope}
                      data-testid="git-scope-toggle"
                      onChange={(event) => setScope(event.target.checked)}
                      type="checkbox"
                    />
                    {t("git.diff.toggle.staged")}
                  </label>
                </span>
              </div>

              {diffFailure !== null && (
                <p className="git-panel__error" data-testid="git-diff-error" role="alert">
                  {diffFailure}
                </p>
              )}

              {diffFailure === null && shownLines.length === 0 && (
                <p className="panel__note git-panel__empty" data-testid="git-diff-empty">
                  {selectedRow.group === "untracked" ? t("git.diff.untracked") : t("git.diff.empty")}
                </p>
              )}

              {shownLines.length > 0 && (
                <pre className="git-panel__diff" data-testid="git-diff">
                  {shownLines.map((line, index) => (
                    <span
                      className="git-panel__diff-line"
                      data-kind={diffLineKind(line)}
                      key={index}
                    >
                      {line}
                      {"\n"}
                    </span>
                  ))}
                </pre>
              )}

              {diff?.truncated && <p className="panel__note">{t("git.diff.truncated")}</p>}
              {diffLines.length > shownLines.length && (
                <p className="panel__note">{t("git.diff.linesCapped")}</p>
              )}
            </>
          )}
          </div>

          {/* The commit box belongs to the index, not to the selected file, so it
              stays put while the diff above scrolls. */}
          <div className="git-panel__commit" data-testid="git-commit-box">
            <textarea
              aria-label={t("git.commit.placeholder")}
              className="git-panel__commit-message"
              data-testid="git-commit-message"
              onChange={(event) => setCommitMessage(event.target.value)}
              onKeyDown={(event) => {
                if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                  event.preventDefault();
                  commit();
                }
              }}
              placeholder={t("git.commit.placeholder")}
              spellCheck={false}
              value={commitMessage}
            />
            <div className="git-panel__commit-row">
              <span className="file-manager__meta" data-testid="git-commit-staged">
                {t("git.commit.staged").replace("{count}", String(stagedCount))}
              </span>
              <span className="file-manager__meta">
                {stagedCount === 0
                  ? t("git.commit.nothingStaged")
                  : commitMessage.trim() === ""
                    ? t("git.commit.emptyMessage")
                    : t("git.commit.hint")}
              </span>
              <button
                className="git-panel__action git-panel__action--primary"
                data-testid="git-commit"
                disabled={pending || !canCommit}
                onClick={commit}
                type="button"
              >
                {pending ? t("git.action.working") : t("git.commit.action")}
              </button>
            </div>
          </div>
        </div>
      </div>

      <div className="git-panel__statusbar" data-testid="git-status">
        <span className="file-manager__meta" data-testid="git-status-root">
          {status ? status.root : "—"}
        </span>
        <span className="file-manager__meta" data-testid="git-status-count">
          {rows.length === 0 && loaded && !failure
            ? t("git.clean")
            : t("git.statusbar.changes").replace("{count}", String(rows.length))}
        </span>
      </div>
    </section>
  );
}
