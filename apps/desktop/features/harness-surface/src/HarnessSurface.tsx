import { useLayoutEffect, useRef, useState } from "react";

import { useI18n } from "../../../src/i18n";

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
  const { t } = useI18n();
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
        <p className="eyebrow">{t("harness.bootstrap.eyebrow")}</p>
        <h2>{t("harness.bootstrap.reading")}</h2>
      </section>
    );
  }

  if (!environment || !validation?.valid) {
    return (
      <section className="surface-state" aria-labelledby="surface-empty-heading">
        <span className="surface-state__mark" aria-hidden="true">DS</span>
        <p className="eyebrow">{t("harness.empty.eyebrow")}</p>
        <h2 id="surface-empty-heading">{t("harness.empty.title")}</h2>
        <p>
          {t("harness.empty.body")}
        </p>
        <button className="primary-button" onClick={onOpenSettings} type="button">
          {t("harness.empty.openSettings")}
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
            <span className="surface-native__state">{t("enum.surface." + state)}</span>
          </div>
          <code>{origin}</code>
        </div>
        <div className="dsh-surface-slot" data-state={state} ref={nativeSlotRef}>
          <div className="surface-native__placeholder" aria-live="polite">
            {(state === "mounting" || state === "loading" || state === "hidden") && (
              <>
                <div className="surface-state__pulse" aria-hidden="true" />
                <p className="eyebrow">{t("harness.native.eyebrow")}</p>
                <h2 id="surface-native-heading">
                  {state === "hidden" ? t("harness.native.restoring") : t("harness.native.loading")}
                </h2>
              </>
            )}
            {state === "ready" && (
              <p id="surface-native-heading" className="sr-only">
                {t("harness.native.ready")}
              </p>
            )}
            {state === "unsupported_platform" && (
              <>
                <p className="eyebrow">{t("harness.platformGate.eyebrow")}</p>
                <h2 id="surface-native-heading">{t("harness.platformGate.title", { platform: nativeSurface?.platform ?? "this platform" })}</h2>
                <p>{t("harness.platformGate.body")}</p>
              </>
            )}
            {state === "stale" && (
              <>
                <p className="eyebrow">{t("harness.generationGate.eyebrow")}</p>
                <h2 id="surface-native-heading">{t("harness.generationGate.title")}</h2>
                <p>{t("harness.generationGate.body")}</p>
              </>
            )}
            {state === "unmounted" && (
              <>
                <p className="eyebrow">{t("harness.native.eyebrow")}</p>
                <h2 id="surface-native-heading">{t("harness.unmounted.title")}</h2>
              </>
            )}
            {state === "viewport_too_small" && (
              <>
                <p className="eyebrow">{t("harness.layoutGate.eyebrow")}</p>
                <h2 id="surface-native-heading">{t("harness.layoutGate.title")}</h2>
                <p>{t("harness.layoutGate.body")}</p>
              </>
            )}
            {state === "error" && (
              <>
                <p className="eyebrow">{t("harness.native.eyebrow")}</p>
                <h2 id="surface-native-heading">{t("harness.error.title")}</h2>
                <p>{nativeSurfaceError ?? nativeSurface?.error?.message ?? t("harness.error.operationFailed")}</p>
                {canRetry && (
                  <button className="primary-button" disabled={retryingSurface} onClick={onRetry} type="button">
                    {retryingSurface ? t("harness.error.retrying") : t("harness.error.retry")}
                  </button>
                )}
              </>
            )}
          </div>
        </div>
        <footer className="surface-native__policy" aria-label={t("harness.footer.aria")}>
          <span>{t("harness.footer.ipcDenied")}</span>
          <span>{t("harness.footer.permissionsDenied")}</span>
          <span>{t("harness.footer.exactOrigin")}</span>
        </footer>
      </section>
    );
  }

  const surfaceHeading = environment.ownership === "attached"
    ? t("harness.attached.title")
    : t("harness.idle.title");
  const surfaceDescription = environment.ownership === "attached"
    ? t("harness.attached.body")
    : t("harness.idle.body");

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
        <p className="eyebrow">{t("harness.validated.eyebrow")}</p>
        <h2 id="surface-ready-heading">{surfaceHeading}</h2>
        <p>{surfaceDescription}</p>
        {policy ? (
          <section className="surface-policy" aria-labelledby="surface-policy-heading">
            <div>
              <p className="eyebrow">{t("harness.policy.eyebrow")}</p>
              <h3 id="surface-policy-heading">{t("harness.policy.title")}</h3>
              <p>{t("harness.policy.body")}</p>
              <p className="surface-policy__note">{t("harness.policy.note.defaults")}</p>
            </div>
            <dl className="definition-grid definition-grid--policy">
              <div>
                <dt>{t("harness.policy.exactOrigin")}</dt>
                <dd>{policy.allowedOrigin.scheme}://{policy.allowedOrigin.host}:{policy.allowedOrigin.port}</dd>
              </div>
              <div><dt>{t("harness.policy.nativeIpc")}</dt><dd>{t("enum.mut." + policy.privilegedIpc)}</dd></div>
              <div><dt>{t("harness.policy.externalLinks")}</dt><dd>{t("harness.policy.userAction")}</dd></div>
              <div><dt>{t("harness.policy.automaticOpen")}</dt><dd>{policy.automaticExternalOpen ? t("harness.policy.allowed") : t("harness.policy.denied")}</dd></div>
            </dl>
            <p className="surface-policy__note surface-policy__note--origin">{t("harness.policy.note.origin")}</p>
          </section>
        ) : (
          <div className="callout callout--warning" role="status">
            <strong>{t("harness.policy.pendingTitle")}</strong>{" "}
            {policyError ?? t("harness.policy.pendingBody")}
          </div>
        )}
        <dl className="definition-grid definition-grid--compact">
          <div><dt>{t("harness.meta.ownership")}</dt><dd>{environment.ownership}</dd></div>
          <div><dt>{t("harness.meta.profile")}</dt><dd>{environment.profile}</dd></div>
          <div><dt>{t("harness.meta.runtime")}</dt><dd>{snapshot.runtimeState}</dd></div>
          <div><dt>{t("harness.meta.generation")}</dt><dd>{snapshot.generation}</dd></div>
        </dl>
      </div>
    </section>
  );
}
