import { useCallback, useEffect, useState } from "react";

import type { DesktopApi } from "../../../src/desktop-api";
import type { FsEntry, FsEntryKind, FsFileReport, FsRootsReport } from "../../../src/contracts";
import { useI18n } from "../../../src/i18n";

interface FileManagerPanelProps {
  api: DesktopApi;
}

/** Lazy listing per "rootId:path" (the Rust side owns containment). */
type Listings = Record<string, { entries: FsEntry[]; truncated: boolean }>;

/**
 * FS-M1 workbench surface: environment-linked roots, a lazy tree and a
 * read-only text view.
 *
 * Human-only by construction - every call goes through the Shell capability,
 * every path is root-relative, and a containment failure comes back as a
 * malformed request the panel simply shows. There is no editor here yet:
 * writes land in FS-M2 with the atomic-save and conflict flow.
 */
export function FileManagerPanel({ api }: FileManagerPanelProps) {
  const { t } = useI18n();
  const [roots, setRoots] = useState<FsRootsReport | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [listings, setListings] = useState<Listings>({});
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [file, setFile] = useState<FsFileReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const report = await api.fsListRoots({ schemaVersion: 1 });
      setRoots(report);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [api]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const toggle = async (rootId: string, path: string) => {
    const key = rootId + ":" + path;
    if (open[key]) {
      setOpen((current) => ({ ...current, [key]: false }));
      return;
    }
    try {
      if (!listings[key]) {
        const report = await api.fsReadDir({
          schemaVersion: 1,
          rootId,
          relativePath: path,
          showHidden,
        });
        setListings((current) => ({
          ...current,
          [key]: { entries: report.entries, truncated: report.truncated },
        }));
      }
      setOpen((current) => ({ ...current, [key]: true }));
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const openFile = async (rootId: string, path: string) => {
    try {
      setFile(await api.fsReadFile({ schemaVersion: 1, rootId, relativePath: path }));
      setError(null);
    } catch (cause) {
      setFile(null);
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  // Rebuild the visible rows each render: open directories are expanded
  // depth-first, so the tree stays a pure function of the loaded listings.
  const rows: Array<{ key: string; rootId: string; path: string; entry: FsEntry; depth: number }> = [];
  const walk = (rootId: string, path: string, depth: number) => {
    const listing = listings[rootId + ":" + path];
    if (!listing) {
      return;
    }
    for (const entry of listing.entries) {
      const childPath = path ? path + "/" + entry.name : entry.name;
      rows.push({ key: rootId + ":" + childPath, rootId, path: childPath, entry, depth });
      if (entry.kind === "dir" && open[rootId + ":" + childPath]) {
        walk(rootId, childPath, depth + 1);
      }
    }
  };

  const kindLabel = (kind: FsEntryKind) => t("fs.kind." + kind);

  return (
    <section className="panel file-manager" aria-label={t("surface.workbench")}>
      <header className="panel__heading panel__heading--split">
        <div>
          <p className="eyebrow">{t("rail.workbench")}</p>
          <h2>{t("surface.workbench")}</h2>
        </div>
        <label className="file-manager__toggle">
          <input
            checked={showHidden}
            onChange={(event) => {
              setShowHidden(event.target.checked);
              // Loaded listings belong to the previous filter.
              setListings({});
              setOpen({});
            }}
            type="checkbox"
          />
          {t("fs.showHidden")}
        </label>
      </header>
      <p className="panel__note">{t("fs.subtitle")}</p>

      {error && (
        <p className="file-manager__error" role="alert">
          {error}
        </p>
      )}

      <div className="file-manager__body">
        <div className="file-manager__tree">
          {roots === null && !error && <p className="panel__note">{t("fs.loading")}</p>}
          {roots !== null && roots.roots.length === 0 && (
            <p className="panel__note">{t("fs.emptyRoots")}</p>
          )}
          {(roots?.roots ?? []).map((root) => {
            const rootKey = root.id + ":";
            if (!root.available) {
              return (
                <div className="file-manager__root" key={root.id} data-available="false">
                  <span className="file-manager__root-label">{root.label}</span>
                  <span className="file-manager__root-reason">
                    {t("fs.unavailable")}: {root.reason ?? ""}
                  </span>
                </div>
              );
            }
            return (
              <div className="file-manager__root" key={root.id}>
                <button
                  aria-expanded={open[rootKey] === true}
                  className="file-manager__root-label"
                  onClick={() => void toggle(root.id, "")}
                  type="button"
                >
                  {root.label}
                </button>
              </div>
            );
          })}
          {(roots?.roots ?? []).map((root) => {
            if (!root.available || !open[root.id + ":"]) {
              return null;
            }
            walk(root.id, "", 0);
            const listing = listings[root.id + ":"];
            if (listing && listing.entries.length === 0) {
              return <p className="panel__note" key={root.id + ":empty"}>{t("fs.empty")}</p>;
            }
            return null;
          })}
          <ul className="file-manager__rows">
            {rows.map((row) => (
              <li key={row.key} style={{ paddingLeft: 8 + row.depth * 14 }}>
                {row.entry.kind === "dir" ? (
                  <button
                    aria-expanded={open[row.key] === true}
                    className="file-manager__row"
                    onClick={() => void toggle(row.rootId, row.path)}
                    type="button"
                  >
                    <span aria-hidden="true">▸</span> {row.entry.name}/
                  </button>
                ) : row.entry.kind === "file" ? (
                  <button
                    className="file-manager__row"
                    onClick={() => void openFile(row.rootId, row.path)}
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
          {Object.values(listings).some((listing) => listing.truncated) && (
            <p className="panel__note">{t("fs.truncated")}</p>
          )}
        </div>

        <div className="file-manager__view">
          {file === null ? (
            <p className="panel__note">{t("fs.selectFile")}</p>
          ) : (
            <>
              <div className="file-manager__view-header">
                <code>{file.path}</code>
                <span className="file-manager__meta">
                  {t("fs.size")} {file.size} B · {t("fs.encoding")} {file.encoding}
                </span>
              </div>
              {file.readOnly && (
                <p className="file-manager__banner" role="status">
                  {file.encoding === "binary" ? t("fs.readOnly.binary") : t("fs.readOnly.overLimit")}
                </p>
              )}
              <pre className="file-manager__content">{file.content}</pre>
            </>
          )}
        </div>
      </div>
    </section>
  );
}
