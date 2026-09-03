import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DshEnvironment } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { I18nProvider, persistLang } from "../../../src/i18n";
import { ShellApp } from "./ShellApp";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

beforeEach(() => {
  window.localStorage.clear();
});

// ShellApp reads the active language from I18nProvider (no provider means
// the default zh locale). Most assertions in this file are written in
// English, so the default render helper pins the English locale.
function renderShellApp(api: DesktopApi) {
  persistLang("en");
  return render(
    <I18nProvider>
      <ShellApp api={api} />
    </I18nProvider>,
  );
}

function createApi(): DesktopApi {
  return {
    dismissNotification: vi.fn().mockResolvedValue(undefined),
    listBrowsers: vi.fn().mockResolvedValue([]),
    listNotifications: vi.fn().mockResolvedValue([]),
    navigateBrowser: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      sessionId: request.sessionId,
      state: "ready",
      mode: "human_surface",
      currentUrl: request.url,
      createdAtUnixMs: 1787792400000,
      lastActivityUnixMs: 1787792400100,
      error: null,
    })),
    notifyApplication: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      id: "notif-1787792400000-1",
      event: "runtime_changed",
      title: "Runtime changed",
      contentPolicy: "title_only",
      deliveredBody: null,
      createdAtUnixMs: 1787792400000,
      dedupeKey: null,
      deduplicated: false,
    }),
    closeBrowser: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      sessionId: request.sessionId,
      state: "closed",
      mode: "human_surface",
      currentUrl: null,
      createdAtUnixMs: 1787792400000,
      lastActivityUnixMs: 1787792400100,
      error: null,
    })),
    closeTerminal: vi.fn().mockResolvedValue(undefined),
    createBrowser: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      state: "created",
      mode: "human_surface",
      currentUrl: null,
      createdAtUnixMs: 1787792400000,
      lastActivityUnixMs: null,
      error: null,
    }),
    createTerminal: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      sessionId: "pty-test-1",
      state: "running",
      mode: "human_surface",
      cols: 80,
      rows: 24,
      createdAtUnixMs: 1787792400000,
      lastActivityUnixMs: null,
      error: null,
    }),
    listTerminals: vi.fn().mockResolvedValue([]),
    resizeTerminal: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      sessionId: request.sessionId,
      state: "running",
      mode: "human_surface",
      cols: request.cols,
      rows: request.rows,
      createdAtUnixMs: 1787792400000,
      lastActivityUnixMs: null,
      error: null,
    })),
    statusTerminal: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      sessionId: request.sessionId,
      state: "running",
      mode: "human_surface",
      cols: 80,
      rows: 24,
      createdAtUnixMs: 1787792400000,
      lastActivityUnixMs: null,
      error: null,
    })),
    writeTerminal: vi.fn().mockResolvedValue(undefined),
    discoverHarnesses: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      scannedSources: ["explicit", "dsh_path", "path"],
      deferredSources: ["global"],
      candidates: [],
    }),
    discoverProfiles: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      dshHome: "C:\\Users\\test\\.dsh",
      profiles: [],
    }),
    probePort: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      port: 8080,
      inUse: false,
    }),
    pickDirectory: vi.fn().mockResolvedValue(null),
    setActiveEnvironment: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      revision: 2,
      activeEnvironmentId: request.environmentId,
      environments: [],
    })),
    evaluateDshSurfaceNavigation: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      environmentId: "attached-local",
      navigationKind: "main_frame",
      candidateOrigin: "http://127.0.0.1:4317",
      disposition: "allow_in_surface",
      reason: "SAME_ORIGIN",
      userGestureRequired: false,
      userConfirmationRequired: false,
      privilegedIpc: "denied",
      openAutomatically: false,
    }),
    getDshSurfacePolicy: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      environmentId: request.environmentId,
      surfaceLabel: "dsh-surface",
      allowedOrigin: { scheme: "http", host: "127.0.0.1", port: 4317 },
      sameOriginMainFrame: "allow",
      externalHttpNavigation: "delegate_with_user_action",
      newWindow: "deny",
      downloads: "deny",
      permissions: "deny",
      privilegedIpc: "denied",
      domInjection: "no",
      rendererPatch: "no",
      automaticExternalOpen: false,
    })),
    getDshSurfaceStatus: vi.fn().mockImplementation(async (request) =>
      surfaceStatus(request.environmentId, request.expectedGeneration, "ready", true),
    ),
    getEnvironmentCatalog: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      revision: 0,
      activeEnvironmentId: null,
      environments: [],
    }),
    getDiagnostics: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      environmentId: request.environmentId,
      observedAtUnixMs: 1787792400100,
      runtime: {
        state: "stopped",
        generation: 0,
        readiness: "not_started",
        endpoint: null,
        recovery: null,
      },
      surface: {
        state: "unmounted",
        platform: "windows",
        generation: 0,
        visible: false,
        error: null,
      },
      catalog: { revision: 0, activeEnvironmentId: null },
      process: { retained: false, owned: false },
      evidence: [
        {
          code: "DIAGNOSTICS_COLLECTED",
          severity: "info",
          message: "Diagnostics snapshot collected.",
        },
      ],
    })),
    getManagedRuntimeStatus: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      environmentId: request.environmentId,
      ownership: "managed",
      state: "stopped",
      generation: 0,
      instanceId: null,
      processOwnership: "none",
      lifecycleMutation: "allowed",
      readiness: "not_started",
      endpoint: null,
      stopDisposition: "not_requested",
      recovery: null,
      observedAtUnixMs: 1787792400000,
      evidence: [
        {
          code: "MANAGED_STOPPED",
          severity: "info",
          message: "No Managed process tree is currently retained.",
        },
      ],
    })),
    getShellSnapshot: vi.fn().mockResolvedValue({
      phase: "shell-mvp",
      runtimeState: "unconfigured",
      environmentId: null,
      generation: 0,
    }),
    getUsageSnapshot: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      generatedAtUnixMs: 1787792400000,
      records: [],
      totals: { inputTokens: 0, outputTokens: 0, estimateCount: 0 },
    }),
    mountDshSurface: vi.fn().mockImplementation(async (request) => ({
      ...surfaceStatus(request.environmentId, request.expectedGeneration, "ready", request.visible),
      bounds: request.bounds,
    })),
    probeAttachedEnvironment: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      environmentId: "attached-local",
      ownership: "attached",
      state: "attached",
      reachability: "reachable",
      identity: "unverified",
      processOwnership: "external",
      lifecycleMutation: "denied",
      endpoint: { host: "127.0.0.1", port: 4317 },
      timeoutMs: 750,
      latencyMs: 4,
      observedAtUnixMs: 1787792400000,
      evidence: [
        {
          code: "TCP_REACHABLE_IDENTITY_UNVERIFIED",
          severity: "warning",
          message: "Reachability does not verify DSH identity or process ownership.",
        },
      ],
    }),
    reloadDshSurface: vi.fn().mockImplementation(async (request) =>
      surfaceStatus(request.environmentId, request.expectedGeneration, "loading", true),
    ),
    saveEnvironment: vi.fn().mockImplementation(async (environment) => ({
      schemaVersion: 1,
      revision: 1,
      activeEnvironmentId: environment.id,
      environments: [environment],
    })),
    snapshotBrowser: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      sessionId: request.sessionId,
      state: "ready",
      mode: "human_surface",
      currentUrl: "https://example.com/",
      createdAtUnixMs: 1787792400000,
      lastActivityUnixMs: 1787792400100,
      error: null,
      text: "Example Domain",
    })),
    restartManagedEnvironment: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      environmentId: request.environmentId,
      ownership: "managed",
      state: "healthy",
      generation: request.expectedGeneration + 1,
      instanceId: "managed-2-1787792400000",
      processOwnership: "owned",
      lifecycleMutation: "allowed",
      readiness: "verified",
      endpoint: {
        scheme: "http",
        host: "127.0.0.1",
        port: 4318,
        source: "managed_process_output",
        verification: "owned_generation_output_and_tcp",
      },
      stopDisposition: "not_requested",
      recovery: null,
      observedAtUnixMs: 1787792400100,
      evidence: [
        {
          code: "MANAGED_ENDPOINT_VERIFIED",
          severity: "info",
          message: "The restarted generation published a verified endpoint.",
        },
      ],
    })),
    startManagedEnvironment: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      environmentId: request.environmentId,
      ownership: "managed",
      state: "healthy",
      generation: 1,
      instanceId: "managed-1-1787792400000",
      processOwnership: "owned",
      lifecycleMutation: "allowed",
      readiness: "verified",
      endpoint: {
        scheme: "http",
        host: "127.0.0.1",
        port: 4317,
        source: "managed_process_output",
        verification: "owned_generation_output_and_tcp",
      },
      stopDisposition: "not_requested",
      recovery: null,
      observedAtUnixMs: 1787792400100,
      evidence: [
        {
          code: "MANAGED_ENDPOINT_VERIFIED",
          severity: "info",
          message: "The owned generation emitted an exact loopback endpoint.",
        },
      ],
    })),
    stopManagedEnvironment: vi.fn().mockImplementation(async (request) => ({
      schemaVersion: 1,
      environmentId: request.environmentId,
      ownership: "managed",
      state: "stopped",
      generation: request.expectedGeneration,
      instanceId: null,
      processOwnership: "none",
      lifecycleMutation: "allowed",
      readiness: "not_started",
      endpoint: null,
      stopDisposition: "forced",
      recovery: null,
      observedAtUnixMs: 1787792400200,
      evidence: [
        {
          code: "MANAGED_PROCESS_TREE_STOPPED",
          severity: "info",
          message: "The retained Managed process tree stopped.",
        },
      ],
    })),
    unmountDshSurface: vi.fn().mockImplementation(async (request) =>
      surfaceStatus(request.environmentId, request.expectedGeneration, "unmounted", false),
    ),
    updateDshSurfaceLayout: vi.fn().mockImplementation(async (request) => ({
      ...surfaceStatus(
        request.environmentId,
        request.expectedGeneration,
        request.visible ? "ready" : "hidden",
        request.visible,
      ),
      bounds: request.bounds,
    })),
    validateEnvironment: vi.fn().mockResolvedValue({
      valid: true,
      issues: [],
      launchPreview: {
        source: "executable",
        executable: "dsh",
        cwd: null,
        ownership: "managed",
        endpoint: "http://127.0.0.1:0",
        arguments: [
          { category: "command", display: "web" },
          { category: "browser-policy", display: "--no-open" },
        ],
      },
    }),
  };
}

function surfaceStatus(
  environmentId: string,
  generation: number,
  state: "unmounted" | "loading" | "ready" | "hidden",
  visible: boolean,
) {
  return {
    schemaVersion: 1 as const,
    environmentId,
    generation,
    surfaceLabel: "dsh-surface" as const,
    state,
    platform: "windows" as const,
    verifiedOrigin: { scheme: "http" as const, host: "127.0.0.1" as const, port: 4317 },
    bounds: null,
    visible,
    policy: {
      sameOriginNavigation: "allow" as const,
      crossOriginNavigation: "deny" as const,
      newWindow: "deny" as const,
      downloads: "deny" as const,
      pagePermissions: "deny" as const,
      privilegedIpc: "denied" as const,
      domInjection: "denied" as const,
      automaticExternalOpen: false as const,
    },
    error: null,
  };
}

describe("ShellApp", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the unconfigured DSH boundary from backend state", async () => {
    renderShellApp(createApi());
    expect(await screen.findByText("Choose an existing DSH environment")).toBeInTheDocument();
    expect(screen.getByText("unconfigured")).toBeInTheDocument();
  });

  it("validates a setup draft through the wizard without launching DSH", async () => {
    const api = createApi();
    const user = userEvent.setup();
    renderShellApp(api);

    await screen.findByText("Choose an existing DSH environment");
    await user.click(screen.getByRole("button", { name: "Open Environment Settings" }));
    await screen.findByTestId("setup-wizard");
    // Wizard: mode (next) → harness (type repo dir, next) → profile
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:/src/deepseek-harness");
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("dsh-home"), "C:/Users/example/.dsh");
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("run-validation"));

    expect(await screen.findByTestId("validation-ok")).toBeInTheDocument();
    expect(api.validateEnvironment).toHaveBeenCalledOnce();
    expect(api.validateEnvironment).toHaveBeenCalledWith(
      expect.objectContaining({
        ownership: "managed",
        endpoint: { host: "127.0.0.1", port: "auto" },
      }),
    );
  });

  it("persists a validated environment through the wizard without starting DSH", async () => {
    const api = createApi();
    const user = userEvent.setup();
    renderShellApp(api);

    await screen.findByText("Choose an existing DSH environment");
    await user.click(screen.getByRole("button", { name: "Open Environment Settings" }));
    await screen.findByTestId("setup-wizard");
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:/src/deepseek-harness");
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("dsh-home"), "C:/Users/example/.dsh");
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("run-validation"));
    await screen.findByTestId("validation-ok");
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("finish-save"));

    expect(api.saveEnvironment).toHaveBeenCalledOnce();
    expect(await screen.findByText(/Saved at catalog revision 1/i)).toBeInTheDocument();
  });

  it("uses a launchable discovery candidate without executing it", async () => {
    const api = createApi();
    vi.mocked(api.discoverHarnesses).mockResolvedValue({
      schemaVersion: 1,
      scannedSources: ["explicit", "dsh_path", "path"],
      deferredSources: ["global"],
      candidates: [
        {
          id: "candidate-0001",
          source: "explicit",
          mode: "repository",
          requestedPath: "C:/src/deepseek-harness",
          canonicalPath: "C:/src/deepseek-harness",
          status: "available",
          launchable: true,
          version: "0.2.0",
          repository: {
            repoRoot: "C:/src/deepseek-harness",
            entry: "apps/cli/src/bin.ts",
            loader: "scripts/register-tsx-esm.mjs",
            needsInstall: false,
            needsBuild: false,
          },
          evidence: [
            {
              code: "REPO_RECOGNIZED",
              severity: "info",
              message: "Directory is a recognized DeepSeek Harness source repository.",
            },
          ],
        },
      ],
    });
    const user = userEvent.setup();
    renderShellApp(api);

    await screen.findByText("Choose an existing DSH environment");
    await user.click(screen.getByRole("button", { name: "Open Environment Settings" }));
    await screen.findByTestId("setup-wizard");
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:/src/deepseek-harness");
    await user.click(screen.getByTestId("discover-button"));
    expect(await screen.findByText("C:/src/deepseek-harness")).toBeInTheDocument();
    expect(screen.getByTestId("harness-path")).toHaveValue("C:/src/deepseek-harness");
  });

  it("restores and validates the active persisted environment on startup", async () => {
    const environment = {
      schemaVersion: 1,
      id: "local-dsh",
      label: "Restored DSH",
      harness: { mode: "executable", path: "C:/tools/dsh.exe" },
      dshHome: "C:/Users/example/.dsh",
      profile: "default",
      endpoint: { host: "127.0.0.1", port: "auto" },
      ownership: "managed",
    } satisfies DshEnvironment;
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 4,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });

    renderShellApp(api);

    expect(await screen.findByText("DSH won't start automatically")).toBeInTheDocument();
    expect(screen.getByText("Restored DSH")).toBeInTheDocument();
    expect(api.validateEnvironment).toHaveBeenCalledWith(environment);
  });

  it("renders a derived fail-closed policy without creating a remote WebView", async () => {
    const environment = attachedEnvironment();
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 5,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });

    const { container } = renderShellApp(api);

    expect(await screen.findByText("DSH view permissions")).toBeInTheDocument();
    expect(screen.getByText("http://127.0.0.1:4317")).toBeInTheDocument();
    expect(
      screen.getByText("Only a DSH instance started by this app (Managed) and verified may show its view here."),
    ).toBeInTheDocument();
    expect(api.getDshSurfacePolicy).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "attached-local",
    });
    expect(container.querySelector("iframe")).not.toBeInTheDocument();
  });

  it("keeps the DSH Surface pending when no fixed endpoint policy exists", async () => {
    const environment = {
      ...attachedEnvironment(),
      endpoint: { host: "127.0.0.1", port: "auto" },
    } satisfies DshEnvironment;
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 5,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    vi.mocked(api.getDshSurfacePolicy).mockRejectedValue({
      code: "UNAVAILABLE",
      message: "DSH Surface policy requires a fixed loopback endpoint.",
      retryable: false,
      correlationId: "desktop-test-policy",
    });

    renderShellApp(api);

    expect(await screen.findByText("Permission rules are not ready yet.")).toBeInTheDocument();
    expect(
      await screen.findByText("DSH Surface policy requires a fixed loopback endpoint."),
    ).toBeInTheDocument();
  });

  it("probes a persisted Attached environment without lifecycle controls", async () => {
    const environment = attachedEnvironment();
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 5,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    const user = userEvent.setup();

    renderShellApp(api);

    expect(await screen.findByText("attached", { selector: ".runtime-badge" })).toBeInTheDocument();
    expect(api.probeAttachedEnvironment).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "attached-local",
    });
    await user.click(screen.getByRole("button", { name: "Runtime" }));
    expect(screen.getByText("reachable")).toBeInTheDocument();
    expect(screen.getByText("unverified")).toBeInTheDocument();
    expect(screen.getByText("external")).toBeInTheDocument();
    expect(screen.getByText("denied")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /start|stop|restart/i })).not.toBeInTheDocument();
  });

  it("supports a bounded manual Attached re-probe", async () => {
    const environment = attachedEnvironment();
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 5,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    const user = userEvent.setup();
    renderShellApp(api);

    expect(await screen.findByText("attached", { selector: ".runtime-badge" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Runtime" }));
    await user.click(screen.getByRole("button", { name: "Probe again" }));
    await waitFor(() => expect(api.probeAttachedEnvironment).toHaveBeenCalledTimes(2));
  });

  it("starts and generation-bound stops only a persisted Managed environment", async () => {
    const environment = managedEnvironment();
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 6,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    const user = userEvent.setup();

    renderShellApp(api);

    expect(await screen.findByText("stopped", { selector: ".runtime-badge" })).toBeInTheDocument();
    expect(api.getManagedRuntimeStatus).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "managed-local",
    });
    await user.click(screen.getByRole("button", { name: "Runtime" }));
    await user.click(screen.getByRole("button", { name: "Start Managed DSH" }));
    expect(api.startManagedEnvironment).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "managed-local",
    });
    expect(await screen.findByText("Confirmed address of this instance: http://127.0.0.1:4317")).toBeInTheDocument();
    expect(screen.getByText("owned")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Review managed stop" }));
    expect(screen.getByRole("alertdialog", { name: "Confirm managed stop" })).toHaveTextContent(
      "generation 1",
    );
    await user.click(screen.getByRole("button", { name: "Confirm stop generation 1" }));
    expect(api.stopManagedEnvironment).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "managed-local",
      expectedGeneration: 1,
    });
    expect(await screen.findByRole("button", { name: "Start Managed DSH" })).toBeInTheDocument();
  });

  it("mounts only a verified Managed generation and hides it outside the DSH rail", async () => {
    const environment = managedEnvironment();
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 7,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    vi.mocked(api.getManagedRuntimeStatus).mockResolvedValue(healthyManagedReport());
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 120,
      y: 140,
      left: 120,
      top: 140,
      right: 920,
      bottom: 640,
      width: 800,
      height: 500,
      toJSON: () => ({}),
    } as DOMRect);
    const user = userEvent.setup();

    const { container } = renderShellApp(api);

    expect(await screen.findByText("DSH view is ready")).toBeInTheDocument();
    expect(api.mountDshSurface).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "managed-local",
      expectedGeneration: 7,
      bounds: { x: 120, y: 140, width: 800, height: 500 },
      visible: true,
    });
    expect(container.querySelector("iframe, webview, script")).not.toBeInTheDocument();
    expect(screen.getByText("The page cannot use native app features")).toBeInTheDocument();
    expect(screen.getByText("Page permission requests are denied")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Runtime" }));
    await waitFor(() => {
      expect(api.updateDshSurfaceLayout).toHaveBeenCalledWith({
        schemaVersion: 1,
        environmentId: "managed-local",
        expectedGeneration: 7,
        bounds: { x: 120, y: 140, width: 800, height: 500 },
        visible: false,
      });
    });
  });

  it("keeps the native Surface unmounted below the minimum visible bounds", async () => {
    const environment = managedEnvironment();
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 7,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    vi.mocked(api.getManagedRuntimeStatus).mockResolvedValue(healthyManagedReport());
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 70,
      y: 120,
      left: 70,
      top: 120,
      right: 310,
      bottom: 620,
      width: 240,
      height: 500,
      toJSON: () => ({}),
    } as DOMRect);

    renderShellApp(api);

    expect(await screen.findByText("Expand the window to show native DSH")).toBeInTheDocument();
    expect(screen.getByText("The DSH view needs at least 320 × 240 pixels of space.")).toBeInTheDocument();
    expect(api.mountDshSurface).not.toHaveBeenCalled();
  });

  it("offers a generation-bound reload after native lifecycle failure", async () => {
    const environment = managedEnvironment();
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 7,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    vi.mocked(api.getManagedRuntimeStatus).mockResolvedValue(healthyManagedReport());
    vi.mocked(api.mountDshSurface).mockImplementation(async (request) => ({
      ...surfaceStatus(request.environmentId, request.expectedGeneration, "ready", false),
      state: "error",
      bounds: request.bounds,
      error: {
        code: "UNAVAILABLE",
        reason: "surface_operation_failed",
        message: "Native DSH Surface operation failed.",
      },
    }));
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 80,
      y: 120,
      left: 80,
      top: 120,
      right: 880,
      bottom: 620,
      width: 800,
      height: 500,
      toJSON: () => ({}),
    } as DOMRect);
    const user = userEvent.setup();

    renderShellApp(api);

    expect(await screen.findByText("Native DSH Surface operation failed.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Reload the DSH view" }));
    expect(api.reloadDshSurface).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "managed-local",
      expectedGeneration: 7,
    });
    expect(await screen.findByText("DSH view is ready")).toBeInTheDocument();
  });

  it("degrades gracefully when Attached has no fixed port instead of probing", async () => {
    const environment = { ...attachedEnvironment(), endpoint: { host: "127.0.0.1", port: "auto" } } satisfies DshEnvironment;
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 5,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    const user = userEvent.setup();
    renderShellApp(api);

    await screen.findByText("degraded", { selector: ".runtime-badge" });
    expect(api.probeAttachedEnvironment).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Runtime" }));
    expect(screen.getByRole("alert")).toHaveTextContent("no concrete port (auto)");
  });

  it("restarts a healthy Managed runtime into a new generation", async () => {
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 6,
      activeEnvironmentId: "managed-local",
      environments: [managedEnvironment()],
    });
    vi.mocked(api.startManagedEnvironment).mockResolvedValue({
      schemaVersion: 1,
      environmentId: "managed-local",
      ownership: "managed",
      state: "healthy",
      generation: 1,
      instanceId: "managed-1-1787792400000",
      processOwnership: "owned",
      lifecycleMutation: "allowed",
      readiness: "verified",
      endpoint: {
        scheme: "http",
        host: "127.0.0.1",
        port: 4317,
        source: "managed_process_output",
        verification: "owned_generation_output_and_tcp",
      },
      stopDisposition: "not_requested",
      recovery: null,
      observedAtUnixMs: 1787792400100,
      evidence: [
        {
          code: "MANAGED_ENDPOINT_VERIFIED",
          severity: "info",
          message: "The owned generation emitted an exact loopback endpoint.",
        },
      ],
    });
    renderShellApp(api);
    await screen.findByText("stopped", { selector: ".runtime-badge" });
    await userEvent.click(screen.getByRole("button", { name: "Runtime" }));
    await screen.findByText("Start Managed DSH");
    await userEvent.click(screen.getByRole("button", { name: "Start Managed DSH" }));
    await screen.findByText("Restart managed DSH");
    await userEvent.click(screen.getByRole("button", { name: "Restart managed DSH" }));
    await waitFor(() => {
      expect(api.restartManagedEnvironment).toHaveBeenCalledWith({
        schemaVersion: 1,
        environmentId: "managed-local",
        expectedGeneration: 1,
      });
    });
    expect(
      await screen.findByText("Confirmed address of this instance: http://127.0.0.1:4318"),
    ).toBeInTheDocument();
  });
  it("switches managed environments: stop previous, activate, start target (REVIEW-M7 HIGH-1)", async () => {
    const api = createApi();
    const envA = managedEnvironment();
    const envB = { ...managedEnvironment(), id: "work-dsh", label: "Work DSH" };
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 11,
      activeEnvironmentId: envA.id,
      environments: [envA, envB],
    });
    vi.mocked(api.getManagedRuntimeStatus).mockResolvedValue(healthyManagedReport());
    vi.mocked(api.setActiveEnvironment).mockResolvedValue({
      schemaVersion: 1,
      revision: 12,
      activeEnvironmentId: envB.id,
      environments: [envA, envB],
    });
    const user = userEvent.setup();
    renderShellApp(api);

    await user.click(screen.getByRole("button", { name: /settings/i }));
    await screen.findByTestId("setup-wizard");
    await user.click(screen.getByTestId("activate-work-dsh"));

    // Ordered B1 sequence: stop the previous managed environment, persist
    // the activation, then start the target (explicit environments — no
    // stale closure values).
    await waitFor(() =>
      expect(api.stopManagedEnvironment).toHaveBeenCalledWith({
        schemaVersion: 1,
        environmentId: envA.id,
        expectedGeneration: 7,
      }),
    );
    expect(api.setActiveEnvironment).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: envB.id,
    });
    await waitFor(() =>
      expect(api.startManagedEnvironment).toHaveBeenCalledWith({
        schemaVersion: 1,
        environmentId: envB.id,
      }),
    );
    // The previous environment must never be restarted by the switch.
    expect(api.startManagedEnvironment).not.toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: envA.id,
    });
  });

  it("starts the target managed environment after switching from attached (REVIEW-M7 HIGH-1)", async () => {
    const api = createApi();
    const envAttached = attachedEnvironment();
    const envManaged = managedEnvironment();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 13,
      activeEnvironmentId: envAttached.id,
      environments: [envAttached, envManaged],
    });
    vi.mocked(api.setActiveEnvironment).mockResolvedValue({
      schemaVersion: 1,
      revision: 14,
      activeEnvironmentId: envManaged.id,
      environments: [envAttached, envManaged],
    });
    const user = userEvent.setup();
    renderShellApp(api);

    await user.click(screen.getByRole("button", { name: /settings/i }));
    await screen.findByTestId("setup-wizard");
    await user.click(screen.getByTestId("activate-managed-local"));

    // No previous managed process to stop; the target must still start
    // (the pre-fix closure bug returned early for attached previous).
    expect(api.stopManagedEnvironment).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(api.startManagedEnvironment).toHaveBeenCalledWith({
        schemaVersion: 1,
        environmentId: envManaged.id,
      }),
    );
    // The activated environment must be re-validated: validation drives the
    // DSH surface gate, so without it the surface stays on the empty
    // "choose an environment" state even while the runtime is healthy.
    await waitFor(() =>
      expect(api.validateEnvironment).toHaveBeenCalledWith(envManaged),
    );
    await user.click(screen.getByRole("button", { name: "DSH" }));
    expect(await screen.findByText("Managed DSH")).toBeInTheDocument();
    expect(
      screen.queryByText("Choose an existing DSH environment"),
    ).not.toBeInTheDocument();
  });


  it("surfaces bounded recovery history and a safe-stop start path", async () => {
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 6,
      activeEnvironmentId: "managed-local",
      environments: [managedEnvironment()],
    });
    vi.mocked(api.getManagedRuntimeStatus).mockResolvedValue({
      schemaVersion: 1,
      environmentId: "managed-local",
      ownership: "managed",
      state: "safe_stop",
      generation: 3,
      instanceId: null,
      processOwnership: "none",
      lifecycleMutation: "allowed",
      readiness: "failed",
      endpoint: null,
      stopDisposition: "not_requested",
      recovery: {
        crashCount: 3,
        windowStartUnixMs: 1787792340000,
        budget: 3,
        safeStop: true,
        lastCrashAtUnixMs: 1787792399000,
      },
      observedAtUnixMs: 1787792400100,
      evidence: [
        {
          code: "MANAGED_SAFE_STOP",
          severity: "error",
          message: "Recovery budget exhausted; the Managed generation entered Safe Stop.",
        },
      ],
    });
    vi.mocked(api.getShellSnapshot).mockResolvedValue({
      phase: "shell-mvp",
      runtimeState: "safe_stop",
      environmentId: "managed-local",
      generation: 3,
    });
    renderShellApp(api);
    await screen.findByText("safe_stop", { selector: ".runtime-badge" });
    await userEvent.click(screen.getByRole("button", { name: "Runtime" }));
    expect(await screen.findByText("Start Managed DSH")).toBeInTheDocument();
    expect(screen.getByText("3 / 3")).toBeInTheDocument();
    expect(screen.getByText("safe stop")).toBeInTheDocument();
  });

  it("renders the credential-free Diagnostics block without leaking secrets", async () => {
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 6,
      activeEnvironmentId: "managed-local",
      environments: [managedEnvironment()],
    });
    vi.mocked(api.getDiagnostics).mockResolvedValue({
      schemaVersion: 1,
      environmentId: "managed-local",
      observedAtUnixMs: 1787792400100,
      runtime: {
        state: "healthy",
        generation: 1,
        readiness: "verified",
        endpoint: { host: "127.0.0.1", port: 4317 },
        recovery: null,
      },
      surface: {
        state: "ready",
        platform: "windows",
        generation: 1,
        visible: true,
        error: null,
      },
      catalog: { revision: 6, activeEnvironmentId: "managed-local" },
      process: { retained: true, owned: true },
      evidence: [
        {
          code: "DIAGNOSTICS_COLLECTED",
          severity: "info",
          message: "Diagnostics snapshot collected.",
        },
        {
          code: "MANAGED_ENDPOINT_VERIFIED",
          severity: "info",
          message: "The owned generation emitted an exact loopback endpoint.",
        },
      ],
    });
    const user = userEvent.setup();

    renderShellApp(api);

    await screen.findByText("stopped", { selector: ".runtime-badge" });
    await user.click(screen.getByRole("button", { name: "Runtime" }));

    expect(
      await screen.findByRole("heading", { name: "Diagnostics" }),
    ).toBeInTheDocument();
    expect(api.getDiagnostics).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "managed-local",
    });
    expect(screen.getByText("127.0.0.1:4317")).toBeInTheDocument();
    expect(
      screen.getByText("The owned generation emitted an exact loopback endpoint."),
    ).toBeInTheDocument();
    expect(screen.getByText("retained · owned")).toBeInTheDocument();
    expect(screen.getByText("6")).toBeInTheDocument();
    expect(screen.queryByText(/token=/)).not.toBeInTheDocument();
    expect(screen.queryByText(/bootstrap/i)).not.toBeInTheDocument();
  });

  it("lists notifications with policy badges and dismisses one", async () => {
    const api = createApi();
    vi.mocked(api.listNotifications).mockResolvedValue([
      {
        schemaVersion: 1,
        id: "notif-1787792401000-2",
        event: "turn_completed",
        title: "Turn completed",
        contentPolicy: "explicit_body",
        deliveredBody: "Agent turn finished.",
        createdAtUnixMs: 1787792401000,
        dedupeKey: null,
        deduplicated: false,
      },
      {
        schemaVersion: 1,
        id: "notif-1787792400000-1",
        event: "runtime_changed",
        title: "Runtime changed",
        contentPolicy: "title_only",
        deliveredBody: null,
        createdAtUnixMs: 1787792400000,
        dedupeKey: null,
        deduplicated: false,
      },
    ]);
    const user = userEvent.setup();

    renderShellApp(api);

    await user.click(screen.getByRole("button", { name: "Notifications" }));
    expect(await screen.findByText("Turn completed")).toBeInTheDocument();
    expect(screen.getByText("Runtime changed")).toBeInTheDocument();
    expect(screen.getByText("explicit_body")).toBeInTheDocument();
    expect(screen.getByText("Agent turn finished.")).toBeInTheDocument();

    await user.click(screen.getAllByRole("button", { name: "Dismiss" })[0]);
    expect(api.dismissNotification).toHaveBeenCalledWith({
      schemaVersion: 1,
      notificationId: "notif-1787792401000-2",
    });
    await waitFor(() => {
      expect(screen.queryByText("Turn completed")).not.toBeInTheDocument();
    });
  });

  it("marks folded deduplicated notifications in the list", async () => {
    const api = createApi();
    vi.mocked(api.listNotifications).mockResolvedValue([
      {
        schemaVersion: 1,
        id: "notif-1787792400000-1",
        event: "schedule_result",
        title: "Schedule job finished",
        contentPolicy: "title_only",
        deliveredBody: null,
        createdAtUnixMs: 1787792400000,
        dedupeKey: "job-42",
        deduplicated: true,
      },
    ]);
    const user = userEvent.setup();

    renderShellApp(api);

    await user.click(screen.getByRole("button", { name: "Notifications" }));
    expect(await screen.findByText("Schedule job finished")).toBeInTheDocument();
    expect(screen.getByText("deduplicated")).toBeInTheDocument();
    expect(screen.getByText("title_only")).toBeInTheDocument();
  });

  it("renders the local usage snapshot totals on the Usage rail", async () => {
    const api = createApi();
    vi.mocked(api.getUsageSnapshot).mockResolvedValue({
      schemaVersion: 1,
      generatedAtUnixMs: 1787792400000,
      records: [],
      totals: { inputTokens: 1250, outputTokens: 340, estimateCount: 4, cost: 0.42, currency: "USD" },
    });
    const user = userEvent.setup();

    renderShellApp(api);

    await user.click(screen.getByRole("button", { name: "Usage" }));
    expect(await screen.findByRole("heading", { level: 2, name: "Usage" })).toBeInTheDocument();
    expect(api.getUsageSnapshot).toHaveBeenCalledWith({ schemaVersion: 1 });
    expect(screen.getByText("1250")).toBeInTheDocument();
    expect(screen.getByText("340")).toBeInTheDocument();
    expect(screen.getByText("4")).toBeInTheDocument();
    expect(screen.getByText("0.42 USD")).toBeInTheDocument();
  });

  it("marks estimate records with a badge and shows source and period only", async () => {
    const api = createApi();
    vi.mocked(api.getUsageSnapshot).mockResolvedValue({
      schemaVersion: 1,
      generatedAtUnixMs: 1787792400000,
      records: [
        {
          schemaVersion: 1,
          source: "runtime",
          period: { start: "2026-08-28T10:00:00Z", end: "2026-08-28T11:00:00Z" },
          inputTokens: 0,
          outputTokens: 0,
          isEstimate: true,
          recordedAtUnixMs: 1787792400000,
        },
        {
          schemaVersion: 1,
          source: "shell",
          period: { start: "2026-08-28T09:00:00Z", end: "2026-08-28T09:30:00Z" },
          inputTokens: 1200,
          outputTokens: 300,
          isEstimate: false,
          recordedAtUnixMs: 1787792300000,
        },
      ],
      totals: { inputTokens: 1200, outputTokens: 300, estimateCount: 1 },
    });
    const user = userEvent.setup();

    renderShellApp(api);

    await user.click(screen.getByRole("button", { name: "Usage" }));
    expect(await screen.findByText("runtime")).toBeInTheDocument();
    expect(screen.getByText("shell")).toBeInTheDocument();
    expect(screen.getAllByText("estimate")).toHaveLength(1);
    expect(screen.getByText("1200 in · 300 out")).toBeInTheDocument();
    expect(screen.getByText("0 in · 0 out")).toBeInTheDocument();
    expect(screen.queryByText(/echo bridge-ok|secret body/i)).not.toBeInTheDocument();
  });

  it("opens the Browser surface from the rail", async () => {
    const api = createApi();
    const user = userEvent.setup();
    renderShellApp(api);

    await user.click(screen.getByRole("button", { name: "Browser" }));
    expect(await screen.findByRole("heading", { name: "Browser" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Browser URL" })).toBeInTheDocument();
    expect(api.listBrowsers).toHaveBeenCalled();
  });
});

describe("language switching", () => {
  it("switches the rail copy to English and persists the choice", async () => {
    persistLang("zh");
    const api = createApi();
    const user = userEvent.setup();
    render(
      <I18nProvider>
        <ShellApp api={api} />
      </I18nProvider>,
    );

    const select = screen.getByRole("combobox", { name: "语言" });
    expect(select).toHaveValue("zh");
    expect(screen.getByTitle("计时器（M3）")).toBeInTheDocument();

    await user.selectOptions(select, "en");

    expect(select).toHaveValue("en");
    expect(screen.getByTitle("Timer (M3)")).toBeInTheDocument();
    expect(screen.queryByTitle("计时器（M3）")).not.toBeInTheDocument();
    expect(window.localStorage.getItem("dsh-lang")).toBe("en");
  });

  it("restores a persisted language on mount", async () => {
    window.localStorage.setItem("dsh-lang", "en");
    const api = createApi();
    render(
      <I18nProvider>
        <ShellApp api={api} />
      </I18nProvider>,
    );

    expect(screen.getByRole("combobox", { name: "Language" })).toHaveValue("en");
    expect(await screen.findByTitle("Timer (M3)")).toBeInTheDocument();
  });

  it("retries the bootstrap snapshot when the daemon is not connected yet", async () => {
    // BLOCK-M8E-BOOTSTRAP-STUCK regression: the daemon connector installs
    // in the background, so the first getShellSnapshot can fail; without a
    // retry the snapshot stays null and HarnessSurface renders the
    // bootstrap state forever.
    const api = createApi();
    vi.mocked(api.getShellSnapshot)
      .mockRejectedValueOnce(new Error("The daemon is not connected."))
      .mockResolvedValueOnce({
        phase: "shell-mvp",
        runtimeState: "unconfigured",
        environmentId: null,
        generation: 0,
      });
    renderShellApp(api);

    // First attempt fails: the bootstrap state is visible and the retry
    // timer is armed.
    await screen.findByText("Reading the latest runtime state…");
    await waitFor(
      () => expect(api.getShellSnapshot).toHaveBeenCalledTimes(2),
      { timeout: 5000 },
    );

    // The retried snapshot leaves the bootstrap state.
    await waitFor(
      () =>
        expect(
          screen.queryByText("Reading the latest runtime state…"),
        ).not.toBeInTheDocument(),
      { timeout: 3000 },
    );
  });
});

function attachedEnvironment(): DshEnvironment {
  return {
    schemaVersion: 1,
    id: "attached-local",
    label: "Attached DSH",
    harness: { mode: "executable", path: "C:/tools/dsh.exe" },
    dshHome: "C:/Users/example/.dsh",
    profile: "default",
    endpoint: { host: "127.0.0.1", port: 4317 },
    ownership: "attached",
  };
}

function managedEnvironment(): DshEnvironment {
  return {
    schemaVersion: 1,
    id: "managed-local",
    label: "Managed DSH",
    harness: { mode: "executable", path: "C:/tools/dsh.exe" },
    dshHome: "C:/Users/example/.dsh",
    profile: "default",
    endpoint: { host: "127.0.0.1", port: "auto" },
    ownership: "managed",
  };
}

function healthyManagedReport() {
  return {
    schemaVersion: 1 as const,
    environmentId: "managed-local",
    ownership: "managed" as const,
    state: "healthy" as const,
    generation: 7,
    instanceId: "managed-7-1787892400000",
    processOwnership: "owned" as const,
    lifecycleMutation: "allowed" as const,
    readiness: "verified" as const,
    endpoint: {
      scheme: "http" as const,
      host: "127.0.0.1" as const,
      port: 4317,
      source: "managed_process_output" as const,
      verification: "owned_generation_output_and_tcp" as const,
    },
    stopDisposition: "not_requested" as const,
    recovery: null,
    observedAtUnixMs: 1787892400000,
    evidence: [],
  };
}