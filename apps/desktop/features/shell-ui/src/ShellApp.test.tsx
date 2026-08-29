import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { DshEnvironment } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { ShellApp } from "./ShellApp";

function createApi(): DesktopApi {
  return {
    discoverHarnesses: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      scannedSources: ["explicit", "dsh_path", "path"],
      deferredSources: ["global"],
      candidates: [],
    }),
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
      domInjection: "denied",
      rendererPatch: "denied",
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
    render(<ShellApp api={createApi()} />);
    expect(await screen.findByText("Choose an existing DSH environment")).toBeInTheDocument();
    expect(screen.getByText("unconfigured")).toBeInTheDocument();
  });

  it("validates a setup draft without installing or launching DSH", async () => {
    const api = createApi();
    const user = userEvent.setup();
    render(<ShellApp api={api} />);

    await screen.findByText("Choose an existing DSH environment");
    await user.click(screen.getByRole("button", { name: "Open Environment Settings" }));
    await user.type(screen.getByLabelText("DSH_HOME"), "C:/Users/example/.dsh");
    await user.click(screen.getByRole("button", { name: "Validate environment" }));

    expect(await screen.findByLabelText("Redacted launch preview")).toBeInTheDocument();
    expect(api.validateEnvironment).toHaveBeenCalledOnce();
    expect(api.validateEnvironment).toHaveBeenCalledWith(
      expect.objectContaining({
        ownership: "managed",
        endpoint: { host: "127.0.0.1", port: "auto" },
      }),
    );
  });

  it("persists a validated environment without starting DSH", async () => {
    const api = createApi();
    const user = userEvent.setup();
    render(<ShellApp api={api} />);

    await screen.findByText("Choose an existing DSH environment");
    await user.click(screen.getByRole("button", { name: "Open Environment Settings" }));
    await user.type(screen.getByLabelText("DSH_HOME"), "C:/Users/example/.dsh");
    await user.click(screen.getByRole("button", { name: "Validate environment" }));
    await user.click(await screen.findByRole("button", { name: "Save active environment" }));

    expect(api.saveEnvironment).toHaveBeenCalledOnce();
    expect(await screen.findByText(/active catalog revision 1/i)).toBeInTheDocument();
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
          source: "path",
          mode: "executable",
          requestedPath: "C:/tools/dsh.exe",
          canonicalPath: "C:/tools/dsh.exe",
          status: "available",
          launchable: true,
          version: null,
          evidence: [
            { code: "FILE_CANDIDATE", severity: "info", message: "Candidate was not executed." },
          ],
        },
      ],
    });
    const user = userEvent.setup();
    render(<ShellApp api={api} />);

    await screen.findByText("Choose an existing DSH environment");
    await user.click(screen.getByRole("button", { name: "Open Environment Settings" }));
    await user.click(screen.getByRole("button", { name: "Discover harnesses" }));
    expect(await screen.findByText("C:/tools/dsh.exe")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Use candidate" }));
    expect(screen.getByLabelText("Executable or recipe path")).toHaveValue("C:/tools/dsh.exe");
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

    render(<ShellApp api={api} />);

    expect(await screen.findByText("DSH launch remains intentionally idle")).toBeInTheDocument();
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

    const { container } = render(<ShellApp api={api} />);

    expect(await screen.findByText("DSH Surface policy ready")).toBeInTheDocument();
    expect(screen.getByText("http://127.0.0.1:4317")).toBeInTheDocument();
    expect(
      screen.getByText("A native Surface requires a verified, owned Managed generation."),
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

    render(<ShellApp api={api} />);

    expect(await screen.findByText("DSH Surface policy pending.")).toBeInTheDocument();
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

    render(<ShellApp api={api} />);

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
    render(<ShellApp api={api} />);

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

    render(<ShellApp api={api} />);

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
    expect(await screen.findByText("Verified endpoint: http://127.0.0.1:4317")).toBeInTheDocument();
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

    const { container } = render(<ShellApp api={api} />);

    expect(await screen.findByText("Native DSH Surface ready")).toBeInTheDocument();
    expect(api.mountDshSurface).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "managed-local",
      expectedGeneration: 7,
      bounds: { x: 120, y: 140, width: 800, height: 500 },
      visible: true,
    });
    expect(container.querySelector("iframe, webview, script")).not.toBeInTheDocument();
    expect(screen.getByText("Native IPC denied")).toBeInTheDocument();
    expect(screen.getByText("Page permissions denied")).toBeInTheDocument();

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

    render(<ShellApp api={api} />);

    expect(await screen.findByText("Expand the window to show native DSH")).toBeInTheDocument();
    expect(screen.getByText("The native Surface requires at least 320 × 240 visible CSS pixels.")).toBeInTheDocument();
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

    render(<ShellApp api={api} />);

    expect(await screen.findByText("Native DSH Surface operation failed.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry native Surface" }));
    expect(api.reloadDshSurface).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "managed-local",
      expectedGeneration: 7,
    });
    expect(await screen.findByText("Native DSH Surface ready")).toBeInTheDocument();
  });

  it("renders a safe error when Attached has no fixed endpoint", async () => {
    const environment = { ...attachedEnvironment(), endpoint: { host: "127.0.0.1", port: "auto" } } satisfies DshEnvironment;
    const api = createApi();
    vi.mocked(api.getEnvironmentCatalog).mockResolvedValue({
      schemaVersion: 1,
      revision: 5,
      activeEnvironmentId: environment.id,
      environments: [environment],
    });
    vi.mocked(api.probeAttachedEnvironment).mockRejectedValue({
      code: "UNAVAILABLE",
      message: "Attached health requires a fixed loopback port.",
      retryable: false,
      correlationId: "desktop-test-1",
    });
    const user = userEvent.setup();
    render(<ShellApp api={api} />);

    await screen.findByText("unavailable", { selector: ".runtime-badge" });
    await user.click(screen.getByRole("button", { name: "Runtime" }));
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Attached health requires a fixed loopback port.",
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
    observedAtUnixMs: 1787892400000,
    evidence: [],
  };
}
