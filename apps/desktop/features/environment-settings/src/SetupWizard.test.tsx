import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type {
  DshEnvironment,
  EnvironmentCatalog,
  EnvironmentValidation,
  HarnessDiscoveryReport,
  HarnessCandidate,
} from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { SetupWizard } from "./SetupWizard";

function candidate(id: string, path: string, status: HarnessCandidate["status"]): HarnessCandidate {
  return {
    id,
    source: "path",
    mode: "executable",
    requestedPath: path,
    canonicalPath: path,
    status,
    launchable: status === "available",
    version: null,
    evidence: [],
  };
}

function makeApi(overrides: Partial<DesktopApi> = {}): DesktopApi {
  return {
    dismissNotification: vi.fn(),
    listBrowsers: vi.fn(),
    listNotifications: vi.fn(),
    navigateBrowser: vi.fn(),
    notifyApplication: vi.fn(),
    snapshotBrowser: vi.fn(),
    closeBrowser: vi.fn(),
    createBrowser: vi.fn(),
    closeTerminal: vi.fn(),
    createTerminal: vi.fn(),
    listTerminals: vi.fn(),
    resizeTerminal: vi.fn(),
    statusTerminal: vi.fn(),
    writeTerminal: vi.fn(),
    discoverHarnesses: vi.fn(),
    discoverProfiles: vi.fn(),
    probePort: vi.fn(),
    setActiveEnvironment: vi.fn(),
    evaluateDshSurfaceNavigation: vi.fn(),
    getDshSurfacePolicy: vi.fn(),
    getDshSurfaceStatus: vi.fn(),
    updateDshSurfaceLayout: vi.fn(),
    mountDshSurface: vi.fn(),
    reloadDshSurface: vi.fn(),
    unmountDshSurface: vi.fn(),
    getEnvironmentCatalog: vi.fn(),
    getManagedRuntimeStatus: vi.fn(),
    getShellSnapshot: vi.fn(),
    getUsageSnapshot: vi.fn(),
    getDiagnostics: vi.fn(),
    probeAttachedEnvironment: vi.fn(),
    saveEnvironment: vi.fn(),
    startManagedEnvironment: vi.fn(),
    stopManagedEnvironment: vi.fn(),
    restartManagedEnvironment: vi.fn(),
    validateEnvironment: vi.fn(),
    ...overrides,
  };
}

const validationOk: EnvironmentValidation = {
  valid: true,
  issues: [],
  launchPreview: {
    source: "executable",
    executable: "dsh",
    cwd: null,
    ownership: "managed",
    endpoint: "127.0.0.1:auto",
    arguments: [],
  },
};

async function driveToReview(
  api: DesktopApi,
  user: ReturnType<typeof userEvent.setup>,
  onSaved: (() => void) | undefined = undefined,
) {
  render(<SetupWizard api={api} initialEnvironment={null} onSaved={onSaved ?? vi.fn()} />);
  // Step 1: mode (Managed is default) → next
  await user.click(screen.getByTestId("wizard-next"));
  // Step 2: harness path + search
  await user.type(screen.getByTestId("harness-path"), "dsh");
  await user.click(screen.getByTestId("discover-button"));
  await waitFor(() => expect(screen.getByTestId("candidate-list")).toBeInTheDocument());
  await user.click(screen.getByTestId("wizard-next"));
  // Step 3: dshHome + scan profiles
  await user.type(screen.getByTestId("dsh-home"), "C:\\Users\\test\\.dsh");
  await user.click(screen.getByTestId("scan-profiles"));
  await waitFor(() => expect(screen.getByTestId("profile-list")).toBeInTheDocument());
  await user.click(screen.getByTestId("wizard-next"));
  // Step 4: port probe
  await user.clear(screen.getByTestId("port-input"));
  await user.type(screen.getByTestId("port-input"), "12345");
  await user.click(screen.getByTestId("probe-port"));
  await waitFor(() => expect(screen.getByTestId("probe-result")).toBeInTheDocument());
  await user.click(screen.getByTestId("wizard-next"));
  // Step 5: review
  await user.click(screen.getByTestId("run-validation"));
  await waitFor(() => expect(screen.getByTestId("validation-ok")).toBeInTheDocument());
  await user.click(screen.getByTestId("wizard-next"));
  return user;
}

describe("SetupWizard", () => {
  it("walks the six steps and saves a managed environment", async () => {
    const user = userEvent.setup();
    const saveEnvironment = vi.fn().mockResolvedValue({ schemaVersion: 1, revision: 7, activeEnvironmentId: "local-dsh", environments: [] });
    const startManagedEnvironment = vi.fn().mockResolvedValue({});
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        scannedSources: ["path"],
        deferredSources: [],
        candidates: [candidate("candidate-0001", "C:\\tools\\dsh.exe", "available")],
      } as HarnessDiscoveryReport),
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [
          { name: "default", path: "C:\\Users\\test\\.dsh\\profiles\\default", hasRootConfig: true },
        ],
      }),
      probePort: vi.fn().mockResolvedValue({ schemaVersion: 1, port: 12345, inUse: false }),
      validateEnvironment: vi.fn().mockResolvedValue(validationOk),
      saveEnvironment,
      startManagedEnvironment,
    });
    const onSaved = vi.fn();

    await driveToReview(api, user, onSaved);
    // Step 6: finish
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(startManagedEnvironment).toHaveBeenCalledWith({ schemaVersion: 1, environmentId: "local-dsh" }));
    expect(onSaved).toHaveBeenCalledTimes(1);
    const saved = saveEnvironment.mock.calls[0][0] as DshEnvironment;
    expect(saved.ownership).toBe("managed");
    expect(saved.profile).toBe("default");
    expect(saved.dshHome).toBe("C:\\Users\\test\\.dsh");
    expect(saved.endpoint.port).toBe(12345);
    expect(screen.getByTestId("launch-message").textContent).toContain("starting");
  });

  it("lets the user go back and change the mode", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    render(<SetupWizard api={api} initialEnvironment={null} onSaved={vi.fn()} />);
    await user.click(screen.getByTestId("wizard-next"));
    expect(screen.getByTestId("harness-path")).toBeInTheDocument();
    await user.click(screen.getByText("Back"));
    expect(screen.getByTestId("wizard-step-mode")).toBeInTheDocument();
  });

  it("blocks Next when required fields are empty", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    render(<SetupWizard api={api} initialEnvironment={null} onSaved={vi.fn()} />);
    const next = screen.getByTestId("wizard-next");
    expect(next).toBeEnabled(); // step 1 has no requirements
    await user.click(next);
    // Step 2: clear the prefilled path → Next must be disabled.
    await user.clear(screen.getByTestId("harness-path"));
    expect(screen.getByTestId("wizard-next")).toBeDisabled();
  });

  it("reports attached save without starting managed", async () => {
    const user = userEvent.setup();
    const probeAttachedEnvironment = vi.fn().mockResolvedValue({
      schemaVersion: 1,
      environmentId: "attached-local",
      ownership: "attached",
      state: "attached",
      reachability: "reachable",
      identity: "unverified",
      processOwnership: "external",
      lifecycleMutation: "denied",
      endpoint: { host: "127.0.0.1", port: 8080 },
      timeoutMs: 750,
      latencyMs: 2,
      observedAtUnixMs: 1787792400000,
      evidence: [],
    });
    const saveEnvironment = vi.fn().mockResolvedValue({ schemaVersion: 1, revision: 8, activeEnvironmentId: "attached-local", environments: [] });
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        scannedSources: ["path"],
        deferredSources: [],
        candidates: [candidate("candidate-0001", "dsh", "available")],
      } as HarnessDiscoveryReport),
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [],
      }),
      probePort: vi.fn().mockResolvedValue({ schemaVersion: 1, port: 8080, inUse: true }),
      validateEnvironment: vi.fn().mockResolvedValue({
        ...validationOk,
        launchPreview: { ...validationOk.launchPreview!, ownership: "attached" },
      }),
      saveEnvironment,
      probeAttachedEnvironment,
    });
    const onSaved = vi.fn();

    // Choose attached on step 1
    render(<SetupWizard api={api} initialEnvironment={null} onSaved={onSaved} />);
    await user.click(screen.getByLabelText(/Attached/));
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "dsh");
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("dsh-home"), "C:\\Users\\test\\.dsh");
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("run-validation"));
    await waitFor(() => expect(screen.getByTestId("validation-ok")).toBeInTheDocument());
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));
    expect(probeAttachedEnvironment).toHaveBeenCalledWith({ schemaVersion: 1, environmentId: "local-dsh" });
    expect(screen.getByTestId("launch-message").textContent).toContain("reachable");
  });
});