import { useCallback, useEffect, useRef, useState } from "react";

import type {
  AttachedHealthReport,
  DesktopCommandError,
  DiagnosticsReport,
  DshEnvironment,
  DshSurfaceBounds,
  DshSurfacePolicy,
  DshSurfaceStatus,
  EnvironmentCatalog,
  EnvironmentValidation,
  ManagedRuntimeReport,
  NotificationReport,
  ShellSnapshot,
  TerminalReport,
  UsageSnapshot,
} from "../../../src/contracts";
import { desktopApi, type DesktopApi } from "../../../src/desktop-api";
import { useI18n } from "../../../src/i18n";
import { EnvironmentList } from "../../environment-settings/src/EnvironmentList";
import { SetupWizard } from "../../environment-settings/src/SetupWizard";
import { BrowserPanel } from "../../browser-ui/src/BrowserPanel";
import { HarnessSurface } from "../../harness-surface/src/HarnessSurface";
import { TerminalPanel } from "../../terminal-ui/src/TerminalPanel";
import { ActivityRail, type SurfaceId } from "./ActivityRail";

interface ShellAppProps {
  api?: DesktopApi;
}

export function ShellApp({ api = desktopApi }: ShellAppProps) {
  const { t } = useI18n();
  const [activeSurface, setActiveSurface] = useState<SurfaceId>("dsh");
  const [snapshot, setSnapshot] = useState<ShellSnapshot | null>(null);
  const [snapshotError, setSnapshotError] = useState<string | null>(null);
  const [validatedEnvironment, setValidatedEnvironment] = useState<DshEnvironment | null>(null);
  const [catalog, setCatalog] = useState<EnvironmentCatalog | null>(null);
  const [validation, setValidation] = useState<EnvironmentValidation | null>(null);
  const [attachedHealth, setAttachedHealth] = useState<AttachedHealthReport | null>(null);
  const [attachedHealthError, setAttachedHealthError] = useState<string | null>(null);
  const [probingAttached, setProbingAttached] = useState(false);
  const [probeRevision, setProbeRevision] = useState(0);
  const [surfacePolicy, setSurfacePolicy] = useState<DshSurfacePolicy | null>(null);
  const [surfacePolicyError, setSurfacePolicyError] = useState<string | null>(null);
  const [managedRuntime, setManagedRuntime] = useState<ManagedRuntimeReport | null>(null);
  const [managedRuntimeError, setManagedRuntimeError] = useState<string | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsReport | null>(null);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);
  const [transitioningManaged, setTransitioningManaged] = useState(false);
  const [confirmingManagedStop, setConfirmingManagedStop] = useState(false);
  const [terminalSession, setTerminalSession] = useState<TerminalReport | null>(null);
  const [surfaceBounds, setSurfaceBounds] = useState<DshSurfaceBounds | null>(null);
  const [nativeSurface, setNativeSurface] = useState<DshSurfaceStatus | null>(null);
  const [nativeSurfaceError, setNativeSurfaceError] = useState<string | null>(null);
  const [retryingSurface, setRetryingSurface] = useState(false);
  const mountedSurfaceRef = useRef<{ environmentId: string; generation: number } | null>(null);
  const lastSurfaceBoundsRef = useRef<DshSurfaceBounds | null>(null);
  const surfaceIntentRef = useRef<string | null>(null);
  const surfaceFailureRef = useRef<string | null>(null);
  const tRef = useRef(t);
  tRef.current = t;

  const handleSurfaceBoundsChange = useCallback((next: DshSurfaceBounds | null) => {
    if (next) lastSurfaceBoundsRef.current = next;
    setSurfaceBounds((current) => (surfaceBoundsEqual(current, next) ? current : next));
  }, []);

  useEffect(() => {
    let current = true;
    const load = async () => {
      try {
        const [nextSnapshot, catalog] = await Promise.all([
          api.getShellSnapshot(),
          api.getEnvironmentCatalog(),
        ]);
        if (!current) return;
        setSnapshot(nextSnapshot);
        setCatalog(catalog);

        const activeEnvironment = catalog.environments.find(
          (environment) => environment.id === catalog.activeEnvironmentId,
        );
        if (!activeEnvironment) return;

        const nextValidation = await api.validateEnvironment(activeEnvironment);
        if (current && nextValidation.valid) {
          setValidatedEnvironment(activeEnvironment);
          setValidation(nextValidation);
        }
      } catch {
        if (current) setSnapshotError(tRef.current("error.desktopUnavailable"));
      }
    };
    void load();
    return () => {
      current = false;
    };
  }, [api]);

  useEffect(() => {
    if (validatedEnvironment?.ownership !== "attached") {
      setAttachedHealth(null);
      setAttachedHealthError(null);
      setProbingAttached(false);
      return;
    }

    let current = true;
    setProbingAttached(true);
    setAttachedHealthError(null);
    api
      .probeAttachedEnvironment({
        schemaVersion: 1,
        environmentId: validatedEnvironment.id,
      })
      .then((report) => {
        if (!current) return;
        setAttachedHealth(report);
        setSnapshot((snapshot) =>
          snapshot ? { ...snapshot, runtimeState: report.state } : snapshot,
        );
      })
      .catch((error: unknown) => {
        if (!current) return;
        setAttachedHealth(null);
        setAttachedHealthError(
          commandErrorMessage(error, tRef.current("error.attachedUnavailable")),
        );
        setSnapshot((snapshot) =>
          snapshot ? { ...snapshot, runtimeState: "unavailable" } : snapshot,
        );
      })
      .finally(() => current && setProbingAttached(false));

    return () => {
      current = false;
    };
  }, [api, probeRevision, validatedEnvironment]);

  useEffect(() => {
    if (validatedEnvironment?.ownership !== "managed") {
      setManagedRuntime(null);
      setManagedRuntimeError(null);
      setTransitioningManaged(false);
      setConfirmingManagedStop(false);
      return;
    }

    let current = true;
    setManagedRuntime(null);
    setManagedRuntimeError(null);
    setTransitioningManaged(false);
    setConfirmingManagedStop(false);
    api
      .getManagedRuntimeStatus({
        schemaVersion: 1,
        environmentId: validatedEnvironment.id,
      })
      .then((report) => {
        if (!current) return;
        setManagedRuntime(report);
        setSnapshot((snapshot) =>
          snapshot
            ? { ...snapshot, runtimeState: report.state, generation: report.generation }
            : snapshot,
        );
      })
      .catch((error: unknown) => {
        if (!current) return;
        setManagedRuntime(null);
        setManagedRuntimeError(
          commandErrorMessage(error, tRef.current("error.managedUnavailable")),
        );
      });

    return () => {
      current = false;
    };
  }, [api, validatedEnvironment]);

  useEffect(() => {
    if (validatedEnvironment?.ownership !== "managed") {
      setDiagnostics(null);
      setDiagnosticsError(null);
      return;
    }
    let current = true;
    setDiagnostics(null);
    setDiagnosticsError(null);
    api
      .getDiagnostics({
        schemaVersion: 1,
        environmentId: validatedEnvironment.id,
      })
      .then((report) => {
        if (!current) return;
        setDiagnostics(report);
      })
      .catch((error: unknown) => {
        if (!current) return;
        setDiagnostics(null);
        setDiagnosticsError(
          commandErrorMessage(error, tRef.current("error.diagnosticsUnavailable")),
        );
      });
    return () => {
      current = false;
    };
  }, [api, managedRuntime, validatedEnvironment]);

  useEffect(() => {
    let current = true;
    const environment = validatedEnvironment;
    const runtimeVerified =
      environment?.ownership === "managed" &&
      managedRuntime?.environmentId === environment.id &&
      managedRuntime.state === "healthy" &&
      managedRuntime.readiness === "verified" &&
      managedRuntime.processOwnership === "owned" &&
      managedRuntime.endpoint !== null &&
      managedRuntime.generation > 0;
    const bindingKey = runtimeVerified && environment && managedRuntime
      ? `${environment.id}:${managedRuntime.generation}`
      : null;

    const reconcile = async () => {
      if (surfaceFailureRef.current && surfaceFailureRef.current !== bindingKey) {
        surfaceFailureRef.current = null;
      }
      const mounted = mountedSurfaceRef.current;
      const bindingChanged =
        mounted !== null &&
        (!runtimeVerified ||
          mounted.environmentId !== environment?.id ||
          mounted.generation !== managedRuntime?.generation);

      if (bindingChanged && mounted) {
        try {
          await api.unmountDshSurface({
            schemaVersion: 1,
            environmentId: mounted.environmentId,
            expectedGeneration: mounted.generation,
          });
        } catch {
          // Runtime stop and generation replacement also force-unmount natively.
        }
        if (!current) return;
        mountedSurfaceRef.current = null;
        surfaceIntentRef.current = null;
        surfaceFailureRef.current = null;
        setNativeSurface(null);
      }

      if (!runtimeVerified || !environment || !managedRuntime) {
        if (current) setNativeSurfaceError(null);
        return;
      }
      if (surfaceFailureRef.current === bindingKey) return;

      const visible = activeSurface === "dsh" && surfaceBounds !== null;
      const bounds = surfaceBounds ?? lastSurfaceBoundsRef.current;
      if (!bounds) return;

      const intent = [
        environment.id,
        managedRuntime.generation,
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        visible,
      ].join(":");
      if (surfaceIntentRef.current === intent) return;
      surfaceIntentRef.current = intent;

      try {
        const request = {
          schemaVersion: 1 as const,
          environmentId: environment.id,
          expectedGeneration: managedRuntime.generation,
          bounds,
          visible,
        };
        const report = mountedSurfaceRef.current
          ? await api.updateDshSurfaceLayout(request)
          : await api.mountDshSurface(request);
        if (!current) return;
        mountedSurfaceRef.current = {
          environmentId: environment.id,
          generation: managedRuntime.generation,
        };
        setNativeSurface(report);
        setNativeSurfaceError(null);
        surfaceFailureRef.current = null;
      } catch (error: unknown) {
        if (!current) return;
        surfaceFailureRef.current = bindingKey;
        setNativeSurfaceError(
          commandErrorMessage(error, tRef.current("error.nativeSurfaceUnavailable")),
        );
      }
    };

    void reconcile();
    return () => {
      current = false;
    };
  }, [activeSurface, api, managedRuntime, nativeSurface?.state, surfaceBounds, validatedEnvironment]);

  useEffect(() => {
    const mounted = mountedSurfaceRef.current;
    if (!mounted || !nativeSurface) return;
    if (!["mounting", "loading", "ready", "hidden"].includes(nativeSurface.state)) return;

    let current = true;
    const interval = nativeSurface.state === "mounting" || nativeSurface.state === "loading"
      ? 300
      : 1_500;
    const timer = window.setTimeout(() => {
      api
        .getDshSurfaceStatus({
          schemaVersion: 1,
          environmentId: mounted.environmentId,
          expectedGeneration: mounted.generation,
        })
        .then(async (report) => {
          if (!current) return;
          if (report.state === "error" && report.bounds && report.visible) {
            try {
              report = await api.updateDshSurfaceLayout({
                schemaVersion: 1,
                environmentId: mounted.environmentId,
                expectedGeneration: mounted.generation,
                bounds: report.bounds,
                visible: false,
              });
            } catch {
              // The lifecycle error is still the primary evidence shown to the user.
            }
          }
          if (!current) return;
          setNativeSurface(report);
          setNativeSurfaceError(null);
          if (report.state === "stale" || report.state === "unmounted") {
            mountedSurfaceRef.current = null;
            surfaceIntentRef.current = null;
          }
        })
        .catch(async (error: unknown) => {
          if (!current) return;
          try {
            await api.unmountDshSurface({
              schemaVersion: 1,
              environmentId: mounted.environmentId,
              expectedGeneration: mounted.generation,
            });
          } catch {
            // The retained identity is still cleared so a user retry must mount afresh.
          }
          if (!current) return;
          mountedSurfaceRef.current = null;
          surfaceFailureRef.current = `${mounted.environmentId}:${mounted.generation}`;
          setNativeSurface(null);
          setNativeSurfaceError(
            commandErrorMessage(error, tRef.current("error.surfaceStatusUnavailable")),
          );
        });
    }, interval);

    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [api, nativeSurface]);

  useEffect(
    () => () => {
      const mounted = mountedSurfaceRef.current;
      if (!mounted) return;
      void api.unmountDshSurface({
        schemaVersion: 1,
        environmentId: mounted.environmentId,
        expectedGeneration: mounted.generation,
      });
    },
    [api],
  );

  useEffect(() => {
    if (!validatedEnvironment) {
      setSurfacePolicy(null);
      setSurfacePolicyError(null);
      return;
    }

    let current = true;
    setSurfacePolicy(null);
    setSurfacePolicyError(null);
    api
      .getDshSurfacePolicy({
        schemaVersion: 1,
        environmentId: validatedEnvironment.id,
      })
      .then((policy) => {
        if (current) setSurfacePolicy(policy);
      })
      .catch((error: unknown) => {
        if (!current) return;
        setSurfacePolicyError(
          commandErrorMessage(error, tRef.current("error.surfacePolicyUnavailable")),
        );
      });

    return () => {
      current = false;
    };
  }, [api, validatedEnvironment]);

  const handleSaved = async (
    environment: DshEnvironment,
    catalog: EnvironmentCatalog,
    result: EnvironmentValidation,
  ) => {
    setValidatedEnvironment(environment);
    setValidation(result);
    setCatalog(catalog);
    setSnapshot((current) =>
      current
        ? {
            ...current,
            environmentId: catalog.activeEnvironmentId,
            runtimeState: environment.ownership === "managed" ? "stopped" : "unavailable",
          }
        : current,
    );
    if (environment.ownership === "attached") return;

    try {
      setSnapshot(await api.getShellSnapshot());
      setSnapshotError(null);
    } catch {
      setSnapshotError(tRef.current("error.savedRefresh"));
    }
  };

  const activateEnvironment = async (
    nextCatalog: EnvironmentCatalog,
    environment: DshEnvironment,
  ) => {
    const previous = validatedEnvironment;
    // Single-active semantics, ordered stop → activate → start (the
    // documented B1 sequence; REVIEW-M7 HIGH-1): stop the currently
    // running managed environment first (explicit previous environment —
    // closures would otherwise read a stale value after the state update),
    // then persist the activation, then start the target.
    if (
      previous?.ownership === "managed" &&
      previous.id !== environment.id &&
      managedRuntime &&
      managedRuntime.environmentId === previous.id &&
      managedRuntime.generation >= 1 &&
      managedRuntime.state === "healthy"
    ) {
      await stopManaged(previous);
    }
    setCatalog(nextCatalog);
    setValidatedEnvironment(environment);
    setValidation(null);
    if (environment.ownership === "attached") {
      setSnapshot((current) =>
        current ? { ...current, environmentId: environment.id } : current,
      );
      return;
    }
    setSnapshot((current) =>
      current ? { ...current, environmentId: environment.id, runtimeState: "stopped" } : current,
    );
    await startManaged(environment);
  };

  const applyManagedReport = (report: ManagedRuntimeReport) => {
    setManagedRuntime(report);
    setSnapshot((snapshot) =>
      snapshot
        ? { ...snapshot, runtimeState: report.state, generation: report.generation }
        : snapshot,
    );
  };

  const startManaged = async (environment?: DshEnvironment) => {
    // The button wiring may pass a DOM event; only a real environment
    // (carrying ownership) is accepted as the explicit target.
    const target = environment?.ownership ? environment : validatedEnvironment;
    if (!target || target.ownership !== "managed") return;
    setTransitioningManaged(true);
    setManagedRuntimeError(null);
    setConfirmingManagedStop(false);
    try {
      applyManagedReport(
        await api.startManagedEnvironment({
          schemaVersion: 1,
          environmentId: target.id,
        }),
      );
    } catch (error: unknown) {
      setManagedRuntimeError(commandErrorMessage(error, tRef.current("error.managedStart")));
    } finally {
      setTransitioningManaged(false);
    }
  };

  const stopManaged = async (environment?: DshEnvironment) => {
    const target = environment?.ownership ? environment : validatedEnvironment;
    if (
      !target ||
      target.ownership !== "managed" ||
      !managedRuntime ||
      managedRuntime.environmentId !== target.id ||
      managedRuntime.generation < 1
    ) {
      return;
    }
    setTransitioningManaged(true);
    setManagedRuntimeError(null);
    try {
      applyManagedReport(
        await api.stopManagedEnvironment({
          schemaVersion: 1,
          environmentId: target.id,
          expectedGeneration: managedRuntime.generation,
        }),
      );
      mountedSurfaceRef.current = null;
      surfaceIntentRef.current = null;
      surfaceFailureRef.current = null;
      setNativeSurface(null);
      setNativeSurfaceError(null);
      setConfirmingManagedStop(false);
    } catch (error: unknown) {
      setManagedRuntimeError(commandErrorMessage(error, tRef.current("error.managedStop")));
    } finally {
      setTransitioningManaged(false);
    }
  };

  const restartManaged = async () => {
    if (
      validatedEnvironment?.ownership !== "managed" ||
      !managedRuntime ||
      managedRuntime.environmentId !== validatedEnvironment.id ||
      managedRuntime.generation < 1
    ) {
      return;
    }
    setTransitioningManaged(true);
    setManagedRuntimeError(null);
    setConfirmingManagedStop(false);
    try {
      applyManagedReport(
        await api.restartManagedEnvironment({
          schemaVersion: 1,
          environmentId: validatedEnvironment.id,
          expectedGeneration: managedRuntime.generation,
        }),
      );
      mountedSurfaceRef.current = null;
      surfaceIntentRef.current = null;
      surfaceFailureRef.current = null;
      setNativeSurface(null);
      setNativeSurfaceError(null);
    } catch (error: unknown) {
      setManagedRuntimeError(commandErrorMessage(error, tRef.current("error.managedRestart")));
    } finally {
      setTransitioningManaged(false);
    }
  };

  const retryNativeSurface = async () => {
    const environment = validatedEnvironment;
    const bounds = lastSurfaceBoundsRef.current;
    if (
      environment?.ownership !== "managed" ||
      !bounds ||
      !managedRuntime ||
      managedRuntime.environmentId !== environment.id ||
      managedRuntime.state !== "healthy" ||
      managedRuntime.readiness !== "verified" ||
      managedRuntime.processOwnership !== "owned" ||
      managedRuntime.generation < 1
    ) {
      return;
    }

    setRetryingSurface(true);
    setNativeSurfaceError(null);
    surfaceFailureRef.current = null;
    const identity = {
      schemaVersion: 1 as const,
      environmentId: environment.id,
      expectedGeneration: managedRuntime.generation,
    };
    try {
      let report: DshSurfaceStatus;
      if (nativeSurface?.state === "error") {
        try {
          report = await api.reloadDshSurface(identity);
        } catch {
          try {
            await api.unmountDshSurface(identity);
          } catch {
            // A failed native create may leave no child WebView to close.
          }
          report = await api.mountDshSurface({
            ...identity,
            bounds,
            visible: activeSurface === "dsh",
          });
        }
      } else {
        report = await api.mountDshSurface({
          ...identity,
          bounds,
          visible: activeSurface === "dsh",
        });
      }
      mountedSurfaceRef.current = {
        environmentId: environment.id,
        generation: managedRuntime.generation,
      };
      surfaceIntentRef.current = null;
      setNativeSurface(report);
      surfaceFailureRef.current = null;
    } catch (error: unknown) {
      setNativeSurfaceError(commandErrorMessage(error, tRef.current("error.surfaceRetry")));
    } finally {
      setRetryingSurface(false);
    }
  };

  return (
    <main className="shell-app">
      <ActivityRail active={activeSurface} onSelect={setActiveSurface} />
      <section className="shell-workspace">
        <header className="shell-header">
          <div>
            <p className="eyebrow">{t("shell.eyebrow")}</p>
            <h1>{surfaceTitle(activeSurface, t)}</h1>
          </div>
          <RuntimeBadge snapshot={snapshot} error={snapshotError} />
        </header>

        <div className="shell-content">
          {activeSurface === "dsh" && (
            <HarnessSurface
              environment={validatedEnvironment}
              managedRuntime={managedRuntime}
              nativeSurface={nativeSurface}
              nativeSurfaceError={nativeSurfaceError}
              onBoundsChange={handleSurfaceBoundsChange}
              policy={surfacePolicy}
              policyError={surfacePolicyError}
              retryingSurface={retryingSurface}
              snapshot={snapshot}
              validation={validation}
              onOpenSettings={() => setActiveSurface("settings")}
              onRetry={retryNativeSurface}
            />
          )}
          {activeSurface === "terminal" && (
            <TerminalPanel
              api={api}
              onSession={setTerminalSession}
              session={terminalSession}
            />
          )}
          {activeSurface === "browser" && <BrowserPanel api={api} />}
          {activeSurface === "notifications" && <NotificationsPanel api={api} />}
          {activeSurface === "usage" && <UsagePanel api={api} />}
          {activeSurface === "runtime" && (
            <RuntimePanel
              attachedHealth={attachedHealth}
              attachedHealthError={attachedHealthError}
              confirmingManagedStop={confirmingManagedStop}
              diagnostics={diagnostics}
              diagnosticsError={diagnosticsError}
              environment={validatedEnvironment}
              error={snapshotError}
              managedRuntime={managedRuntime}
              managedRuntimeError={managedRuntimeError}
              onCancelManagedStop={() => setConfirmingManagedStop(false)}
              onConfirmManagedStop={stopManaged}
              onRestartManaged={restartManaged}
              onReviewManagedStop={() => setConfirmingManagedStop(true)}
              onProbe={() => setProbeRevision((current) => current + 1)}
              onStartManaged={startManaged}
              probingAttached={probingAttached}
              snapshot={snapshot}
              transitioningManaged={transitioningManaged}
            />
          )}
          {activeSurface === "settings" && (
            <>
              <SetupWizard
                api={api}
                initialEnvironment={validatedEnvironment}
                onSaved={handleSaved}
              />
              {catalog && (
                <EnvironmentList
                  api={api}
                  catalog={catalog}
                  activeEnvironmentId={catalog.activeEnvironmentId}
                  transitioning={transitioningManaged}
                  onActivated={activateEnvironment}
                />
              )}
            </>
          )}
        </div>
      </section>
    </main>
  );
}

function surfaceTitle(surface: SurfaceId, t: (key: string) => string) {
  if (surface === "browser") return t("surface.browser");
  if (surface === "terminal") return t("surface.terminal");
  if (surface === "runtime") return t("surface.runtime");
  if (surface === "settings") return t("surface.settings");
  if (surface === "notifications") return t("surface.notifications");
  if (surface === "usage") return t("surface.usage");
  return t("surface.dsh");
}

function RuntimeBadge({ snapshot, error }: { snapshot: ShellSnapshot | null; error: string | null }) {
  const state = error ? "unavailable" : snapshot?.runtimeState ?? "loading";
  return (
    <div className="runtime-badge" data-state={state} aria-live="polite">
      <span className="runtime-badge__dot" aria-hidden="true" />
      {state}
    </div>
  );
}

function RuntimePanel({
  attachedHealth,
  attachedHealthError,
  confirmingManagedStop,
  diagnostics,
  diagnosticsError,
  environment,
  error,
  managedRuntime,
  managedRuntimeError,
  onCancelManagedStop,
  onConfirmManagedStop,
  onRestartManaged,
  onReviewManagedStop,
  onProbe,
  onStartManaged,
  probingAttached,
  snapshot,
  transitioningManaged,
}: {
  attachedHealth: AttachedHealthReport | null;
  attachedHealthError: string | null;
  confirmingManagedStop: boolean;
  diagnostics: DiagnosticsReport | null;
  diagnosticsError: string | null;
  environment: DshEnvironment | null;
  error: string | null;
  managedRuntime: ManagedRuntimeReport | null;
  managedRuntimeError: string | null;
  onCancelManagedStop(): void;
  onConfirmManagedStop(): void;
  onRestartManaged(): void;
  onReviewManagedStop(): void;
  onProbe(): void;
  onStartManaged(): void;
  probingAttached: boolean;
  snapshot: ShellSnapshot | null;
  transitioningManaged: boolean;
}) {
  const { t } = useI18n();
  const isAttached = environment?.ownership === "attached";
  const isManaged = environment?.ownership === "managed";
  return (
    <section className="panel" aria-labelledby="runtime-heading">
      <div className="panel__heading">
        <p className="eyebrow">{t("runtime.eyebrow")}</p>
        <h2 id="runtime-heading">{t("runtime.title")}</h2>
      </div>
      {error ? (
        <div className="callout callout--danger">{error}</div>
      ) : (
        <dl className="definition-grid">
          <div><dt>{t("runtime.phase")}</dt><dd>{snapshot?.phase ?? t("common.loading")}</dd></div>
          <div><dt>{t("runtime.state")}</dt><dd>{snapshot?.runtimeState ?? t("common.loading")}</dd></div>
          <div><dt>{t("runtime.environment")}</dt><dd>{snapshot?.environmentId ?? t("common.notSelected")}</dd></div>
          <div><dt>{t("runtime.generation")}</dt><dd>{snapshot?.generation ?? 0}</dd></div>
        </dl>
      )}
      {isAttached && (
        <section className="attached-health" aria-labelledby="attached-health-heading">
          <div className="attached-health__heading">
            <div>
              <p className="eyebrow">{t("runtime.attached.eyebrow")}</p>
              <h3 id="attached-health-heading">{t("runtime.attached.title")}</h3>
            </div>
            <button
              className="secondary-button"
              disabled={probingAttached}
              onClick={onProbe}
              type="button"
            >
              {probingAttached ? t("runtime.probing") : t("runtime.probeAgain")}
            </button>
          </div>
          {attachedHealthError && (
            <div className="callout callout--danger" role="alert">{attachedHealthError}</div>
          )}
          {attachedHealth && (
            <>
              <dl className="definition-grid definition-grid--health">
                <div><dt>{t("runtime.reachability")}</dt><dd>{attachedHealth.reachability}</dd></div>
                <div><dt>{t("runtime.identity")}</dt><dd>{attachedHealth.identity}</dd></div>
                <div><dt>{t("runtime.processOwnership")}</dt><dd>{attachedHealth.processOwnership}</dd></div>
                <div><dt>{t("runtime.mutation")}</dt><dd>{attachedHealth.lifecycleMutation}</dd></div>
                <div>
                  <dt>{t("runtime.endpoint")}</dt>
                  <dd>{attachedHealth.endpoint.host}:{attachedHealth.endpoint.port}</dd>
                </div>
                <div>
                  <dt>{t("runtime.latency")}</dt>
                  <dd>{attachedHealth.latencyMs === null ? t("runtime.notAvailable") : `${attachedHealth.latencyMs} ms`}</dd>
                </div>
              </dl>
              <div className="callout callout--warning">
                {attachedHealth.evidence[0]?.message}
              </div>
            </>
          )}
        </section>
      )}
      {isManaged && (
        <ManagedRuntimeSection
          confirmingStop={confirmingManagedStop}
          error={managedRuntimeError}
          onCancelStop={onCancelManagedStop}
          onConfirmStop={onConfirmManagedStop}
          onRestart={onRestartManaged}
          onReviewStop={onReviewManagedStop}
          onStart={onStartManaged}
          report={managedRuntime}
          transitioning={transitioningManaged}
        />
      )}
      {isManaged && <DiagnosticsSection error={diagnosticsError} report={diagnostics} />}
      <p className="panel__note">
        {isAttached ? t("runtime.note.attached") : t("runtime.note.managed")}
      </p>
    </section>
  );
}

function surfaceBoundsEqual(left: DshSurfaceBounds | null, right: DshSurfaceBounds | null) {
  return left?.x === right?.x &&
    left?.y === right?.y &&
    left?.width === right?.width &&
    left?.height === right?.height;
}

function ManagedRuntimeSection({
  confirmingStop,
  error,
  onCancelStop,
  onConfirmStop,
  onRestart,
  onReviewStop,
  onStart,
  report,
  transitioning,
}: {
  confirmingStop: boolean;
  error: string | null;
  onCancelStop(): void;
  onConfirmStop(): void;
  onRestart(): void;
  onReviewStop(): void;
  onStart(): void;
  report: ManagedRuntimeReport | null;
  transitioning: boolean;
}) {
  const { t } = useI18n();
  const state = report?.state ?? "loading";
  const canStart =
    report?.state === "stopped" || report?.state === "crashed" || report?.state === "safe_stop";
  const canRestart = report?.state === "healthy";
  const canStop =
    report !== null &&
    report.generation > 0 &&
    (report.state === "starting" || report.state === "healthy");
  return (
    <section className="managed-runtime" aria-labelledby="managed-runtime-heading">
      <div className="managed-runtime__heading">
        <div>
          <p className="eyebrow">{t("runtime.managed.eyebrow")}</p>
          <h3 id="managed-runtime-heading">{t("runtime.managed.title")}</h3>
        </div>
        {canStart && (
          <button
            className="primary-button"
            disabled={transitioning}
            onClick={onStart}
            type="button"
          >
            {transitioning ? t("runtime.starting") : t("runtime.start")}
          </button>
        )}
        {canRestart && (
          <button
            className="secondary-button"
            disabled={transitioning}
            onClick={onRestart}
            type="button"
          >
            {transitioning ? t("runtime.restarting") : t("runtime.restart")}
          </button>
        )}
        {canStop && !confirmingStop && (
          <button
            className="secondary-button"
            disabled={transitioning}
            onClick={onReviewStop}
            type="button"
          >
            {t("runtime.reviewStop")}
          </button>
        )}
      </div>
      {error && <div className="callout callout--danger" role="alert">{error}</div>}
      <dl className="definition-grid definition-grid--health">
        <div><dt>{t("runtime.state")}</dt><dd>{state}</dd></div>
        <div><dt>{t("runtime.generation")}</dt><dd>{report?.generation ?? 0}</dd></div>
        <div><dt>{t("runtime.processOwnership")}</dt><dd>{report?.processOwnership ?? "none"}</dd></div>
        <div><dt>{t("runtime.readiness")}</dt><dd>{report?.readiness ?? t("common.loading")}</dd></div>
        <div><dt>{t("runtime.instance")}</dt><dd>{report?.instanceId ?? "none"}</dd></div>
        <div><dt>{t("runtime.stopDisposition")}</dt><dd>{report?.stopDisposition ?? "not_requested"}</dd></div>
        {report?.recovery && (
          <>
            <div><dt>{t("runtime.recoveryCrashes")}</dt><dd>{report.recovery.crashCount} / {report.recovery.budget}</dd></div>
            <div><dt>{t("runtime.recoveryState")}</dt><dd>{report.recovery.safeStop ? t("runtime.recoverySafeStop") : t("runtime.recoveryBounded")}</dd></div>
          </>
        )}
      </dl>
      {report?.endpoint && (
        <div className="callout callout--success">
          {t("runtime.verifiedEndpoint", {
            endpoint:
              report.endpoint.scheme + "://" + report.endpoint.host + ":" + report.endpoint.port,
          })}
        </div>
      )}
      {report?.evidence[0] && (
        <div className={report.evidence[0].severity === "error" ? "callout callout--danger" : "callout"}>
          {report.evidence[0].message}
        </div>
      )}
      {confirmingStop && canStop && (
        <div
          className="stop-confirmation"
          role="alertdialog"
          aria-label={t("runtime.confirmStop.aria")}
        >
          <p>{t("runtime.confirmStop.body", { generation: String(report.generation) })}</p>
          <div className="button-row">
            <button className="secondary-button" disabled={transitioning} onClick={onCancelStop} type="button">
              {t("common.cancel")}
            </button>
            <button className="primary-button" disabled={transitioning} onClick={onConfirmStop} type="button">
              {transitioning
                ? t("runtime.stopping")
                : t("runtime.confirmStop.action", { generation: String(report.generation) })}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

function DiagnosticsSection({
  error,
  report,
}: {
  error: string | null;
  report: DiagnosticsReport | null;
}) {
  const { t } = useI18n();
  return (
    <section className="diagnostics" aria-labelledby="diagnostics-heading">
      <div className="diagnostics__heading">
        <div>
          <p className="eyebrow">{t("diagnostics.eyebrow")}</p>
          <h3 id="diagnostics-heading">{t("diagnostics.title")}</h3>
        </div>
      </div>
      {error && <div className="callout callout--danger" role="alert">{error}</div>}
      {report ? (
        <>
          <dl className="definition-grid definition-grid--health">
            <div><dt>{t("diagnostics.observed")}</dt><dd>{new Date(report.observedAtUnixMs).toISOString()}</dd></div>
            <div><dt>{t("diagnostics.runtimeState")}</dt><dd>{report.runtime.state}</dd></div>
            <div><dt>{t("runtime.readiness")}</dt><dd>{report.runtime.readiness}</dd></div>
            <div>
              <dt>{t("runtime.endpoint")}</dt>
              <dd>
                {report.runtime.endpoint
                  ? report.runtime.endpoint.host + ":" + report.runtime.endpoint.port
                  : t("common.none")}
              </dd>
            </div>
            <div>
              <dt>{t("diagnostics.surface")}</dt>
              <dd>{report.surface.state}{report.surface.visible ? t("diagnostics.visible") : ""}</dd>
            </div>
            <div>
              <dt>{t("diagnostics.process")}</dt>
              <dd>
                {report.process.retained ? t("diagnostics.retained") : t("diagnostics.notRetained")}
                {report.process.owned ? t("diagnostics.owned") : ""}
              </dd>
            </div>
            <div><dt>{t("diagnostics.catalogRevision")}</dt><dd>{report.catalog.revision}</dd></div>
          </dl>
          <ul className="diagnostics__evidence">
            {report.evidence.map((item, index) => (
              <li key={`${item.code}-${index}`} data-severity={item.severity}>
                <span className="diagnostics__evidence-code">{item.code}</span> {item.message}
              </li>
            ))}
          </ul>
        </>
      ) : (
        <p className="panel__note">{t("diagnostics.notAvailable")}</p>
      )}
    </section>
  );
}
function NotificationsPanel({ api }: { api: DesktopApi }) {
  const { t } = useI18n();
  const [notifications, setNotifications] = useState<NotificationReport[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const tRef = useRef(t);
  tRef.current = t;

  const refresh = useCallback(async () => {
    try {
      setNotifications(await api.listNotifications());
      setError(null);
    } catch (cause: unknown) {
      setError(commandErrorMessage(cause, tRef.current("error.notificationsUnavailable")));
    }
  }, [api]);

  useEffect(() => {
    let current = true;
    let unlisten: (() => void) | undefined;
    // Live push channel (AC-NOT-002): the backend forwards every new
    // notification over notification://event; the list is re-read from the
    // local audit trail so deduplication and dismissal stay authoritative.
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<NotificationReport>("notification://event", () => {
          void refresh();
        }),
      )
      .then((stop) => {
        if (current) unlisten = stop;
        else stop();
      });
    void refresh();
    return () => {
      current = false;
      unlisten?.();
    };
  }, [api, refresh]);

  const dismiss = async (notificationId: string) => {
    try {
      await api.dismissNotification({ schemaVersion: 1, notificationId });
      setNotifications((current) =>
        current
          ? current.filter((notification) => notification.id !== notificationId)
          : current,
      );
      setError(null);
    } catch (cause: unknown) {
      setError(commandErrorMessage(cause, t("error.notificationDismiss")));
    }
  };

  return (
    <section className="panel" aria-labelledby="notifications-heading">
      <div className="panel__heading panel__heading--split">
        <div>
          <p className="eyebrow">{t("notifications.eyebrow")}</p>
          <h2 id="notifications-heading">{t("surface.notifications")}</h2>
        </div>
        <button className="secondary-button" onClick={() => void refresh()} type="button">
          {t("common.refresh")}
        </button>
      </div>
      {error && <div className="callout callout--danger" role="alert">{error}</div>}
      {notifications === null ? (
        <p className="panel__note">{t("notifications.loading")}</p>
      ) : notifications.length === 0 ? (
        <p className="panel__note">{t("notifications.empty")}</p>
      ) : (
        <ul className="notifications-list">
          {notifications.map((notification) => (
            <li className="notification-item" key={notification.id}>
              <div className="notification-item__body">
                <div className="notification-item__title-row">
                  <strong>{notification.title}</strong>
                  <span className="policy-badge" data-policy={notification.contentPolicy}>
                    {notification.contentPolicy}
                  </span>
                  {notification.deduplicated && (
                    <span className="deduplicated-badge">{t("notifications.deduplicated")}</span>
                  )}
                </div>
                {notification.deliveredBody && (
                  <p className="notification-item__body-text">{notification.deliveredBody}</p>
                )}
                <p className="notification-item__meta">
                  {notification.event} · {new Date(notification.createdAtUnixMs).toLocaleString()}
                </p>
              </div>
              <button
                className="secondary-button notification-item__dismiss"
                onClick={() => void dismiss(notification.id)}
                type="button"
              >
                {t("notifications.dismiss")}
              </button>
            </li>
          ))}
        </ul>
      )}
      <p className="panel__note">{t("notifications.note")}</p>
    </section>
  );
}


function UsagePanel({ api }: { api: DesktopApi }) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = useState<UsageSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const tRef = useRef(t);
  tRef.current = t;

  const refresh = useCallback(async () => {
    try {
      setSnapshot(await api.getUsageSnapshot({ schemaVersion: 1 }));
      setError(null);
    } catch (cause: unknown) {
      setError(commandErrorMessage(cause, tRef.current("error.usageUnavailable")));
    }
  }, [api]);

  useEffect(() => {
    void refresh();
  }, [api, refresh]);

  return (
    <section className="panel" aria-labelledby="usage-heading">
      <div className="panel__heading panel__heading--split">
        <div>
          <p className="eyebrow">{t("usage.eyebrow")}</p>
          <h2 id="usage-heading">{t("surface.usage")}</h2>
        </div>
        <button className="secondary-button" onClick={() => void refresh()} type="button">
          {t("common.refresh")}
        </button>
      </div>
      {error && <div className="callout callout--danger" role="alert">{error}</div>}
      {snapshot === null ? (
        <p className="panel__note">{t("usage.loading")}</p>
      ) : (
        <>
          <dl className="definition-grid definition-grid--health">
            <div><dt>{t("usage.inputTokens")}</dt><dd>{snapshot.totals.inputTokens}</dd></div>
            <div><dt>{t("usage.outputTokens")}</dt><dd>{snapshot.totals.outputTokens}</dd></div>
            <div><dt>{t("usage.estimates")}</dt><dd>{snapshot.totals.estimateCount}</dd></div>
            {snapshot.totals.cost !== undefined && snapshot.totals.cost !== null && (
              <div><dt>{t("usage.cost")}</dt><dd>{snapshot.totals.cost} {snapshot.totals.currency ?? ""}</dd></div>
            )}
          </dl>
          {snapshot.records.length === 0 ? (
            <p className="panel__note">{t("usage.empty")}</p>
          ) : (
            <ul className="usage-list">
              {snapshot.records.map((record, index) => (
                <li className="usage-item" key={`${record.recordedAtUnixMs}-${index}`}>
                  <div className="usage-item__row">
                    <strong>{record.source}</strong>
                    {record.isEstimate && <span className="estimate-badge">{t("usage.estimate")}</span>}
                  </div>
                  <p className="usage-item__meta">
                    {new Date(record.period.start).toLocaleString()} → {new Date(record.period.end).toLocaleString()}
                  </p>
                  <p className="usage-item__meta">
                    {t("usage.inOut", {
                      input: String(record.inputTokens),
                      output: String(record.outputTokens),
                    })}
                    {record.cost !== undefined && record.cost !== null
                      ? ` · ${record.cost} ${record.currency ?? ""}`
                      : ""}
                  </p>
                </li>
              ))}
            </ul>
          )}
          <p className="panel__note">{t("usage.note")}</p>
        </>
      )}
    </section>
  );
}
function commandErrorMessage(error: unknown, fallback: string) {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof (error as Partial<DesktopCommandError>).message === "string"
  ) {
    return (error as Partial<DesktopCommandError>).message!;
  }
  return fallback;
}