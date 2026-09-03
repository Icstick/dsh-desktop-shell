// Setup wizard (M7-A, WI-M7-SETUP-WIZARD; D5/WI-A rework): a guided six-step
// environment configuration flow replacing the single-page form.
//
// Steps:
//   1. mode     – Managed (launch locally) | Attached (connect to a running DSH)
//   2. harness  – managed: pick a source-repository directory (repo probe +
//                 clone guidance); legacy executable form kept for editing
//                 old catalog entries / attached records
//   3. profile  – environment name/id + scan $DSH_HOME/profiles, pick or type a profile
//   4. advanced – dshHome confirm + port probe; repository mode also exposes
//                 nodePath / cwd / extraArguments
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
import { useI18n } from "../../../src/i18n";
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

const STEP_KEYS: Record<StepId, string> = {
  mode: "wizard.step.mode",
  harness: "wizard.step.harness",
  profile: "wizard.step.profile",
  advanced: "wizard.step.advanced",
  review: "wizard.step.review",
  finish: "wizard.step.finish",
};

const environmentIdPattern = /^[a-z][a-z0-9-]{1,63}$/;

function deriveId(label: string): string {
  let derived = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  // The catalog id pattern requires a leading letter and 2-64 chars total
  // (^[a-z][a-z0-9-]{1,63}$). Prefix digits/emptiness so a derived id is
  // always valid instead of surfacing a review-time MALFORMED_VALUE.
  if (!/^[a-z]/.test(derived)) derived = "env-" + derived;
  if (derived.length > 64) derived = derived.slice(0, 64);
  if (!/^[a-z]/.test(derived)) derived = "env-dsh";
  return derived;
}

/** Extract a displayable backend message from a rejected promise. */
function backendErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object") {
    const candidate = (error as { message?: unknown }).message;
    if (typeof candidate === "string" && candidate.trim()) return candidate;
  }
  return fallback;
}

export function SetupWizard({ api, initialEnvironment, onSaved }: SetupWizardProps) {
  const { t } = useI18n();
  const [stepIndex, setStepIndex] = useState(0);
  const [draft, setDraft] = useState<EnvironmentDraft>(() =>
    initialEnvironment
      ? environmentToDraft(initialEnvironment)
      : { ...initialEnvironmentDraft, harnessMode: "repository", harnessPath: "" },
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
        includeDshPath: draft.harnessMode === "executable",
        includePath: draft.harnessMode === "executable",
      });
      setDiscovery(report);
      // Auto-pick the first usable candidate of the current source form:
      // repository mode only auto-picks repository candidates.
      const wantedMode = draft.harnessMode;
      const first = report.candidates.find(
        (candidate) =>
          candidate.status === "available" && candidate.launchable && candidate.mode === wantedMode,
      );
      if (first) applyCandidate(first);
    } catch {
      setBackendError(t("wizard.error.discovery"));
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
      setBackendError(t("wizard.error.homeFirst"));
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
      setBackendError(t("wizard.error.profileScan"));
    } finally {
      setBusy(false);
    }
  };

  const runProbe = async () => {
    const parsed = Number(draft.port);
    if (!Number.isInteger(parsed) || parsed < 1024 || parsed > 65535) {
      setBackendError(t("wizard.error.portInvalid"));
      return;
    }
    setBusy(true);
    setBackendError(null);
    try {
      const report = await api.probePort({ schemaVersion: 1, port: parsed });
      setProbe(report);
    } catch {
      setBackendError(t("wizard.error.probe"));
    } finally {
      setBusy(false);
    }
  };

  const review = async () => {
    setBusy(true);
    setBackendError(null);
    const conversion = convertEnvironmentDraft(draft);
    if (!conversion.environment) {
      setBackendError(t("wizard.error.fixFields"));
      setBusy(false);
      return;
    }
    try {
      const result = await api.validateEnvironment(conversion.environment);
      setValidation(result);
      if (!result.valid) {
        setBackendError(t("wizard.error.validation"));
      }
    } catch {
      setBackendError(t("wizard.error.validationBackend"));
    } finally {
      setBusy(false);
    }
  };

  const finish = async () => {
    if (!validation?.valid) return;
    setBusy(true);
    setBackendError(null);
    setLaunchMessage(null);
    let environment: DshEnvironment;
    let catalog: EnvironmentCatalog;
    try {
      environment = convertEnvironmentDraft(draft).environment!;
      // ID-collision guard: the catalog upserts by id, so a fresh wizard
      // must never silently overwrite an existing environment that happens
      // to share the derived id (e.g. two "Local DSH" entries).
      const existingCatalog = await api.getEnvironmentCatalog();
      const taken = existingCatalog.environments.some(
        (entry) => entry.id === environment.id && entry.id !== initialEnvironment?.id,
      );
      if (taken) {
        setBackendError(t("wizard.error.idTaken"));
        setBusy(false);
        return;
      }
      catalog = await api.saveEnvironment(environment);
    } catch (error) {
      // Save failure: nothing was persisted; report the backend detail.
      setBackendError(backendErrorMessage(error, t("wizard.error.save")));
      setBusy(false);
      return;
    }
    setSavedRevision(catalog.revision);
    onSaved(environment, catalog, validation);

    if (environment.ownership === "managed") {
      if (repoUnready(environment)) {
        // WI-C progressive recovery is not implemented yet: starting a
        // checkout without dependencies/web assets would fail right away.
        // Save the environment but do not auto-start it.
        setLaunchMessage(t("wizard.finish.launch.repoUnready"));
      } else if (
        environment.harness.mode === "repository" &&
        environment.endpoint.port === "auto"
      ) {
        // Modern DSH builds print no readiness marker: auto port cannot be
        // located, so the start would always time out. Guide the user to a
        // fixed port instead of failing with a generic runtime error.
        setLaunchMessage(t("wizard.finish.launch.repoAutoPort"));
      } else {
        try {
          await api.startManagedEnvironment({
            schemaVersion: 1,
            environmentId: environment.id,
          });
          setLaunchMessage(t("wizard.finish.launch.managed"));
        } catch (error) {
          // The environment was saved; only the launch step failed.
          setBackendError(backendErrorMessage(error, t("wizard.error.launch")));
        }
      }
    } else if (environment.endpoint.port === "auto") {
      // Attached verification needs a concrete port; the save itself is fine.
      setLaunchMessage(t("wizard.finish.launch.attachedAuto"));
    } else {
      try {
        const health = await api.probeAttachedEnvironment({
          schemaVersion: 1,
          environmentId: environment.id,
        });
        setLaunchMessage(
          health.state === "attached"
            ? t("wizard.finish.launch.attachedOk")
            : t("wizard.finish.launch.attachedMiss"),
        );
      } catch (error) {
        setBackendError(backendErrorMessage(error, t("wizard.error.attachProbe")));
      }
    }
    setBusy(false);
  };

  // The profile id is always machine-generated: derived from the profile
  // name for fresh environments, kept as-is when editing an existing one.
  // True when the selected repository candidate still needs install/build
  // (probed by discovery). WI-C progressive recovery is not implemented, so
  // a checkout without node_modules / web assets cannot launch yet.
  const repoUnready = (environment: DshEnvironment): boolean => {
    if (environment.harness.mode !== "repository") return false;
    const candidate = (discovery?.candidates ?? []).find(
      (entry) =>
        entry.mode === "repository" &&
        entry.status === "available" &&
        entry.launchable &&
        (entry.canonicalPath ?? entry.requestedPath) === environment.harness.path,
    );
    return !!candidate?.repository &&
      (candidate.repository.needsInstall || candidate.repository.needsBuild);
  };

  const onLabelChange = (label: string) => {
    update("label", label);
    if (!initialEnvironment) {
      const derived = deriveId(label);
      if (derived) update("id", derived);
    }
  };

  const browseDirectory = async (target: "harness" | "home") => {
    setBusy(true);
    setBackendError(null);
    try {
      const picked = await api.pickDirectory();
      if (picked) update(target === "harness" ? "harnessPath" : "dshHome", picked);
    } catch {
      setBackendError(t("wizard.error.browse"));
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
            onBrowse={() => browseDirectory("harness")}
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
            onLabelChange={onLabelChange}
            onBrowseHome={() => browseDirectory("home")}
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
    (step === "profile" &&
      draft.profile.trim().length > 0 &&
      draft.label.trim().length > 0 &&
      draft.id.trim().length > 0) ||
    (step === "advanced" && draft.dshHome.trim().length > 0) ||
    (step === "review" && validation?.valid === true);

  return (
    <div className="setup-wizard" data-testid="setup-wizard">
      <ol className="setup-wizard__steps" aria-label={t("wizard.aria.steps")}>
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
              {t(STEP_KEYS[id])}
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
          {t("wizard.nav.back")}
        </button>
        {stepIndex < STEPS.length - 1 ? (
          <button
            type="button"
            className="setup-wizard__next"
            onClick={next}
            disabled={!canNext || busy}
            data-testid="wizard-next"
          >
            {t("wizard.nav.next")}
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
  const { t } = useI18n();
  const choose = (ownership: BackendOwnership) => {
    update("ownership", ownership);
    // Managed sources are source repositories (D5); the legacy executable
    // form stays only for editing old catalog entries. Attached records keep
    // the executable placeholder form (path is informational).
    update("harnessMode", ownership === "managed" ? "repository" : "executable");
  };
  return (
    <fieldset className="setup-wizard__mode">
      <legend>{t("wizard.mode.legend")}</legend>
      <label className={"setup-wizard__mode-card" + (draft.ownership === "managed" ? " is-selected" : "")}>
        <input
          type="radio"
          name="ownership"
          checked={draft.ownership === "managed"}
          onChange={() => choose("managed")}
        />
        <strong>{t("wizard.mode.managed")}</strong>
        <span>{t("wizard.mode.managed.desc")}</span>
      </label>
      <label className={"setup-wizard__mode-card" + (draft.ownership === "attached" ? " is-selected" : "")}>
        <input
          type="radio"
          name="ownership"
          checked={draft.ownership === "attached"}
          onChange={() => choose("attached")}
        />
        <strong>{t("wizard.mode.attached")}</strong>
        <span>{t("wizard.mode.attached.desc")}</span>
      </label>
    </fieldset>
  );
}

function HarnessStep({
  draft,
  discovery,
  busy,
  update,
  onBrowse,
  onDiscover,
  onPick,
}: {
  draft: EnvironmentDraft;
  discovery: HarnessDiscoveryReport | null;
  busy: boolean;
  update: <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => void;
  onBrowse: () => void;
  onDiscover: () => void;
  onPick: (candidate: HarnessCandidate) => void;
}) {
  if (draft.harnessMode === "repository") {
    return (
      <RepositorySourceStep
        draft={draft}
        discovery={discovery}
        busy={busy}
        update={update}
        onBrowse={onBrowse}
        onDiscover={onDiscover}
        onPick={onPick}
      />
    );
  }
  return (
    <LegacyExecutableStep
      draft={draft}
      discovery={discovery}
      busy={busy}
      update={update}
      onDiscover={onDiscover}
      onPick={onPick}
    />
  );
}

// Repository form (D5): the single managed source shape.
function RepositorySourceStep({
  draft,
  discovery,
  busy,
  update,
  onBrowse,
  onDiscover,
  onPick,
}: {
  draft: EnvironmentDraft;
  discovery: HarnessDiscoveryReport | null;
  busy: boolean;
  update: <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => void;
  onBrowse: () => void;
  onDiscover: () => void;
  onPick: (candidate: HarnessCandidate) => void;
}) {
  const { t } = useI18n();
  const repoCandidates = (discovery?.candidates ?? []).filter(
    (candidate) => candidate.mode === "repository",
  );
  const usable = repoCandidates.find((candidate) => candidate.status === "available" && candidate.launchable);
  const pickedPath = usable?.canonicalPath ?? usable?.requestedPath;
  const target = draft.harnessPath.trim();
  const targetHint = target
    ? t("wizard.clone.command", { target })
    : t("wizard.clone.enterTarget");

  const renderCloneGuide = () => (
    <div className="setup-wizard__clone" data-testid="clone-guide">
      <h3>{t("wizard.clone.title")}</h3>
      <p>{t("wizard.clone.body")}</p>
      <code className="setup-wizard__clone-command">{targetHint}</code>
    </div>
  );

  return (
    <div className="setup-wizard__harness">
      <p className="setup-wizard__intro">{t("wizard.source.intro")}</p>
      <div className="setup-wizard__row">
        <input
          type="text"
          value={draft.harnessPath}
          placeholder={t("wizard.source.placeholder")}
          onChange={(event) => update("harnessPath", event.target.value)}
          data-testid="harness-path"
        />
        <button
          type="button"
          className="setup-wizard__browse"
          onClick={onBrowse}
          disabled={busy}
          data-testid="browse-directory"
        >
          {t("wizard.browse")}
        </button>
        <button type="button" onClick={onDiscover} disabled={busy} data-testid="discover-button">
          {busy ? t("wizard.source.checking") : t("wizard.source.check")}
        </button>
      </div>

      {discovery && repoCandidates.length === 0 && (
        <ul className="setup-wizard__candidates" data-testid="candidate-list">
          {(discovery.candidates ?? []).some((candidate) => candidate.mode === "executable") ? (
            <li className="is-empty">{t("wizard.source.fileCandidate")}</li>
          ) : (
            <li className="is-empty">{t("wizard.source.none")}</li>
          )}
        </ul>
      )}

      {repoCandidates.map((candidate) => (
        <div
          key={candidate.id}
          className={
            "setup-wizard__repo" +
            (candidate.status === "available" && candidate.launchable ? " is-usable" : " is-broken")
          }
          data-testid={"repo-candidate-" + candidate.id}
        >
          <label className="setup-wizard__repo-select">
            <input
              type="radio"
              name="candidate"
              checked={pickedPath === (candidate.canonicalPath ?? candidate.requestedPath)}
              disabled={candidate.status !== "available" || !candidate.launchable}
              onChange={() => onPick(candidate)}
            />
            <code>{candidate.canonicalPath ?? candidate.requestedPath}</code>
            {candidate.status === "available" && candidate.launchable && (
              <span className="setup-wizard__badge is-ok">{t("wizard.source.repo")}</span>
            )}
          </label>
          {candidate.status === "available" && candidate.launchable && candidate.repository && (
            <dl className="setup-wizard__repo-details">
              <div>
                <dt>{t("wizard.source.entry")}</dt>
                <dd>
                  <code>{candidate.repository.entry}</code>
                </dd>
              </div>
              <div>
                <dt>{t("wizard.source.loader")}</dt>
                <dd>
                  {candidate.repository.loader ? (
                    <code>{candidate.repository.loader}</code>
                  ) : (
                    <span aria-hidden="true">—</span>
                  )}
                </dd>
              </div>
              <div>
                <dt>{t("wizard.source.version")}</dt>
                <dd>{candidate.version ?? "—"}</dd>
              </div>
              <div>
                <dt>{t("wizard.source.install")}</dt>
                <dd>
                  <span
                    className={
                      "setup-wizard__badge " +
                      (candidate.repository.needsInstall ? "is-warn" : "is-ok")
                    }
                  >
                    {candidate.repository.needsInstall
                      ? t("wizard.source.installMissing")
                      : t("wizard.source.installReady")}
                  </span>
                </dd>
              </div>
              <div>
                <dt>{t("wizard.source.build")}</dt>
                <dd>
                  <span
                    className={
                      "setup-wizard__badge " +
                      (candidate.repository.needsBuild ? "is-warn" : "is-ok")
                    }
                  >
                    {candidate.repository.needsBuild
                      ? t("wizard.source.buildMissing")
                      : t("wizard.source.buildReady")}
                  </span>
                </dd>
              </div>
            </dl>
          )}
          {candidate.evidence.map((item) => (
            <p
              key={item.code}
              className={"setup-wizard__evidence is-" + item.severity}
              data-testid={"evidence-" + item.code}
            >
              {item.message}
            </p>
          ))}
        </div>
      ))}

      {!usable && !discovery && !target && renderCloneGuide()}
      {!usable && !discovery && target && (
        <p className="setup-wizard__evidence is-info" data-testid="probe-first">
          {t("wizard.source.probeFirst")}
        </p>
      )}
      {!usable && discovery && renderCloneGuide()}
    </div>
  );
}

// Legacy executable form: editing old catalog entries or attached records.
function LegacyExecutableStep({
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
  const { t } = useI18n();
  return (
    <div className="setup-wizard__harness">
      {draft.ownership === "managed" && (
        <>
          <p className="setup-wizard__legacy-badge">{t("wizard.source.legacyBadge")}</p>
          <p className="setup-wizard__intro">{t("wizard.source.legacyNote")}</p>
        </>
      )}
      <div className="setup-wizard__row">
        <input
          type="text"
          value={draft.harnessPath}
          placeholder={t("wizard.source.placeholder")}
          onChange={(event) => update("harnessPath", event.target.value)}
          data-testid="harness-path"
        />
        <button type="button" onClick={onDiscover} disabled={busy} data-testid="discover-button">
          {busy ? t("wizard.source.checking") : t("wizard.source.check")}
        </button>
      </div>
      {discovery && (
        <ul className="setup-wizard__candidates" data-testid="candidate-list">
          {discovery.candidates.length === 0 && <li className="is-empty">{t("wizard.source.none")}</li>}
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
  onLabelChange,
  onBrowseHome,
  onScan,
  onHomeChange,
}: {
  draft: EnvironmentDraft;
  profiles: DiscoverProfilesReport | null;
  busy: boolean;
  update: <Key extends keyof EnvironmentDraft>(key: Key, value: EnvironmentDraft[Key]) => void;
  onLabelChange: (value: string) => void;
  onBrowseHome: () => void;
  onScan: () => void;
  onHomeChange: (value: string) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="setup-wizard__profile">
      <div className="setup-wizard__identity">
        <label className="field">
          <span>{t("wizard.identity.label")}</span>
          <input
            type="text"
            value={draft.label}
            placeholder={t("wizard.identity.labelPlaceholder")}
            onChange={(event) => onLabelChange(event.target.value)}
            data-testid="env-label"
          />
          <small data-testid="env-id-auto">
            {t("wizard.identity.idAuto", { id: draft.id })}
          </small>
        </label>
      </div>

      <p>{t("wizard.profile.intro")}</p>
      <div className="setup-wizard__row">
        <input
          type="text"
          value={draft.dshHome}
          placeholder={t("wizard.profile.homePlaceholder")}
          onChange={(event) => onHomeChange(event.target.value)}
          data-testid="dsh-home"
        />
        <button
          type="button"
          className="setup-wizard__browse"
          onClick={onBrowseHome}
          disabled={busy}
          data-testid="browse-home"
        >
          {t("wizard.browse")}
        </button>
        <button type="button" onClick={onScan} disabled={busy} data-testid="scan-profiles">
          {busy ? t("wizard.profile.scanning") : t("wizard.profile.scan")}
        </button>
      </div>
      {profiles && (
        <ul className="setup-wizard__profiles" data-testid="profile-list">
          {profiles.profiles.length === 0 && (
            <li className="is-empty">{t("wizard.profile.none")}</li>
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
                  <span className="setup-wizard__profile-warning">{t("wizard.profile.noConfig")}</span>
                )}
              </label>
            </li>
          ))}
        </ul>
      )}
      <label className="setup-wizard__new-profile">
        <span>{t("wizard.profile.new")}</span>
        <input
          type="text"
          value={draft.profile}
          placeholder={t("wizard.profile.namePlaceholder")}
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
  const { t } = useI18n();
  const repositoryMode = draft.harnessMode === "repository";
  return (
    <div className="setup-wizard__advanced">
      <label className="field">
        <span>{t("wizard.advanced.port")}</span>
        <div className="setup-wizard__row">
          <input
            type="text"
            value={draft.port}
            placeholder={t("wizard.advanced.portPlaceholder")}
            onChange={(event) => onPortChange(event.target.value.trim())}
            data-testid="port-input"
          />
          <button type="button" onClick={onProbe} disabled={busy || draft.port === "auto"} data-testid="probe-port">
            {busy ? t("wizard.advanced.checking") : t("wizard.advanced.check")}
          </button>
        </div>
        <small>
          {draft.ownership === "attached"
            ? t("wizard.advanced.portAttachedHint")
            : t("wizard.advanced.portHint")}
        </small>
      </label>
      {probe && (
        <p className={"setup-wizard__probe is-" + (probe.inUse ? "busy" : "free")} data-testid="probe-result">
          {probe.inUse
            ? t("wizard.advanced.portBusy", { port: String(probe.port) })
            : t("wizard.advanced.portFree", { port: String(probe.port) })}
        </p>
      )}
      {repositoryMode && (
        <div className="setup-wizard__repo-options">
          <label className="field">
            <span>{t("wizard.advanced.nodePath")}</span>
            <input
              type="text"
              value={draft.nodePath}
              placeholder="node"
              onChange={(event) => update("nodePath", event.target.value)}
              data-testid="node-path"
            />
            <small>{t("wizard.advanced.nodePathHint")}</small>
          </label>
          <label className="field">
            <span>{t("wizard.advanced.cwd")}</span>
            <input
              type="text"
              value={draft.cwd}
              placeholder={draft.harnessPath}
              onChange={(event) => update("cwd", event.target.value)}
              data-testid="cwd-input"
            />
            <small>{t("wizard.advanced.cwdHint")}</small>
          </label>
          <label className="field">
            <span>{t("wizard.advanced.args")}</span>
            <textarea
              rows={3}
              value={draft.extraArguments}
              onChange={(event) => update("extraArguments", event.target.value)}
              data-testid="extra-args"
            />
            <small>{t("wizard.advanced.argsHint")}</small>
          </label>
        </div>
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
  const { t } = useI18n();
  return (
    <div className="setup-wizard__review">
      <dl className="setup-wizard__review-summary">
        <dt>{t("wizard.review.mode")}</dt>
        <dd>{draft.ownership}</dd>
        <dt>{t("wizard.review.source")}</dt>
        <dd>
          <code>{draft.harnessPath}</code>
        </dd>
        {draft.cwd.trim() && (
          <>
            <dt>{t("wizard.review.cwd")}</dt>
            <dd>
              <code>{draft.cwd}</code>
            </dd>
          </>
        )}
        {draft.nodePath.trim() && (
          <>
            <dt>{t("wizard.review.node")}</dt>
            <dd>
              <code>{draft.nodePath}</code>
            </dd>
          </>
        )}
        <dt>{t("wizard.review.home")}</dt>
        <dd>
          <code>{draft.dshHome || "—"}</code>
        </dd>
        <dt>{t("wizard.review.profile")}</dt>
        <dd>
          <code>{draft.profile}</code>
        </dd>
        <dt>{t("wizard.review.port")}</dt>
        <dd>{draft.port}</dd>
      </dl>
      {!validation && (
        <button type="button" onClick={onReview} disabled={busy} data-testid="run-validation">
          {busy ? t("wizard.review.validating") : t("wizard.review.validate")}
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
          {validation.launchPreview
            ? t("wizard.review.passed") + " · " + t("wizard.review.ready")
            : t("wizard.review.passed")}
          .
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
  const { t } = useI18n();
  const name = draft.label.trim() || draft.id || "—";
  return (
    <div className="setup-wizard__finish">
      <p>
        {draft.ownership === "managed"
          ? t("wizard.finish.saveManaged", { label: name })
          : t("wizard.finish.saveAttached", { label: name })}
      </p>
      <button
        type="button"
        className="setup-wizard__finish-action"
        onClick={onFinish}
        disabled={!validation?.valid || busy}
        data-testid="finish-save"
      >
        {busy ? t("wizard.finish.working") : t("wizard.finish.action")}
      </button>
      {savedRevision !== null && (
        <p className="setup-wizard__saved" data-testid="saved-revision">
          {t("wizard.finish.savedAt", { revision: String(savedRevision) })}
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
