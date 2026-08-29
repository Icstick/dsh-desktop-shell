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
  ShellSnapshot,
} from "../../../src/contracts";
import { desktopApi, type DesktopApi } from "../../../src/desktop-api";
import { EnvironmentSetup } from "../../environment-settings/src/EnvironmentSetup";
import { HarnessSurface } from "../../harness-surface/src/HarnessSurface";
import { ActivityRail, type SurfaceId } from "./ActivityRail";

interface ShellAppProps {
  api?: DesktopApi;
}

export function ShellApp({ api = desktopApi }: ShellAppProps) {
  const [activeSurface, setActiveSurface] = useState<SurfaceId>("dsh");
  const [snapshot, setSnapshot] = useState<ShellSnapshot | null>(null);
  const [snapshotError, setSnapshotError] = useState<string | null>(null);
  const [validatedEnvironment, setValidatedEnvironment] = useState<DshEnvironment | null>(null);
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
  const [surfaceBounds, setSurfaceBounds] = useState<DshSurfaceBounds | null>(null);
  const [nativeSurface, setNativeSurface] = useState<DshSurfaceStatus | null>(null);
  const [nativeSurfaceError, setNativeSurfaceError] = useState<string | null>(null);
  const [retryingSurface, setRetryingSurface] = useState(false);
  const mountedSurfaceRef = useRef<{ environmentId: string; generation: number } | null>(null);
  const lastSurfaceBoundsRef = useRef<DshSurfaceBounds | null>(null);
  const surfaceIntentRef = useRef<string | null>(null);
  const surfaceFailureRef = useRef<string | null>(null);

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
        if (current) setSnapshotError("Desktop backend is unavailable.");
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
          commandErrorMessage(error, "Attached endpoint health is unavailable."),
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
          commandErrorMessage(error, "Managed runtime status is unavailable."),
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
          commandErrorMessage(error, "Diagnostics are unavailable."),
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
          commandErrorMessage(error, "Native DSH Surface is unavailable."),
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
            commandErrorMessage(error, "Native DSH Surface status is unavailable."),
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
          commandErrorMessage(error, "DSH Surface policy is unavailable."),
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
      setSnapshotError("Saved, but the runtime snapshot could not be refreshed.");
    }
  };

  const applyManagedReport = (report: ManagedRuntimeReport) => {
    setManagedRuntime(report);
    setSnapshot((snapshot) =>
      snapshot
        ? { ...snapshot, runtimeState: report.state, generation: report.generation }
        : snapshot,
    );
  };

  const startManaged = async () => {
    if (validatedEnvironment?.ownership !== "managed") return;
    setTransitioningManaged(true);
    setManagedRuntimeError(null);
    setConfirmingManagedStop(false);
    try {
      applyManagedReport(
        await api.startManagedEnvironment({
          schemaVersion: 1,
          environmentId: validatedEnvironment.id,
        }),
      );
    } catch (error: unknown) {
      setManagedRuntimeError(commandErrorMessage(error, "Managed start is unavailable."));
    } finally {
      setTransitioningManaged(false);
    }
  };

  const stopManaged = async () => {
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
    try {
      applyManagedReport(
        await api.stopManagedEnvironment({
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
      setConfirmingManagedStop(false);
    } catch (error: unknown) {
      setManagedRuntimeError(commandErrorMessage(error, "Managed stop is unavailable."));
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
      setManagedRuntimeError(commandErrorMessage(error, "Managed restart is unavailable."));
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
      setNativeSurfaceError(commandErrorMessage(error, "Native DSH Surface retry failed."));
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
            <p className="eyebrow">DSH Desktop Shell</p>
            <h1>{surfaceTitle(activeSurface)}</h1>
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
            <EnvironmentSetup
              api={api}
              initialEnvironment={validatedEnvironment}
              onSaved={handleSaved}
            />
          )}
        </div>
      </section>
    </main>
  );
}

function surfaceTitle(surface: SurfaceId) {
  if (surface === "runtime") return "Runtime";
  if (surface === "settings") return "Environment Settings";
  return "DSH Surface";
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
  const isAttached = environment?.ownership === "attached";
  const isManaged = environment?.ownership === "managed";
  return (
    <section className="panel" aria-labelledby="runtime-heading">
      <div className="panel__heading">
        <p className="eyebrow">Canonical backend state</p>
        <h2 id="runtime-heading">Runtime snapshot</h2>
      </div>
      {error ? (
        <div className="callout callout--danger">{error}</div>
      ) : (
        <dl className="definition-grid">
          <div><dt>Phase</dt><dd>{snapshot?.phase ?? "loading"}</dd></div>
          <div><dt>State</dt><dd>{snapshot?.runtimeState ?? "loading"}</dd></div>
          <div><dt>Environment</dt><dd>{snapshot?.environmentId ?? "not selected"}</dd></div>
          <div><dt>Generation</dt><dd>{snapshot?.generation ?? 0}</dd></div>
        </dl>
      )}
      {isAttached && (
        <section className="attached-health" aria-labelledby="attached-health-heading">
          <div className="attached-health__heading">
            <div>
              <p className="eyebrow">Read-only endpoint evidence</p>
              <h3 id="attached-health-heading">Attached health</h3>
            </div>
            <button
              className="secondary-button"
              disabled={probingAttached}
              onClick={onProbe}
              type="button"
            >
              {probingAttached ? "Probing…" : "Probe again"}
            </button>
          </div>
          {attachedHealthError && (
            <div className="callout callout--danger" role="alert">{attachedHealthError}</div>
          )}
          {attachedHealth && (
            <>
              <dl className="definition-grid definition-grid--health">
                <div><dt>Reachability</dt><dd>{attachedHealth.reachability}</dd></div>
                <div><dt>Identity</dt><dd>{attachedHealth.identity}</dd></div>
                <div><dt>Process ownership</dt><dd>{attachedHealth.processOwnership}</dd></div>
                <div><dt>Mutation</dt><dd>{attachedHealth.lifecycleMutation}</dd></div>
                <div>
                  <dt>Endpoint</dt>
                  <dd>{attachedHealth.endpoint.host}:{attachedHealth.endpoint.port}</dd>
                </div>
                <div>
                  <dt>Latency</dt>
                  <dd>{attachedHealth.latencyMs === null ? "not available" : `${attachedHealth.latencyMs} ms`}</dd>
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
        {isAttached
          ? "Lifecycle controls remain unavailable. Attached reachability never implies DSH identity or Desktop process ownership."
          : "Managed controls act only on the retained process-tree handle. A verified generation may mount the platform-gated native DSH Surface."}
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
          <p className="eyebrow">Owned process-tree evidence</p>
          <h3 id="managed-runtime-heading">Managed runtime</h3>
        </div>
        {canStart && (
          <button
            className="primary-button"
            disabled={transitioning}
            onClick={onStart}
            type="button"
          >
            {transitioning ? "Starting…" : "Start Managed DSH"}
          </button>
        )}
        {canRestart && (
          <button
            className="secondary-button"
            disabled={transitioning}
            onClick={onRestart}
            type="button"
          >
            {transitioning ? "Restarting…" : "Restart managed DSH"}
          </button>
        )}
        {canStop && !confirmingStop && (
          <button
            className="secondary-button"
            disabled={transitioning}
            onClick={onReviewStop}
            type="button"
          >
            Review managed stop
          </button>
        )}
      </div>
      {error && <div className="callout callout--danger" role="alert">{error}</div>}
      <dl className="definition-grid definition-grid--health">
        <div><dt>State</dt><dd>{state}</dd></div>
        <div><dt>Generation</dt><dd>{report?.generation ?? 0}</dd></div>
        <div><dt>Process ownership</dt><dd>{report?.processOwnership ?? "none"}</dd></div>
        <div><dt>Readiness</dt><dd>{report?.readiness ?? "loading"}</dd></div>
        <div><dt>Instance</dt><dd>{report?.instanceId ?? "none"}</dd></div>
        <div><dt>Stop disposition</dt><dd>{report?.stopDisposition ?? "not_requested"}</dd></div>
        {report?.recovery && (
          <>
            <div><dt>Recovery crashes</dt><dd>{report.recovery.crashCount} / {report.recovery.budget}</dd></div>
            <div><dt>Recovery state</dt><dd>{report.recovery.safeStop ? "safe stop" : "bounded recovery"}</dd></div>
          </>
        )}
      </dl>
      {report?.endpoint && (
        <div className="callout callout--success">
          Verified endpoint: {report.endpoint.scheme}://{report.endpoint.host}:{report.endpoint.port}
        </div>
      )}
      {report?.evidence[0] && (
        <div className={report.evidence[0].severity === "error" ? "callout callout--danger" : "callout"}>
          {report.evidence[0].message}
        </div>
      )}
      {confirmingStop && canStop && (
        <div className="stop-confirmation" role="alertdialog" aria-label="Confirm managed stop">
          <p>
            Stop only the retained process tree for generation {report.generation}. No PID or port
            ownership will be inferred.
          </p>
          <div className="button-row">
            <button className="secondary-button" disabled={transitioning} onClick={onCancelStop} type="button">
              Cancel
            </button>
            <button className="primary-button" disabled={transitioning} onClick={onConfirmStop} type="button">
              {transitioning ? "Stopping…" : `Confirm stop generation ${report.generation}`}
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
  return (
    <section className="diagnostics" aria-labelledby="diagnostics-heading">
      <div className="diagnostics__heading">
        <div>
          <p className="eyebrow">Credential-free snapshot (AC-LOG-001)</p>
          <h3 id="diagnostics-heading">Diagnostics</h3>
        </div>
      </div>
      {error && <div className="callout callout--danger" role="alert">{error}</div>}
      {report ? (
        <>
          <dl className="definition-grid definition-grid--health">
            <div><dt>Observed</dt><dd>{new Date(report.observedAtUnixMs).toISOString()}</dd></div>
            <div><dt>Runtime state</dt><dd>{report.runtime.state}</dd></div>
            <div><dt>Readiness</dt><dd>{report.runtime.readiness}</dd></div>
            <div>
              <dt>Endpoint</dt>
              <dd>
                {report.runtime.endpoint
                  ? `${report.runtime.endpoint.host}:${report.runtime.endpoint.port}`
                  : "none"}
              </dd>
            </div>
            <div>
              <dt>Surface</dt>
              <dd>{report.surface.state}{report.surface.visible ? " · visible" : ""}</dd>
            </div>
            <div>
              <dt>Process</dt>
              <dd>
                {report.process.retained ? "retained" : "not retained"}
                {report.process.owned ? " · owned" : ""}
              </dd>
            </div>
            <div><dt>Catalog revision</dt><dd>{report.catalog.revision}</dd></div>
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
        <p className="panel__note">Diagnostics are not available yet.</p>
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
