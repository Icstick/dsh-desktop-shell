import { useLayoutEffect, useRef, useState } from "react";

import type {
  DshEnvironment,
  DshSurfaceBounds,
  DshSurfacePolicy,
  DshSurfaceStatus,
  EnvironmentValidation,
  ManagedRuntimeReport,
  ShellSnapshot,
} from "../../../src/contracts";

interface HarnessSurfaceProps {
  environment: DshEnvironment | null;
  managedRuntime: ManagedRuntimeReport | null;
  nativeSurface: DshSurfaceStatus | null;
  nativeSurfaceError: string | null;
  onBoundsChange(bounds: DshSurfaceBounds | null): void;
  onOpenSettings(): void;
  onRetry(): void;
  policy: DshSurfacePolicy | null;
  policyError: string | null;
  retryingSurface: boolean;
  snapshot: ShellSnapshot | null;
  validation: EnvironmentValidation | null;
}

export function HarnessSurface({
  environment,
  managedRuntime,
  nativeSurface,
  nativeSurfaceError,
  onBoundsChange,
  onOpenSettings,
  onRetry,
  policy,
  policyError,
  retryingSurface,
  snapshot,
  validation,
}: HarnessSurfaceProps) {
  const nativeSlotRef = useRef<HTMLDivElement | null>(null);
  const [slotHasUsableBounds, setSlotHasUsableBounds] = useState(true);
  const nativeBindingKey =
    environment?.ownership === "managed" &&
    managedRuntime?.environmentId === environment.id &&
    managedRuntime.state === "healthy" &&
    managedRuntime.readiness === "verified" &&
    managedRuntime.processOwnership === "owned" &&
    managedRuntime.generation > 0
      ? `${environment.id}:${managedRuntime.generation}`
      : null;

  useLayoutEffect(() => {
    const slot = nativeSlotRef.current;
    if (!slot) {
      onBoundsChange(null);
      return;
    }

    const reportBounds = () => {
      const rect = slot.getBoundingClientRect();
      const left = Math.max(0, rect.left);
      const top = Math.max(0, rect.top);
      const right = Math.min(window.innerWidth, rect.right);
      const bottom = Math.min(window.innerHeight, rect.bottom);
      const bounds = {
        x: Math.round(left),
        y: Math.round(top),
        width: Math.round(Math.max(0, right - left)),
        height: Math.round(Math.max(0, bottom - top)),
      };
      const usable = bounds.width >= 320 && bounds.height >= 240;
      setSlotHasUsableBounds(usable);
      onBoundsChange(usable ? bounds : null);
    };

    reportBounds();
    const resizeObserver = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(reportBounds);
    resizeObserver?.observe(slot);
    window.addEventListener("resize", reportBounds);
    window.addEventListener("scroll", reportBounds, true);

    return () => {
      resizeObserver?.disconnect();
      window.removeEventListener("resize", reportBounds);
      window.removeEventListener("scroll", reportBounds, true);
      onBoundsChange(null);
    };
  }, [nativeBindingKey, onBoundsChange]);

  if (!snapshot) {
    return (
      <section className="surface-state" aria-live="polite">
        <div className="surface-state__pulse" aria-hidden="true" />
        <p className="eyebrow">Shell bootstrap</p>
        <h2>Reading canonical runtime state…</h2>
      </section>
    );
  }

  if (!environment || !validation?.valid) {
    return (
      <section className="surface-state" aria-labelledby="surface-empty-heading">
        <span className="surface-state__mark" aria-hidden="true">DS</span>
        <p className="eyebrow">Unprivileged DSH surface</p>
        <h2 id="surface-empty-heading">Choose an existing DSH environment</h2>
        <p>
          The Shell hosts upstream DSH without DOM injection or a native bridge. Validate an environment before a native Surface can be considered.
        </p>
        <button className="primary-button" onClick={onOpenSettings} type="button">
          Open Environment Settings
        </button>
      </section>
    );
  }

  const managedReady = nativeBindingKey !== null;

  if (managedReady) {
    const state = !slotHasUsableBounds
      ? "viewport_too_small"
      : nativeSurface?.state ?? (nativeSurfaceError ? "error" : "mounting");
    const managedEndpoint = managedRuntime?.endpoint;
    const origin = nativeSurface
      ? `${nativeSurface.verifiedOrigin.scheme}://${nativeSurface.verifiedOrigin.host}:${nativeSurface.verifiedOrigin.port}`
      : managedEndpoint
        ? `${managedEndpoint.scheme}://${managedEndpoint.host}:${managedEndpoint.port}`
        : "verified loopback origin";
    const canRetry = state === "error" && nativeSurface?.state !== "unsupported_platform";

    return (
      <section className="surface-frame surface-native" aria-labelledby="surface-native-heading">
        <div className="surface-frame__chrome surface-native__chrome">
          <div>
            <span className="surface-frame__status" data-state={state} aria-hidden="true" />
            <strong>{environment.label}</strong>
            <span className="surface-native__state">{state}</span>
          </div>
          <code>{origin}</code>
        </div>
        <div className="dsh-surface-slot" data-state={state} ref={nativeSlotRef}>
          <div className="surface-native__placeholder" aria-live="polite">
            {(state === "mounting" || state === "loading" || state === "hidden") && (
              <>
                <div className="surface-state__pulse" aria-hidden="true" />
                <p className="eyebrow">Native lifecycle</p>
                <h2 id="surface-native-heading">
                  {state === "hidden" ? "Restoring native DSH Surface…" : "Loading native DSH Surface…"}
                </h2>
              </>
            )}
            {state === "ready" && (
              <p id="surface-native-heading" className="sr-only">
                Native DSH Surface ready
              </p>
            )}
            {state === "unsupported_platform" && (
              <>
                <p className="eyebrow">Platform gate</p>
                <h2 id="surface-native-heading">Native DSH Surface is not enabled on {nativeSurface?.platform ?? "this platform"}</h2>
                <p>The platform-specific permission-denial hooks have not passed their implementation gate.</p>
              </>
            )}
            {state === "stale" && (
              <>
                <p className="eyebrow">Generation gate</p>
                <h2 id="surface-native-heading">The native Surface binding is stale</h2>
                <p>Restart or refresh the Managed runtime before mounting another generation.</p>
              </>
            )}
            {state === "unmounted" && (
              <>
                <p className="eyebrow">Native lifecycle</p>
                <h2 id="surface-native-heading">The native DSH Surface is unmounted</h2>
              </>
            )}
            {state === "viewport_too_small" && (
              <>
                <p className="eyebrow">Layout gate</p>
                <h2 id="surface-native-heading">Expand the window to show native DSH</h2>
                <p>The native Surface requires at least 320 × 240 visible CSS pixels.</p>
              </>
            )}
            {state === "error" && (
              <>
                <p className="eyebrow">Native lifecycle</p>
                <h2 id="surface-native-heading">Native DSH Surface needs attention</h2>
                <p>{nativeSurfaceError ?? nativeSurface?.error?.message ?? "The native Surface operation failed."}</p>
                {canRetry && (
                  <button className="primary-button" disabled={retryingSurface} onClick={onRetry} type="button">
                    {retryingSurface ? "Retrying…" : "Retry native Surface"}
                  </button>
                )}
              </>
            )}
          </div>
        </div>
        <footer className="surface-native__policy" aria-label="Native Surface policy">
          <span>Native IPC denied</span>
          <span>Page permissions denied</span>
          <span>Exact-origin navigation only</span>
        </footer>
      </section>
    );
  }

  const surfaceHeading = environment.ownership === "attached"
    ? "Attached DSH remains read-only"
    : "DSH launch remains intentionally idle";
  const surfaceDescription = environment.ownership === "attached"
    ? "Attached health can report bounded reachability, but it never grants process ownership or lifecycle mutation."
    : "Use the Runtime surface for explicit Managed start. No process is launched automatically when an Environment is restored or saved.";

  return (
    <section className="surface-frame" aria-labelledby="surface-ready-heading">
      <div className="surface-frame__chrome">
        <div>
          <span className="surface-frame__status" aria-hidden="true" />
          <strong>{environment.label}</strong>
        </div>
        <code>{validation.launchPreview?.endpoint}</code>
      </div>
      <div className="surface-frame__body">
        <p className="eyebrow">Environment validated</p>
        <h2 id="surface-ready-heading">{surfaceHeading}</h2>
        <p>{surfaceDescription}</p>
        {policy ? (
          <section className="surface-policy" aria-labelledby="surface-policy-heading">
            <div>
              <p className="eyebrow">Fail-closed policy</p>
              <h3 id="surface-policy-heading">DSH Surface policy ready</h3>
              <p>A native Surface requires a verified, owned Managed generation.</p>
            </div>
            <dl className="definition-grid definition-grid--policy">
              <div>
                <dt>Exact origin</dt>
                <dd>{policy.allowedOrigin.scheme}://{policy.allowedOrigin.host}:{policy.allowedOrigin.port}</dd>
              </div>
              <div><dt>Native IPC</dt><dd>{policy.privilegedIpc}</dd></div>
              <div><dt>External links</dt><dd>user action</dd></div>
              <div><dt>Automatic open</dt><dd>{policy.automaticExternalOpen ? "allowed" : "denied"}</dd></div>
            </dl>
          </section>
        ) : (
          <div className="callout callout--warning" role="status">
            <strong>DSH Surface policy pending.</strong>{" "}
            {policyError ?? "Waiting for a persisted fixed loopback endpoint."}
          </div>
        )}
        <dl className="definition-grid definition-grid--compact">
          <div><dt>Ownership</dt><dd>{environment.ownership}</dd></div>
          <div><dt>Profile</dt><dd>{environment.profile}</dd></div>
          <div><dt>Runtime</dt><dd>{snapshot.runtimeState}</dd></div>
          <div><dt>Generation</dt><dd>{snapshot.generation}</dd></div>
        </dl>
      </div>
    </section>
  );
}
