// Setup wizard (M7-A, WI-M7-SETUP-WIZARD): a guided six-step environment
// configuration flow replacing the single-page form.
//
// Steps:
//   1. mode     – Managed (launch locally) | Attached (connect to a running DSH)
//   2. harness  – discover candidates (PATH/DSH_PATH/explicit) or type a path
//   3. profile  – scan $DSH_HOME/profiles, pick or type a new profile name
//   4. advanced – dshHome confirm + port (auto or explicit) with a probe
//   5. review   – backend validation (issues + launch preview)
//   6. finish   – save to the catalog, then launch (managed) or probe (attached)
//
// Every step validates locally where possible and lets the user go back.

import { useState, type ReactNode } from "react";

import type {
  BackendOwnership,
  DiscoverProfilesReport,
  DshEnvironment,
  EnvironmentCatalog,
  EnvironmentValidation,
  HarnessCandidate,
  HarnessDiscoveryReport,
  ProbePortReport,
} from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import {
  convertEnvironmentDraft,
  environmentToDraft,
  initialEnvironmentDraft,
  type EnvironmentDraft,
} from "./environment-draft";

interface SetupWizardProps {
  api: DesktopApi;
  initialEnvironment: DshEnvironment | null;
  onSaved(
    environment: DshEnvironment,
    catalog: EnvironmentCatalog,
    result: EnvironmentValidation,
  ): void;
}

const STEPS = ["mode", "harness", "profile", "advanced", "review", "finish"] as const;
type StepId = (typeof STEPS)[number];

const STEP_TITLES: Record<StepId, string> = {
  mode: "Mode",
  harness: "DSH executable",
  profile: "Profile",
  advanced: "Home & port",
  review: "Review",
  finish: "Save & launch",
};

export function SetupWizard({ api, initialEnvironment, onSaved }: SetupWizardProps) {
  const [stepIndex, setStepIndex] = useState(0);
  const [draft, setDraft] = useState<EnvironmentDraft>(() =>
    initialEnvironment ? environmentToDraft(initialEnvironment) : initialEnvironmentDraft,
  );
  const [discovery, setDiscovery] = useState<HarnessDiscoveryReport | null>(null);
  const [profiles, setProfiles] = useState<DiscoverProfilesReport | null>(null);
  const [probe, setProbe] = useState<ProbePortReport | null>(null);
  const [validation, setValidation] = useState<EnvironmentValidation | null>(null);
  const [busy, setBusy] = useState(false);
  const [backendError, setBackendError] = useState<string | null>(null);
  const [savedRevision, setSavedRevision] = useState<number | null>(null);
  const [launchMessage, setLaunchMessage] = useState<string | null>(null);

  const step = STEPS[stepIndex];
  const update = <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setValidation(null);
    setBackendError(null);
    setLaunchMessage(null);
  };
  const go = (index: number) => {
    setStepIndex(Math.max(0, Math.min(STEPS.length - 1, index)));
    setBackendError(null);
    setLaunchMessage(null);
  };
  const next = () => go(stepIndex + 1);
  const back = () => go(stepIndex - 1);

  // ---- step helpers ----
  const runDiscovery = async () => {
    setBusy(true);
    setBackendError(null);
    try {
      const explicit = draft.harnessPath.trim();
      const report = await api.discoverHarnesses({
        schemaVersion: 1,
        explicitPaths: explicit ? [explicit] : [],
        includeDshPath: true,
        includePath: true,
      });
      setDiscovery(report);
      // Auto-pick the first launchable candidate.
      const first = report.candidates.find((candidate) => candidate.status === "available" && candidate.launchable);
      if (first) applyCandidate(first);
    } catch {
      setBackendError("DSH discovery is unavailable.");
    } finally {
      setBusy(false);
    }
  };

  const applyCandidate = (candidate: HarnessCandidate) => {
    update("harnessPath", candidate.canonicalPath ?? candidate.requestedPath);
    update("harnessMode", candidate.mode);
  };

  const scanProfiles = async () => {
    const home = draft.dshHome.trim();
    if (!home) {
      setBackendError("Enter a DSH_HOME directory first.");
      return;
    }
    setBusy(true);
    setBackendError(null);
    try {
      const report = await api.discoverProfiles({ schemaVersion: 1, dshHome: home });
      setProfiles(report);
      if (report.profiles.length === 1 && report.profiles[0].hasRootConfig) {
        update("profile", report.profiles[0].name);
      }
    } catch {
      setBackendError("Profile scan failed — check the DSH_HOME path.");
    } finally {
      setBusy(false);
    }
  };

  const runProbe = async () => {
    const parsed = Number(draft.port);
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65535) {
      setBackendError("Port must be a number between 1 and 65535.");
      return;
    }
    setBusy(true);
    setBackendError(null);
    try {
      const report = await api.probePort({ schemaVersion: 1, port: parsed });
      setProbe(report);
    } catch {
      setBackendError("Port probe is unavailable.");
    } finally {
      setBusy(false);
    }
  };

  const review = async () => {
    setBusy(true);
    setBackendError(null);
    const conversion = convertEnvironmentDraft(draft);
    if (!conversion.environment) {
      setBackendError("Fix the highlighted fields before reviewing.");
      setBusy(false);
      return;
    }
    try {
      const result = await api.validateEnvironment(conversion.environment);
      setValidation(result);
      if (!result.valid) {
        setBackendError("The environment failed validation — see the issues below.");
      }
    } catch {
      setBackendError("Environment validation backend is unavailable.");
    } finally {
      setBusy(false);
    }
  };

  const finish = async () => {
    if (!validation?.valid) return;
    setBusy(true);
    setBackendError(null);
    setLaunchMessage(null);
    try {
      const environment = convertEnvironmentDraft(draft).environment!;
      const catalog = await api.saveEnvironment(environment);
      setSavedRevision(catalog.revision);
      onSaved(environment, catalog, validation);

      if (environment.ownership === "managed") {
        await api.startManagedEnvironment({
          schemaVersion: 1,
          environmentId: environment.id,
        });
        setLaunchMessage("Environment saved and DSH is starting.");
      } else {
        const health = await api.probeAttachedEnvironment({
          schemaVersion: 1,
          environmentId: environment.id,
        });
        setLaunchMessage(
          health.state === "attached"
            ? "Environment saved and the attached DSH is reachable."
            : "Environment saved; the attached DSH was not reachable.",
        );
      }
    } catch {
      setBackendError("The environment could not be saved or launched.");
    } finally {
      setBusy(false);
    }
  };

  // ---- step renderers ----
  const renderStep = (): ReactNode => {
    switch (step) {
      case "mode":
        return <ModeStep draft={draft} update={update} />;
      case "harness":
        return (
          <HarnessStep
            draft={draft}
            discovery={discovery}
            busy={busy}
            update={update}
            onDiscover={runDiscovery}
            onPick={applyCandidate}
          />
        );
      case "profile":
        return (
          <ProfileStep
            draft={draft}
            profiles={profiles}
            busy={busy}
            update={update}
            onScan={scanProfiles}
            onHomeChange={(value) => {
              update("dshHome", value);
              setProfiles(null);
            }}
          />
        );
      case "advanced":
        return (
          <AdvancedStep
            draft={draft}
            probe={probe}
            busy={busy}
            update={update}
            onProbe={runProbe}
            onPortChange={(value) => {
              update("port", value);
              setProbe(null);
            }}
          />
        );
      case "review":
        return <ReviewStep draft={draft} validation={validation} onReview={review} busy={busy} />;
      case "finish":
        return (
          <FinishStep
            draft={draft}
            validation={validation}
            busy={busy}
            savedRevision={savedRevision}
            launchMessage={launchMessage}
            onFinish={finish}
          />
        );
    }
  };

  const canNext =
    step === "mode" ||
    (step === "harness" && draft.harnessPath.trim().length > 0) ||
    (step === "profile" && draft.profile.trim().length > 0) ||
    (step === "advanced" && draft.dshHome.trim().length > 0) ||
    (step === "review" && validation?.valid === true);

  return (
    <div className="setup-wizard" data-testid="setup-wizard">
      <ol className="setup-wizard__steps" aria-label="Setup wizard steps">
        {STEPS.map((id, index) => (
          <li
            key={id}
            className={
              "setup-wizard__step" +
              (index === stepIndex ? " is-active" : "") +
              (index < stepIndex ? " is-done" : "")
            }
            data-testid={"wizard-step-" + id}
          >
            <button type="button" onClick={() => index < stepIndex && go(index)} disabled={index >= stepIndex}>
              <span className="setup-wizard__step-index">{index + 1}</span>
              {STEP_TITLES[id]}
            </button>
          </li>
        ))}
      </ol>

      {backendError && (
        <p className="setup-wizard__error" role="alert" data-testid="wizard-error">
          {backendError}
        </p>
      )}

      <div className="setup-wizard__body">{renderStep()}</div>

      <div className="setup-wizard__nav">
        <button
          type="button"
          className="setup-wizard__back"
          onClick={back}
          disabled={stepIndex === 0 || busy}
        >
          Back
        </button>
        {stepIndex < STEPS.length - 1 ? (
          <button
            type="button"
            className="setup-wizard__next"
            onClick={next}
            disabled={!canNext || busy}
            data-testid="wizard-next"
          >
            Next
          </button>
        ) : (
          // The finish step carries its own primary CTA; the nav keeps
          // only Back to avoid duplicate actions (REVIEW-M7 MEDIUM-2).
          <span className="setup-wizard__nav-end" />
        )}
      </div>
    </div>
  );
}

// ---- step components ----

function ModeStep({
  draft,
  update,
}: {
  draft: EnvironmentDraft;
  update: <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => void;
}) {
  const choose = (ownership: BackendOwnership) => {
    update("ownership", ownership);
    if (ownership === "attached") update("harnessMode", "executable");
  };
  return (
    <fieldset className="setup-wizard__mode">
      <legend>How will this environment run?</legend>
      <label className={"setup-wizard__mode-card" + (draft.ownership === "managed" ? " is-selected" : "")}>
        <input
          type="radio"
          name="ownership"
          checked={draft.ownership === "managed"}
          onChange={() => choose("managed")}
        />
        <strong>Managed</strong>
        <span>Launch a DSH process from this machine and supervise it (restart, health, generation).</span>
      </label>
      <label className={"setup-wizard__mode-card" + (draft.ownership === "attached" ? " is-selected" : "")}>
        <input
          type="radio"
          name="ownership"
          checked={draft.ownership === "attached"}
          onChange={() => choose("attached")}
        />
        <strong>Attached</strong>
        <span>Connect to a DSH instance that is already running (read-only lifecycle).</span>
      </label>
    </fieldset>
  );
}

function HarnessStep({
  draft,
  discovery,
  busy,
  update,
  onDiscover,
  onPick,
}: {
  draft: EnvironmentDraft;
  discovery: HarnessDiscoveryReport | null;
  busy: boolean;
  update: <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => void;
  onDiscover: () => void;
  onPick: (candidate: HarnessCandidate) => void;
}) {
  return (
    <div className="setup-wizard__harness">
      <p>
        Pick the DSH executable. The wizard can search your PATH and DSH_PATH, or you can type
        the full path.
      </p>
      <div className="setup-wizard__row">
        <input
          type="text"
          value={draft.harnessPath}
          placeholder="dsh or C:\path\to\dsh.exe"
          onChange={(event) => update("harnessPath", event.target.value)}
          data-testid="harness-path"
        />
        <button type="button" onClick={onDiscover} disabled={busy} data-testid="discover-button">
          {busy ? "Searching…" : "Search"}
        </button>
      </div>
      {discovery && (
        <ul className="setup-wizard__candidates" data-testid="candidate-list">
          {discovery.candidates.length === 0 && <li className="is-empty">No DSH candidates found.</li>}
          {discovery.candidates.map((candidate) => (
            <li key={candidate.id}>
              <label>
                <input
                  type="radio"
                  name="candidate"
                  checked={draft.harnessPath === (candidate.canonicalPath ?? candidate.requestedPath)}
                  onChange={() => onPick(candidate)}
                />
                <code>{candidate.canonicalPath ?? candidate.requestedPath}</code>
                <span className={"setup-wizard__candidate-status is-" + candidate.status}>
                  {candidate.status}
                </span>
              </label>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function ProfileStep({
  draft,
  profiles,
  busy,
  update,
  onScan,
  onHomeChange,
}: {
  draft: EnvironmentDraft;
  profiles: DiscoverProfilesReport | null;
  busy: boolean;
  update: <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => void;
  onScan: () => void;
  onHomeChange: (value: string) => void;
}) {
  return (
    <div className="setup-wizard__profile">
      <p>Profiles live under <code>&lt;DSH_HOME&gt;/profiles/&lt;name&gt;</code>.</p>
      <div className="setup-wizard__row">
        <input
          type="text"
          value={draft.dshHome}
          placeholder="C:\Users\you\.dsh"
          onChange={(event) => onHomeChange(event.target.value)}
          data-testid="dsh-home"
        />
        <button type="button" onClick={onScan} disabled={busy} data-testid="scan-profiles">
          {busy ? "Scanning…" : "Scan"}
        </button>
      </div>
      {profiles && (
        <ul className="setup-wizard__profiles" data-testid="profile-list">
          {profiles.profiles.length === 0 && (
            <li className="is-empty">No profiles found under this DSH_HOME.</li>
          )}
          {profiles.profiles.map((entry) => (
            <li key={entry.name}>
              <label>
                <input
                  type="radio"
                  name="profile"
                  checked={draft.profile === entry.name}
                  onChange={() => update("profile", entry.name)}
                />
                <code>{entry.name}</code>
                {!entry.hasRootConfig && (
                  <span className="setup-wizard__profile-warning">no cordis.yml</span>
                )}
              </label>
            </li>
          ))}
        </ul>
      )}
      <label className="setup-wizard__new-profile">
        <span>Or create a new profile:</span>
        <input
          type="text"
          value={draft.profile}
          placeholder="my-profile"
          onChange={(event) => update("profile", event.target.value.trim())}
          data-testid="new-profile"
        />
      </label>
    </div>
  );
}

function AdvancedStep({
  draft,
  probe,
  busy,
  update,
  onProbe,
  onPortChange,
}: {
  draft: EnvironmentDraft;
  probe: ProbePortReport | null;
  busy: boolean;
  update: <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => void;
  onProbe: () => void;
  onPortChange: (value: string) => void;
}) {
  return (
    <div className="setup-wizard__advanced">
      <label>
        <span>DSH_HOME</span>
        <input
          type="text"
          value={draft.dshHome}
          onChange={(event) => update("dshHome", event.target.value)}
          data-testid="advanced-home"
        />
      </label>
      <label>
        <span>Endpoint port</span>
        <div className="setup-wizard__row">
          <input
            type="text"
            value={draft.port}
            placeholder="auto"
            onChange={(event) => onPortChange(event.target.value.trim())}
            data-testid="port-input"
          />
          <button type="button" onClick={onProbe} disabled={busy || draft.port === "auto"} data-testid="probe-port">
            {busy ? "Probing…" : "Check"}
          </button>
        </div>
      </label>
      {probe && (
        <p className={"setup-wizard__probe is-" + (probe.inUse ? "busy" : "free")} data-testid="probe-result">
          Port {probe.port} is {probe.inUse ? "already in use" : "free"}.
        </p>
      )}
    </div>
  );
}

function ReviewStep({
  draft,
  validation,
  busy,
  onReview,
}: {
  draft: EnvironmentDraft;
  validation: EnvironmentValidation | null;
  busy: boolean;
  onReview: () => void;
}) {
  return (
    <div className="setup-wizard__review">
      <dl className="setup-wizard__review-summary">
        <dt>Mode</dt>
        <dd>{draft.ownership}</dd>
        <dt>Executable</dt>
        <dd><code>{draft.harnessPath}</code></dd>
        <dt>DSH_HOME</dt>
        <dd><code>{draft.dshHome || "—"}</code></dd>
        <dt>Profile</dt>
        <dd><code>{draft.profile}</code></dd>
        <dt>Port</dt>
        <dd>{draft.port}</dd>
      </dl>
      {!validation && (
        <button type="button" onClick={onReview} disabled={busy} data-testid="run-validation">
          {busy ? "Validating…" : "Validate"}
        </button>
      )}
      {validation && !validation.valid && (
        <ul className="setup-wizard__issues" data-testid="issue-list">
          {validation.issues.map((issue) => (
            <li key={issue.field + ":" + issue.code} className="is-error">
              <code>{issue.field}</code> — {issue.message}
            </li>
          ))}
        </ul>
      )}
      {validation?.valid && (
        <p className="setup-wizard__ok" data-testid="validation-ok">
          Validation passed{validation.launchPreview ? " — ready to launch" : ""}.
        </p>
      )}
    </div>
  );
}

function FinishStep({
  draft,
  validation,
  busy,
  savedRevision,
  launchMessage,
  onFinish,
}: {
  draft: EnvironmentDraft;
  validation: EnvironmentValidation | null;
  busy: boolean;
  savedRevision: number | null;
  launchMessage: string | null;
  onFinish: () => void;
}) {
  return (
    <div className="setup-wizard__finish">
      <p>
        Save <strong>{draft.label || draft.id}</strong> as a {draft.ownership} environment and
        {draft.ownership === "managed" ? " start DSH." : " verify the running DSH."}
      </p>
      <button type="button" onClick={onFinish} disabled={!validation?.valid || busy} data-testid="finish-save">
        {busy ? "Working…" : "Save & launch"}
      </button>
      {savedRevision !== null && (
        <p className="setup-wizard__saved" data-testid="saved-revision">
          Saved at catalog revision {savedRevision}.
        </p>
      )}
      {launchMessage && (
        <p className="setup-wizard__launch" data-testid="launch-message">
          {launchMessage}
        </p>
      )}
    </div>
  );
}
