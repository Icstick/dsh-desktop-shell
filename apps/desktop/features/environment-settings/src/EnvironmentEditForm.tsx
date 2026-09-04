// Environment partition edit form (env quick-edit D3): replaces the wizard's
// edit mode with a flat, sectioned dialog (name / source / data / endpoint /
// advanced / ownership). Every section edits independently — there is no step
// order and no next/back navigation. Saving keeps the original environment id
// (upsert-by-id semantics), runs backend validation first, and reports the
// saved environment/catalog/result through onSaved.
//
// v1 read-only parts: id (changing it means remove + re-create), harness mode,
// cwd, policy values, and ownership (managed/attached). The dialog shell
// (open/close, busy orchestration) is owned by ShellApp.

import { useMemo, useRef, useState } from "react";

import type {
  DiscoverProfilesReport,
  DshEnvironment,
  EnvironmentCatalog,
  EnvironmentValidation,
  ValidationIssue,
} from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { useI18n } from "../../../src/i18n";
import {
  convertEnvironmentDraft,
  environmentToDraft,
  type EnvironmentDraft,
} from "./environment-draft";

export interface EnvironmentEditFormProps {
  api: DesktopApi;
  /** Read-only baseline snapshot of the environment being edited. */
  environment: DshEnvironment;
  /** Current catalog; accepted for ShellApp context (no id-collision check
   *  needed — the id is fixed, upsert-by-id). */
  catalog: EnvironmentCatalog;
  /** True while the ShellApp orchestrates around the dialog; disables all commits. */
  busy?: boolean;
  onClose(): void;
  onSaved(
    environment: DshEnvironment,
    catalog: EnvironmentCatalog,
    result: EnvironmentValidation,
  ): void;
}

function fieldIssue(issues: ValidationIssue[], field: string): ValidationIssue | undefined {
  return issues.find((issue) => issue.field === field);
}

export function EnvironmentEditForm({
  api,
  environment,
  catalog,
  busy,
  onClose,
  onSaved,
}: EnvironmentEditFormProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<EnvironmentDraft>(() => environmentToDraft(environment));
  const [profiles, setProfiles] = useState<DiscoverProfilesReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [backendIssues, setBackendIssues] = useState<ValidationIssue[]>([]);
  const savingRef = useRef(false);
  const scanningRef = useRef(false);

  const update = <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => {
    setDraft((current) => ({ ...current, [key]: value }));
    // Any edit invalidates earlier backend results.
    setBackendIssues([]);
    setError(null);
    if (key === "dshHome") setProfiles(null);
  };

  // Local (client-side) issues come straight from the shared draft converter
  // and gate the Save button; they are also shown inline under each field.
  const localIssues = useMemo(
    () => convertEnvironmentDraft({ ...draft, id: environment.id }).issues,
    [draft, environment.id],
  );
  const issueOf = (field: string) => fieldIssue(localIssues, field);

  const scanProfiles = async () => {
    const home = draft.dshHome.trim();
    if (!home || busy || saving || scanningRef.current) return;
    scanningRef.current = true;
    setScanning(true);
    setBackendIssues([]);
    setError(null);
    try {
      const report = await api.discoverProfiles({ schemaVersion: 1, dshHome: home });
      setProfiles(report);
    } catch {
      // Scan is best-effort: on failure keep the plain input (D3).
      setProfiles(null);
    } finally {
      scanningRef.current = false;
      setScanning(false);
    }
  };

  const browseDirectory = async (target: "harness" | "home") => {
    if (busy || saving || savingRef.current) return;
    setBackendIssues([]);
    setError(null);
    try {
      const picked = await api.pickDirectory();
      if (picked) update(target === "harness" ? "harnessPath" : "dshHome", picked);
    } catch {
      setError(t("wizard.error.browse"));
    }
  };

  const save = async () => {
    if (busy || saving || scanning || savingRef.current) return;
    const conversion = convertEnvironmentDraft({ ...draft, id: environment.id });
    if (!conversion.environment) return; // local issues keep Save disabled
    savingRef.current = true;
    setSaving(true);
    setBackendIssues([]);
    setError(null);
    try {
      const result = await api.validateEnvironment(conversion.environment);
      if (!result.valid) {
        setBackendIssues(result.issues);
        return;
      }
      try {
        const nextCatalog = await api.saveEnvironment(conversion.environment);
        onSaved(conversion.environment, nextCatalog, result);
      } catch {
        setError(t("envEdit.errorSave"));
      }
    } catch {
      setError(t("envEdit.errorValidate"));
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const working = busy || saving;
  const repositoryMode = draft.harnessMode === "repository";
  // D3: nodePath is editable only for managed repository environments.
  const showNodePath = repositoryMode && draft.ownership === "managed";
  const modeLabel =
    draft.harnessMode === "repository"
      ? t("envEdit.mode.repository")
      : draft.harnessMode === "executable"
        ? t("envEdit.mode.executable")
        : draft.harnessMode;
  const cwdValue = draft.cwd.trim()
    ? draft.cwd
    : repositoryMode
      ? t("envEdit.cwdDefault")
      : t("envEdit.cwdUnset");

  const title = draft.label.trim() || environment.label;

  return (
    <section className="environment-edit" aria-label={title} data-testid="environment-edit">
      <header className="environment-edit__header">
        <h2 className="environment-edit__title" data-testid="edit-title">
          {title}
        </h2>
        <code className="environment-edit__id" data-testid="edit-id">
          {draft.id}
        </code>
        <button
          type="button"
          className="environment-edit__close"
          onClick={onClose}
          disabled={working}
          data-testid="edit-close"
        >
          {t("envEdit.close")}
        </button>
      </header>

      {error && (
        <p className="environment-edit__error" role="alert" data-testid="edit-error">
          {error}
        </p>
      )}

      <fieldset className="environment-edit__section" data-testid="edit-section-name">
        <legend>{t("envEdit.section.name")}</legend>
        <label className="field">
          <span>{t("envEdit.label")}</span>
          <input
            type="text"
            value={draft.label}
            onChange={(event) => update("label", event.target.value)}
            data-testid="edit-label"
          />
          {issueOf("label") && (
            <small className="field__error" role="alert" data-testid="edit-label-error">
              {issueOf("label")!.message}
            </small>
          )}
        </label>
        <div className="environment-edit__kv">
          <span className="environment-edit__kv-key">{t("envEdit.id")}</span>
          <code className="environment-edit__kv-value">{draft.id}</code>
        </div>
        <p className="environment-edit__hint" data-testid="edit-id-note">
          {t("envEdit.idReadonly")}
        </p>
      </fieldset>

      <fieldset className="environment-edit__section" data-testid="edit-section-source">
        <legend>{t("envEdit.section.source")}</legend>
        <div className="environment-edit__kv">
          <span className="environment-edit__kv-key">{t("envEdit.mode")}</span>
          <span className="environment-edit__mode" data-testid="edit-mode">
            {modeLabel}
          </span>
        </div>
        <label className="field">
          <span>{t("envEdit.harnessPath")}</span>
          <div className="setup-wizard__row">
            <input
              type="text"
              value={draft.harnessPath}
              placeholder={t("wizard.source.placeholder")}
              onChange={(event) => update("harnessPath", event.target.value)}
              data-testid="edit-harness-path"
            />
            <button
              type="button"
              className="setup-wizard__browse"
              onClick={() => void browseDirectory("harness")}
              disabled={working}
              data-testid="edit-browse-harness"
            >
              {t("wizard.browse")}
            </button>
          </div>
          {issueOf("harness.path") && (
            <small className="field__error" role="alert" data-testid="edit-harness-path-error">
              {issueOf("harness.path")!.message}
            </small>
          )}
        </label>
        <div className="environment-edit__kv">
          <span className="environment-edit__kv-key">{t("wizard.advanced.cwd")}</span>
          {draft.cwd.trim() ? (
            <code className="environment-edit__kv-value" data-testid="edit-cwd">
              {draft.cwd}
            </code>
          ) : (
            <span className="environment-edit__kv-hint" data-testid="edit-cwd">
              {cwdValue}
            </span>
          )}
        </div>
      </fieldset>

      <fieldset className="environment-edit__section" data-testid="edit-section-data">
        <legend>{t("envEdit.section.data")}</legend>
        <label className="field">
          <span>{t("wizard.review.home")}</span>
          <div className="setup-wizard__row">
            <input
              type="text"
              value={draft.dshHome}
              placeholder={t("wizard.profile.homePlaceholder")}
              onChange={(event) => update("dshHome", event.target.value)}
              data-testid="edit-dsh-home"
            />
            <button
              type="button"
              className="setup-wizard__browse"
              onClick={() => void browseDirectory("home")}
              disabled={working}
              data-testid="edit-browse-home"
            >
              {t("wizard.browse")}
            </button>
          </div>
          {issueOf("dshHome") && (
            <small className="field__error" role="alert" data-testid="edit-dsh-home-error">
              {issueOf("dshHome")!.message}
            </small>
          )}
        </label>
        <label className="field">
          <span>{t("wizard.review.profile")}</span>
          <div className="setup-wizard__row">
            <input
              type="text"
              value={draft.profile}
              placeholder={t("wizard.profile.namePlaceholder")}
              onChange={(event) => update("profile", event.target.value.trim())}
              data-testid="edit-profile"
            />
            <button
              type="button"
              className="setup-wizard__browse"
              onClick={() => void scanProfiles()}
              disabled={working || scanning || !draft.dshHome.trim()}
              data-testid="edit-scan-profiles"
            >
              {scanning ? t("wizard.profile.scanning") : t("wizard.profile.scan")}
            </button>
          </div>
          {issueOf("profile") && (
            <small className="field__error" role="alert" data-testid="edit-profile-error">
              {issueOf("profile")!.message}
            </small>
          )}
        </label>
        {profiles && (
          <ul className="setup-wizard__profiles" data-testid="edit-profile-options">
            {profiles.profiles.length === 0 && (
              <li className="is-empty">{t("wizard.profile.none")}</li>
            )}
            {profiles.profiles.map((entry) => (
              <li key={entry.name}>
                <label>
                  <input
                    type="radio"
                    name="edit-profile-option"
                    checked={draft.profile === entry.name}
                    onChange={() => update("profile", entry.name)}
                  />
                  <code>{entry.name}</code>
                  {!entry.hasRootConfig && (
                    <span className="setup-wizard__profile-warning">{t("wizard.profile.noConfig")}</span>
                  )}
                </label>
              </li>
            ))}
          </ul>
        )}
      </fieldset>

      <fieldset className="environment-edit__section" data-testid="edit-section-endpoint">
        <legend>{t("envEdit.section.endpoint")}</legend>
        <label className="field">
          <span>{t("wizard.advanced.port")}</span>
          <input
            type="text"
            value={draft.port}
            placeholder={t("wizard.advanced.portPlaceholder")}
            onChange={(event) => update("port", event.target.value.trim())}
            data-testid="edit-port"
          />
          <small data-testid="edit-port-hint">
            {draft.ownership === "attached"
              ? t("envEdit.portAttachedHint")
              : t("envEdit.portManagedHint")}
          </small>
          {issueOf("endpoint.port") && (
            <small className="field__error" role="alert" data-testid="edit-port-error">
              {issueOf("endpoint.port")!.message}
            </small>
          )}
        </label>
      </fieldset>

      <fieldset className="environment-edit__section" data-testid="edit-section-advanced">
        <legend>{t("envEdit.section.advanced")}</legend>
        {showNodePath && (
          <label className="field">
            <span>{t("wizard.advanced.nodePath")}</span>
            <input
              type="text"
              value={draft.nodePath}
              placeholder="node"
              onChange={(event) => update("nodePath", event.target.value)}
              data-testid="edit-node-path"
            />
            <small>{t("wizard.advanced.nodePathHint")}</small>
          </label>
        )}
        {draft.ownership === "managed" && (
          <>
            <div className="environment-edit__kv" data-testid="edit-policy">
              <span className="environment-edit__kv-key">{t("envEdit.policy.autoRestartOnCrash")}</span>
              <span
                className={
                  "environment-edit__bool " +
                  (draft.autoRestartOnCrash ? "is-on" : "is-off")
                }
                data-testid="edit-policy-autorestart"
              >
                {String(draft.autoRestartOnCrash)}
              </span>
              <span className="environment-edit__kv-key">{t("envEdit.policy.allowNativeAdapter")}</span>
              <span
                className={
                  "environment-edit__bool " +
                  (draft.allowNativeAdapter ? "is-on" : "is-off")
                }
                data-testid="edit-policy-adapter"
              >
                {String(draft.allowNativeAdapter)}
              </span>
            </div>
            <p className="environment-edit__hint">{t("envEdit.policy.readonly")}</p>
          </>
        )}
      </fieldset>

      <fieldset className="environment-edit__section" data-testid="edit-section-ownership">
        <legend>{t("envEdit.section.ownership")}</legend>
        <div className="environment-edit__kv">
          <span className="environment-edit__kv-key">{t("envEdit.section.ownership")}</span>
          <span className="environment-edit__mode" data-testid="edit-ownership">
            {draft.ownership === "managed"
              ? t("wizard.mode.managed")
              : t("wizard.mode.attached")}
          </span>
        </div>
        <p className="environment-edit__hint" data-testid="edit-ownership-note">
          {t("envEdit.ownershipLocked")}
        </p>
      </fieldset>

      {backendIssues.length > 0 && (
        <div className="environment-edit__issues" role="alert" data-testid="edit-issues">
          <p className="environment-edit__issues-title">{t("envEdit.issuesHeading")}</p>
          <ul className="setup-wizard__issues">
            {backendIssues.map((issue) => (
              <li key={issue.field + ":" + issue.code}>
                <code>{issue.field}</code> — {issue.message}
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="environment-edit__footer">
        <button
          type="button"
          className="environment-edit__cancel"
          onClick={onClose}
          disabled={working}
          data-testid="edit-cancel"
        >
          {t("envEdit.cancel")}
        </button>
        <button
          type="button"
          className="environment-edit__save"
          onClick={() => void save()}
          disabled={working || scanning || localIssues.length > 0}
          data-testid="edit-save"
        >
          {saving ? t("envEdit.saving") : t("envEdit.save")}
        </button>
      </div>
    </section>
  );
}
