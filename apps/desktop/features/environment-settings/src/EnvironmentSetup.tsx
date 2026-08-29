import { useState, type FormEvent } from "react";

import type {
  DshEnvironment,
  EnvironmentCatalog,
  EnvironmentValidation,
  HarnessCandidate,
  HarnessDiscoveryReport,
} from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import {
  convertEnvironmentDraft,
  environmentToDraft,
  initialEnvironmentDraft,
  type EnvironmentDraft,
} from "./environment-draft";

interface EnvironmentSetupProps {
  api: DesktopApi;
  initialEnvironment: DshEnvironment | null;
  onSaved(
    environment: DshEnvironment,
    catalog: EnvironmentCatalog,
    result: EnvironmentValidation,
  ): void;
}

export function EnvironmentSetup({ api, initialEnvironment, onSaved }: EnvironmentSetupProps) {
  const [draft, setDraft] = useState<EnvironmentDraft>(() =>
    initialEnvironment ? environmentToDraft(initialEnvironment) : initialEnvironmentDraft,
  );
  const [result, setResult] = useState<EnvironmentValidation | null>(null);
  const [validatedEnvironment, setValidatedEnvironment] = useState<DshEnvironment | null>(null);
  const [discovery, setDiscovery] = useState<HarnessDiscoveryReport | null>(null);
  const [localIssues, setLocalIssues] = useState<EnvironmentValidation["issues"]>([]);
  const [backendError, setBackendError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [discovering, setDiscovering] = useState(false);
  const [saving, setSaving] = useState(false);
  const [savedRevision, setSavedRevision] = useState<number | null>(null);

  const update = <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setResult(null);
    setValidatedEnvironment(null);
    setSavedRevision(null);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBackendError(null);
    setResult(null);
    const conversion = convertEnvironmentDraft(draft);
    setLocalIssues(conversion.issues);
    if (!conversion.environment) return;

    setSubmitting(true);
    try {
      const validation = await api.validateEnvironment(conversion.environment);
      setResult(validation);
      setValidatedEnvironment(validation.valid ? conversion.environment : null);
    } catch {
      setBackendError("Environment validation backend is unavailable. No settings were saved.");
    } finally {
      setSubmitting(false);
    }
  };

  const discover = async () => {
    setBackendError(null);
    setDiscovering(true);
    try {
      const explicitPath = draft.harnessPath.trim();
      setDiscovery(
        await api.discoverHarnesses({
          schemaVersion: 1,
          explicitPaths: explicitPath ? [explicitPath] : [],
          includeDshPath: true,
          includePath: true,
        }),
      );
    } catch {
      setBackendError("Harness discovery is unavailable. No candidate was executed.");
    } finally {
      setDiscovering(false);
    }
  };

  const selectCandidate = (candidate: HarnessCandidate) => {
    setDraft((current) => ({
      ...current,
      harnessMode: candidate.mode,
      harnessPath: candidate.canonicalPath ?? candidate.requestedPath,
    }));
    setResult(null);
    setValidatedEnvironment(null);
    setSavedRevision(null);
  };

  const save = async () => {
    if (!validatedEnvironment || !result?.valid) return;
    setBackendError(null);
    setSaving(true);
    try {
      const catalog = await api.saveEnvironment(validatedEnvironment);
      setSavedRevision(catalog.revision);
      onSaved(validatedEnvironment, catalog, result);
    } catch {
      setBackendError("Environment could not be saved. DSH was not launched or modified.");
    } finally {
      setSaving(false);
    }
  };

  const issues = result?.issues ?? localIssues;

  return (
    <section className="panel setup-panel" aria-labelledby="setup-heading">
      <div className="panel__heading panel__heading--split">
        <div>
          <p className="eyebrow">Desktop-owned reference</p>
          <h2 id="setup-heading">Configure an existing DSH</h2>
        </div>
        <span className="boundary-chip">No install · No profile mutation</span>
      </div>

      <form className="setup-form" onSubmit={submit}>
        <fieldset>
          <legend>Identity</legend>
          <div className="form-grid form-grid--two">
            <Field label="Environment ID" issue={findIssue(issues, "id")}>
              <input value={draft.id} onChange={(event) => update("id", event.target.value)} />
            </Field>
            <Field label="Display label" issue={findIssue(issues, "label")}>
              <input value={draft.label} onChange={(event) => update("label", event.target.value)} />
            </Field>
          </div>
        </fieldset>

        <fieldset>
          <legend>Harness source</legend>
          <div className="form-grid form-grid--two">
            <Field label="Source type">
              <select
                value={draft.harnessMode}
                onChange={(event) => update("harnessMode", event.target.value as EnvironmentDraft["harnessMode"])}
              >
                <option value="executable">Executable</option>
                <option value="repository">Prebuilt source checkout</option>
                <option value="command">Advanced command</option>
              </select>
            </Field>
            <Field label="Executable or recipe path" issue={findIssue(issues, "harness.path")}>
              <input value={draft.harnessPath} onChange={(event) => update("harnessPath", event.target.value)} />
            </Field>
            <Field label="Working directory (optional)">
              <input value={draft.cwd} onChange={(event) => update("cwd", event.target.value)} />
            </Field>
            <Field
              label="Node executable (optional)"
              issue={findIssue(issues, "nodePath")}
              hint="Managed prebuilt source checkout only; must be an absolute existing executable."
            >
              <input value={draft.nodePath} onChange={(event) => update("nodePath", event.target.value)} />
            </Field>
          </div>
          <Field
            label="Extra arguments — one literal argument per line"
            issue={findIssue(issues, "harness.args")}
            hint="--host, --port, --trusted-host and --no-open are reserved. Values are not shell-parsed."
          >
            <textarea
              rows={3}
              value={draft.extraArguments}
              onChange={(event) => update("extraArguments", event.target.value)}
            />
          </Field>
          <div className="discovery-actions">
            <p>Inspect explicit path, DSH_PATH and PATH without launching candidates.</p>
            <button className="secondary-button" disabled={discovering} onClick={discover} type="button">
              {discovering ? "Discovering…" : "Discover harnesses"}
            </button>
          </div>
          {discovery && <DiscoveryResults report={discovery} onSelect={selectCandidate} />}
        </fieldset>

        <fieldset>
          <legend>DSH state</legend>
          <div className="form-grid form-grid--two">
            <Field label="DSH_HOME" issue={findIssue(issues, "dshHome")}>
              <input value={draft.dshHome} onChange={(event) => update("dshHome", event.target.value)} />
            </Field>
            <Field label="Profile" issue={findIssue(issues, "profile")}>
              <input value={draft.profile} onChange={(event) => update("profile", event.target.value)} />
            </Field>
          </div>
        </fieldset>

        <fieldset>
          <legend>Ownership and endpoint</legend>
          <div className="ownership-grid">
            <OwnershipCard
              checked={draft.ownership === "managed"}
              description="Desktop may start, stop and recover only the process it creates."
              label="Managed"
              onSelect={() => update("ownership", "managed")}
            />
            <OwnershipCard
              checked={draft.ownership === "attached"}
              description="Connect and observe. Lifecycle mutation is always denied."
              label="Attached"
              onSelect={() => update("ownership", "attached")}
            />
          </div>
          <div className="form-grid form-grid--two">
            <Field label="Host" hint="Fixed security boundary">
              <input disabled value="127.0.0.1" />
            </Field>
            <Field
              label="Port"
              issue={findIssue(issues, "endpoint.port")}
              hint={
                draft.ownership === "attached"
                  ? "Attached health requires a fixed loopback port; auto is saved but cannot be probed."
                  : undefined
              }
            >
              <input value={draft.port} onChange={(event) => update("port", event.target.value)} />
            </Field>
          </div>
          <div className="checkbox-row">
            <label>
              <input
                checked={draft.autoRestartOnCrash}
                disabled={draft.ownership === "attached"}
                onChange={(event) => update("autoRestartOnCrash", event.target.checked)}
                type="checkbox"
              />
              Recover after crash within budget
            </label>
            {/* Reserved M4/M5 knob: allowNativeAdapter has no backend consumer yet
                (ADR-0014 broker dispatch is the future gate). Hidden until the
                adapter grant path exists so the UI never claims an unenforced
                authorization. */}
            <label className="reserved-option">
              <input
                checked={draft.allowNativeAdapter}
                disabled
                onChange={(event) => update("allowNativeAdapter", event.target.checked)}
                type="checkbox"
              />
              Allow negotiated native adapter (reserved)
            </label>
          </div>
        </fieldset>

        {issues.length > 0 && (
          <div className="callout callout--danger" role="alert">
            <strong>Validation failed</strong>
            <ul>{issues.map((issue) => <li key={`${issue.field}-${issue.code}`}>{issue.message}</li>)}</ul>
          </div>
        )}
        {backendError && <div className="callout callout--danger" role="alert">{backendError}</div>}
        {savedRevision !== null && (
          <div className="callout callout--success" role="status">
            Environment saved as active catalog revision {savedRevision}. DSH remains stopped.
          </div>
        )}
        {result?.valid && result.launchPreview && <LaunchPreviewView preview={result.launchPreview} />}

        <div className="form-actions">
          <p>Validation is read-only. Saving is explicit and never launches DSH.</p>
          <div className="button-row">
            <button className="secondary-button" disabled={submitting || saving} type="submit">
              {submitting ? "Validating…" : "Validate environment"}
            </button>
            <button
              className="primary-button"
              disabled={!validatedEnvironment || !result?.valid || submitting || saving}
              onClick={save}
              type="button"
            >
              {saving ? "Saving…" : "Save active environment"}
            </button>
          </div>
        </div>
      </form>
    </section>
  );
}

function DiscoveryResults({
  report,
  onSelect,
}: {
  report: HarnessDiscoveryReport;
  onSelect(candidate: HarnessCandidate): void;
}) {
  return (
    <section className="discovery-results" aria-label="Harness discovery results">
      <div className="discovery-results__summary">
        <strong>{report.candidates.length} candidate(s)</strong>
        <span>
          Scanned {report.scannedSources.join(", ")}; deferred {report.deferredSources.join(", ")}.
        </span>
      </div>
      {report.candidates.length === 0 ? (
        <p>No candidates found. Provide an explicit path or configure DSH_PATH.</p>
      ) : (
        <ul>
          {report.candidates.map((candidate) => {
            const selectable = candidate.launchable || candidate.status === "requires_recipe";
            return (
              <li key={candidate.id}>
                <div>
                  <span className="candidate-status" data-status={candidate.status}>
                    {candidate.status.replace("_", " ")}
                  </span>
                  <strong>{candidate.canonicalPath ?? candidate.requestedPath}</strong>
                  <small>{candidate.evidence.map((item) => item.message).join(" ")}</small>
                </div>
                <button
                  className="secondary-button"
                  disabled={!selectable}
                  onClick={() => onSelect(candidate)}
                  type="button"
                >
                  Use candidate
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

function Field({
  label,
  hint,
  issue,
  children,
}: {
  label: string;
  hint?: string;
  issue?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <small>{hint}</small>}
      {issue && <small className="field__error">{issue}</small>}
    </label>
  );
}

function OwnershipCard({
  checked,
  description,
  label,
  onSelect,
}: {
  checked: boolean;
  description: string;
  label: string;
  onSelect(): void;
}) {
  return (
    <label className="ownership-card" data-checked={checked}>
      <input checked={checked} name="ownership" onChange={onSelect} type="radio" />
      <span><strong>{label}</strong><small>{description}</small></span>
    </label>
  );
}

function LaunchPreviewView({ preview }: { preview: NonNullable<EnvironmentValidation["launchPreview"]> }) {
  return (
    <section className="launch-preview" aria-label="Redacted launch preview">
      <div><span>Source</span><strong>{preview.source}</strong></div>
      <div><span>Executable</span><strong>{preview.executable}</strong></div>
      <div><span>Working directory</span><strong>{preview.cwd ?? "inherited"}</strong></div>
      <div><span>Ownership</span><strong>{preview.ownership}</strong></div>
      <div><span>Endpoint</span><strong>{preview.endpoint}</strong></div>
      <div className="launch-preview__args">
        <span>Arguments</span>
        <code>{preview.arguments.map((argument) => argument.display).join(" ") || "none (attached)"}</code>
      </div>
    </section>
  );
}

function findIssue(issues: EnvironmentValidation["issues"], field: string) {
  return issues.find((issue) => issue.field === field)?.message;
}
