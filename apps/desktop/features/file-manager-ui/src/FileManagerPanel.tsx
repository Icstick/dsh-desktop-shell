import { useCallback, useEffect, useMemo, useState } from "react";

import type { DesktopApi } from "../../../src/desktop-api";
import type { FsEntry, FsEntryKind, FsFileReport, FsRootsReport, FsStatReport } from "../../../src/contracts";
import { useI18n } from "../../../src/i18n";

interface FileManagerPanelProps {
  api: DesktopApi;
}

/** Lazy listing per "rootId:path" (the Rust side owns containment). */
type Listings = Record<string, { entries: FsEntry[]; truncated: boolean }>;

/** The conflict baseline: what the file looked like when we opened it. */
type Baseline = { size: number; modifiedUnixMs: number };

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
 * FS-M1 read-only surface + FS-M2 editing.
 *
 * Layout follows docs/roadmap/SPEC-WORKBENCH-VISUAL.md: compact toolbar, a
 * filter box on top of the tree, a document bar over the view, and the file's
 * path/encoding/size in a bottom status strip with Save at its right end.
 * Human-only: every call goes through the Shell capability, every path is
 * root-relative, and a containment failure arrives as a malformed request.
 */
export function FileManagerPanel({ api }: FileManagerPanelProps) {
  const { t } = useI18n();
  const [roots, setRoots] = useState<FsRootsReport | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [filter, setFilter] = useState("");
  const [listings, setListings] = useState<Listings>({});
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [file, setFile] = useState<FsFileReport | null>(null);
  const [baseline, setBaseline] = useState<Baseline | null>(null);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [pendingOpen, setPendingOpen] = useState<{ rootId: string; path: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const dirty = file !== null && draft !== file.content;
  const readOnly = file?.readOnly === true;

  const reloadRoots = useCallback(async () => {
    try {
      const report = await api.fsListRoots({ schemaVersion: 1 });
      setRoots(report);
      setError(null);
    } catch (cause) {
      setError(errorText(cause));
    }
  }, [api]);

  useEffect(() => {
    void reloadRoots();
  }, [reloadRoots]);

  const loadFile = useCallback(
    async (rootId: string, path: string) => {
      try {
        const report = await api.fsReadFile({ schemaVersion: 1, rootId, relativePath: path });
        // The read report carries no mtime, so the baseline comes from stat -
        // that pair is exactly what the write path compares before it saves.
        const stat = await api.fsStat({ schemaVersion: 1, rootId, relativePath: path });
        setFile(report);
        setBaseline({ size: stat.size, modifiedUnixMs: stat.modifiedUnixMs });
        setDraft(report.content);
        setConflict(false);
        setError(null);
      } catch (cause) {
        setFile(null);
        setBaseline(null);
        setError(errorText(cause));
      }
    },
    [api],
  );

  const requestOpen = useCallback(
    (rootId: string, path: string) => {
      if (dirty) {
        setPendingOpen({ rootId, path });
        return;
      }
      void loadFile(rootId, path);
    },
    [dirty, loadFile],
  );

  const save = useCallback(
    async (force: boolean) => {
      if (!file || !baseline) return;
      setSaving(true);
      try {
        const stat: FsStatReport = await api.fsWriteFile({
          schemaVersion: 1,
          rootId: file.rootId,
          relativePath: file.path,
          content: draft,
          expectedSize: baseline.size,
          expectedModifiedUnixMs: baseline.modifiedUnixMs,
          force,
        });
        setBaseline({ size: stat.size, modifiedUnixMs: stat.modifiedUnixMs });
        setFile({ ...file, content: draft, size: stat.size });
        setConflict(false);
        setError(null);
      } catch (cause) {
        if (errorCode(cause) === "CONFLICT") {
          setConflict(true);
        } else {
          setError(errorText(cause));
        }
      } finally {
        setSaving(false);
      }
    },
    [api, baseline, draft, file],
  );

  const toggle = async (rootId: string, path: string) => {
    const key = rootId + ":" + path;
    if (open[key]) {
      setOpen((current) => ({ ...current, [key]: false }));
      return;
    }
    try {
      if (!listings[key]) {
        const report = await api.fsReadDir({ schemaVersion: 1, rootId, relativePath: path, showHidden });
        setListings((current) => ({ ...current, [key]: { entries: report.entries, truncated: report.truncated } }));
      }
      setOpen((current) => ({ ...current, [key]: true }));
      setError(null);
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  // Rebuild the visible rows from the loaded listings, then apply the filter.
  const { rows, shown, total } = useMemo(() => {
    const collected: Array<{ key: string; rootId: string; path: string; entry: FsEntry; depth: number }> = [];
    const walk = (rootId: string, path: string, depth: number) => {
      const listing = listings[rootId + ":" + path];
      if (!listing) return;
      for (const entry of listing.entries) {
        const childPath = path ? path + "/" + entry.name : entry.name;
        collected.push({ key: rootId + ":" + childPath, rootId, path: childPath, entry, depth });
        if (entry.kind === "dir" && open[rootId + ":" + childPath]) walk(rootId, childPath, depth + 1);
      }
    };
    for (const root of roots?.roots ?? []) {
      if (root.available && open[root.id + ":"]) walk(root.id, "", 0);
    }
    const needle = filter.trim().toLowerCase();
    const visible = needle ? collected.filter((row) => row.entry.name.toLowerCase().includes(needle)) : collected;
    return { rows: visible, shown: visible.length, total: collected.length };
  }, [filter, listings, open, roots]);

  const kindLabel = (kind: FsEntryKind) => t("fs.kind." + kind);
  const truncated = Object.values(listings).some((listing) => listing.truncated);

  return (
    <section className="panel file-manager" aria-label={t("surface.workbench")}>
      <div className="file-manager__toolbar" data-testid="fm-toolbar">
        <button className="file-manager__action" onClick={() => void reloadRoots()} type="button">
          {t("fs.toolbar.refresh")}
        </button>
        <label className="file-manager__toggle">
          <input
            checked={showHidden}
            onChange={(event) => {
              setShowHidden(event.target.checked);
              setListings({});
              setOpen({});
            }}
            type="checkbox"
          />
          {t("fs.toolbar.hidden")}
        </label>
        <button className="file-manager__action" onClick={() => setOpen({})} type="button">
          {t("fs.toolbar.collapse")}
        </button>
        <span className="file-manager__meta file-manager__hint">{t("fs.toolbar.saveHint")}</span>
      </div>

      {error && (
        <p className="file-manager__error" role="alert">
          {error}
        </p>
      )}

      <div className="file-manager__body">
        <div className="file-manager__tree">
          <input
            aria-label={t("fs.filter.placeholder")}
            className="file-manager__filter"
            data-testid="fm-filter"
            onChange={(event) => setFilter(event.target.value)}
            placeholder={t("fs.filter.placeholder")}
            value={filter}
          />
          <div className="file-manager__tree-header">
            <span>{t("fs.roots")}</span>
            {filter.trim() !== "" && (
              <span className="file-manager__meta" data-testid="fm-filter-count">
                {t("fs.filter.count").replace("{shown}", String(shown)).replace("{total}", String(total))}
              </span>
            )}
          </div>
          {roots === null && !error && <p className="panel__note">{t("fs.loading")}</p>}
          {roots !== null && roots.roots.length === 0 && <p className="panel__note">{t("fs.emptyRoots")}</p>}
          {(roots?.roots ?? []).map((root) =>
            root.available ? (
              <div className="file-manager__root" key={root.id}>
                <button
                  aria-expanded={open[root.id + ":"] === true}
                  className="file-manager__root-label"
                  onClick={() => void toggle(root.id, "")}
                  type="button"
                >
                  {root.label}
                </button>
              </div>
            ) : (
              <div className="file-manager__root" data-available="false" key={root.id}>
                <span className="file-manager__root-label">{root.label}</span>
                <span className="file-manager__root-reason">
                  {t("fs.unavailable")}: {root.reason ?? ""}
                </span>
              </div>
            ),
          )}
          {rows.length === 0 && filter.trim() !== "" && <p className="panel__note">{t("fs.empty")}</p>}
          <ul className="file-manager__rows">
            {rows.map((row) => (
              <li key={row.key} style={{ paddingLeft: 6 + row.depth * 12 }}>
                {row.entry.kind === "dir" ? (
                  <button
                    aria-expanded={open[row.key] === true}
                    className="file-manager__row"
                    onClick={() => void toggle(row.rootId, row.path)}
                    type="button"
                  >
                    <span aria-hidden="true">{open[row.key] ? "▾" : "▸"}</span> {row.entry.name}/
                  </button>
                ) : row.entry.kind === "file" ? (
                  <button
                    className="file-manager__row"
                    onClick={() => requestOpen(row.rootId, row.path)}
                    type="button"
                  >
                    {row.entry.name}
                    <span className="file-manager__meta">{row.entry.size} B</span>
                  </button>
                ) : (
                  <span className="file-manager__row" data-kind="link" title={t("fs.kind.link")}>
                    {row.entry.name} → {kindLabel("link")}
                  </span>
                )}
              </li>
            ))}
          </ul>
          {truncated && <p className="panel__note">{t("fs.truncated")}</p>}
        </div>

        <div className="file-manager__view">
          <div className="file-manager__docbar" data-testid="fm-docbar">
            <code>{file ? file.path : t("fs.selectFile")}</code>
            {file && (
              <span className="file-manager__chips">
                <span className="file-manager__chip" data-state={dirty ? "warn" : "normal"} data-testid="fm-dirty">
                  {dirty ? t("fs.docbar.dirty") : t("fs.docbar.saved")}
                </span>
                <span className="file-manager__chip">{file.encoding}</span>
                <span className="file-manager__chip" data-state={readOnly ? "bad" : "normal"}>
                  {readOnly ? t("fs.chip.readonly") : t("fs.chip.editable")}
                </span>
              </span>
            )}
          </div>

          {file === null ? (
            <p className="panel__note file-manager__empty">{t("fs.selectFile")}</p>
          ) : (
            <>
              {readOnly && (
                <p className="file-manager__banner" role="status">
                  {file.encoding === "binary" ? t("fs.readOnly.binary") : t("fs.readOnly.overLimit")}
                </p>
              )}
              {conflict && (
                <div className="file-manager__dialog" data-testid="fm-conflict" role="alertdialog">
                  <strong>{t("fs.conflict.title")}</strong>
                  <p>{t("fs.conflict.body")}</p>
                  <div className="file-manager__dialog-actions">
                    <button onClick={() => void save(true)} type="button">
                      {t("fs.conflict.overwrite")}
                    </button>
                    <button onClick={() => void loadFile(file.rootId, file.path)} type="button">
                      {t("fs.conflict.discard")}
                    </button>
                    <button onClick={() => setConflict(false)} type="button">
                      {t("fs.conflict.keep")}
                    </button>
                  </div>
                </div>
              )}
              <textarea
                aria-label={file.path}
                className="file-manager__editor"
                data-testid="fm-editor"
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={(event) => {
                  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
                    event.preventDefault();
                    if (!readOnly) void save(false);
                  }
                }}
                readOnly={readOnly}
                spellCheck={false}
                value={draft}
              />
            </>
          )}

          <div className="file-manager__statusbar" data-testid="fm-status">
            <span className="file-manager__meta" data-testid="fm-status-path">
              {file ? file.path : "—"}
            </span>
            <span className="file-manager__meta">
              {file ? file.encoding : ""} {file ? file.size + " B" : ""}
            </span>
            <span className="file-manager__statusbar-actions">
              <button
                className="file-manager__action"
                data-testid="fm-save"
                disabled={!dirty || readOnly || saving}
                onClick={() => void save(false)}
                type="button"
              >
                {saving ? t("fs.saving") : t("fs.save")}
              </button>
            </span>
          </div>
        </div>
      </div>

      {pendingOpen && (
        <div className="file-manager__dialog" data-testid="fm-confirm" role="alertdialog">
          <strong>{t("fs.confirm.title")}</strong>
          <p>{t("fs.confirm.body")}</p>
          <div className="file-manager__dialog-actions">
            <button
              onClick={() => {
                const next = pendingOpen;
                setPendingOpen(null);
                void loadFile(next.rootId, next.path);
              }}
              type="button"
            >
              {t("fs.confirm.continue")}
            </button>
            <button onClick={() => setPendingOpen(null)} type="button">
              {t("fs.confirm.cancel")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
