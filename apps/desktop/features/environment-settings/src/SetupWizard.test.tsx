import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type {
  DshEnvironment,
  EnvironmentCatalog,
  EnvironmentValidation,
  HarnessCandidate,
  HarnessDiscoveryReport,
  RepositoryInfo,
} from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { I18nProvider, persistLang } from "../../../src/i18n";
import { SetupWizard } from "./SetupWizard";

// The wizard reads the active language from I18nProvider (no provider means
// the default zh locale). Assertions in this file are written in English, so
// the render helper pins the English locale (same pattern as ShellApp.test).
function renderWizard(api: DesktopApi, initialEnvironment: DshEnvironment | null = null) {
  persistLang("en");
  return render(
    <I18nProvider>
      <SetupWizard api={api} initialEnvironment={initialEnvironment} onSaved={vi.fn()} />
    </I18nProvider>,
  );
}

const repoInfo: RepositoryInfo = {
  repoRoot: "C:\\src\\deepseek-harness",
  entry: "apps/cli/src/bin.ts",
  loader: "scripts/register-tsx-esm.mjs",
  needsInstall: false,
  needsBuild: false,
};

function repositoryCandidate(
  id: string,
  path: string,
  overrides: Partial<HarnessCandidate> = {},
): HarnessCandidate {
  return {
    id,
    source: "explicit",
    mode: "repository",
    requestedPath: path,
    canonicalPath: path,
    status: "available",
    launchable: true,
    version: "0.2.0",
    repository: { ...repoInfo, repoRoot: path, ...(overrides.repository as Partial<RepositoryInfo>) },
    evidence: [
      {
        code: "REPO_RECOGNIZED",
        severity: "info",
        message: "Directory is a recognized DeepSeek Harness source repository.",
      },
    ],
    ...overrides,
  };
}

function discoveryReport(candidates: HarnessCandidate[]): HarnessDiscoveryReport {
  return {
    schemaVersion: 1,
    scannedSources: ["explicit"],
    deferredSources: [],
    candidates,
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
    pickDirectory: vi.fn(),
    setActiveEnvironment: vi.fn(),
    evaluateDshSurfaceNavigation: vi.fn(),
    getDshSurfacePolicy: vi.fn(),
    getDshSurfaceStatus: vi.fn(),
    updateDshSurfaceLayout: vi.fn(),
    mountDshSurface: vi.fn(),
    reloadDshSurface: vi.fn(),
    unmountDshSurface: vi.fn(),
    getEnvironmentCatalog: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      revision: 0,
      activeEnvironmentId: null,
      environments: [],
    }),
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
    source: "repository",
    executable: "node",
    cwd: "C:\\src\\deepseek-harness",
    ownership: "managed",
    endpoint: "127.0.0.1:auto",
    arguments: [],
  },
};

async function driveManagedToReview(
  api: DesktopApi,
  user: ReturnType<typeof userEvent.setup>,
  repositoryPath = "C:\\src\\deepseek-harness",
) {
  // Caller renders the wizard first.
  // Step 1: mode (Managed is default) → next
  await user.click(screen.getByTestId("wizard-next"));
  // Step 2: repository dir + probe
  await user.type(screen.getByTestId("harness-path"), repositoryPath);
  await user.click(screen.getByTestId("discover-button"));
  await waitFor(() =>
    expect(screen.getByTestId("repo-candidate-candidate-0001")).toBeInTheDocument(),
  );
  await waitFor(() =>
    expect(screen.getByTestId("harness-path")).toHaveValue(repositoryPath),
  );
  await user.click(screen.getByTestId("wizard-next"));
  // Step 3: identity is prefilled; dshHome + scan profiles
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
}

const repositoryDiscovery = discoveryReport([
  repositoryCandidate("candidate-0001", "C:\\src\\deepseek-harness"),
]);

describe("SetupWizard", () => {
  it("walks the six steps and saves a managed repository environment", async () => {
    const user = userEvent.setup();
    const saveEnvironment = vi
      .fn()
      .mockResolvedValue({
        schemaVersion: 1,
        revision: 7,
        activeEnvironmentId: "local-dsh",
        environments: [],
      } as EnvironmentCatalog);
    const startManagedEnvironment = vi.fn().mockResolvedValue({});
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(repositoryDiscovery),
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
    renderWizard(api);

    await driveManagedToReview(api, user);
    // Step 6: finish
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(startManagedEnvironment).toHaveBeenCalledWith({
        schemaVersion: 1,
        environmentId: "local-dsh",
      }),
    );
    const saved = saveEnvironment.mock.calls[0][0] as DshEnvironment;
    expect(saved.ownership).toBe("managed");
    expect(saved.profile).toBe("default");
    expect(saved.dshHome).toBe("C:\\Users\\test\\.dsh");
    expect(saved.endpoint.port).toBe(12345);
    // Repository source: mode + cwd defaults to the repo root (ADR-0020).
    expect(saved.harness.mode).toBe("repository");
    expect(saved.harness.path).toBe("C:\\src\\deepseek-harness");
    expect(saved.harness.cwd).toBe("C:\\src\\deepseek-harness");
    expect(screen.getByTestId("launch-message").textContent).toContain("starting");
  });

  it("renders repository probe details (entry/loader/install/build/version)", async () => {
    const user = userEvent.setup();
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(
        discoveryReport([
          {
            ...repositoryCandidate("candidate-0001", "C:\\src\\deepseek-harness", {
              repository: { ...repoInfo, needsInstall: true, needsBuild: true, loader: null },
            }),
            evidence: [
              {
                code: "REPO_RECOGNIZED",
                severity: "info",
                message: "Directory is a recognized DeepSeek Harness source repository.",
              },
              {
                code: "LOADER_MISSING",
                severity: "warning",
                message: "The repository is missing the TS loader (scripts/register-tsx-esm.mjs).",
              },
            ],
          },
        ]),
      ),
    });
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\deepseek-harness");
    await user.click(screen.getByTestId("discover-button"));

    expect(await screen.findByText("0.2.0")).toBeInTheDocument();
    expect(screen.getByText("apps/cli/src/bin.ts")).toBeInTheDocument();
    expect(screen.getByText("Dependencies not installed")).toBeInTheDocument();
    expect(screen.getByText("Web assets not built")).toBeInTheDocument();
    expect(screen.getByTestId("evidence-LOADER_MISSING")).toBeInTheDocument();
    expect(screen.queryByTestId("clone-guide")).not.toBeInTheDocument();
  });

  it("shows the clone guide with the target command for a non-repo directory", async () => {
    const user = userEvent.setup();
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(
        discoveryReport([
          {
            id: "candidate-0001",
            source: "explicit",
            mode: "repository",
            requestedPath: "C:\\src\\not-a-harness",
            canonicalPath: "C:\\src\\not-a-harness",
            status: "requires_recipe",
            launchable: false,
            version: null,
            evidence: [
              {
                code: "NOT_A_DSH_REPO",
                severity: "error",
                message: "The directory is not a recognized DeepSeek Harness source repository.",
              },
            ],
          },
        ]),
      ),
    });
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\not-a-harness");
    await user.click(screen.getByTestId("discover-button"));

    expect(await screen.findByTestId("clone-guide")).toBeInTheDocument();
    expect(screen.getByTestId("evidence-NOT_A_DSH_REPO")).toBeInTheDocument();
    expect(screen.getByTestId("clone-guide").textContent).toContain("git clone --depth 1");
    expect(screen.getByTestId("clone-guide").textContent).toContain("C:\\src\\not-a-harness");
  });

  it("prompts for a target directory before the clone command is concrete", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    expect(await screen.findByTestId("clone-guide")).toBeInTheDocument();
    expect(screen.getByText("Enter the target directory path above first.")).toBeInTheDocument();
  });

  it("auto-generates the profile id from the profile name", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\deepseek-harness");
    await user.click(screen.getByTestId("wizard-next"));
    // Step 3: the read-only id follows the label
    expect(screen.getByTestId("env-id-auto").textContent).toContain("local-dsh");
    const label = screen.getByTestId("env-label");
    await user.clear(label);
    await user.type(label, "My Lab DSH");
    expect(screen.getByTestId("env-id-auto").textContent).toContain("my-lab-dsh");
    // The id input itself no longer exists (machine-generated).
    expect(screen.queryByTestId("env-id")).not.toBeInTheDocument();
  });

  it("keeps the auto id valid for digit-led or empty-derived labels", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\deepseek-harness");
    await user.click(screen.getByTestId("wizard-next"));
    const label = screen.getByTestId("env-label");
    await user.clear(label);
    await user.type(label, "123 实验室");
    expect(screen.getByTestId("env-id-auto").textContent).toContain("env-123");
    // Next stays enabled (the id is valid).
    expect(screen.getByTestId("wizard-next")).toBeEnabled();
  });

  it("keeps the original id when editing an existing environment", async () => {
    const user = userEvent.setup();
    const environment: DshEnvironment = {
      schemaVersion: 1,
      id: "legacy-dsh",
      label: "Legacy DSH",
      harness: { mode: "executable", path: "C:\\tools\\dsh.exe" },
      dshHome: "C:\\Users\\test\\.dsh",
      profile: "default",
      endpoint: { host: "127.0.0.1", port: "auto" },
      ownership: "managed",
    };
    const api = makeApi();
    renderWizard(api, environment);
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("wizard-next"));
    const label = screen.getByTestId("env-label");
    await user.clear(label);
    await user.type(label, "Renamed DSH");
    expect(screen.getByTestId("env-id-auto").textContent).toContain("legacy-dsh");
  });

  it("fills the repository path from the folder browser", async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn().mockResolvedValue("C:\\src\\deepseek-harness");
    const api = makeApi({ pickDirectory });
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("browse-directory"));
    await waitFor(() =>
      expect(screen.getByTestId("harness-path")).toHaveValue("C:\\src\\deepseek-harness"),
    );
    expect(pickDirectory).toHaveBeenCalledTimes(1);
  });

  it("ignores a cancelled folder browse", async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn().mockResolvedValue(null);
    const api = makeApi({ pickDirectory });
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\typed\\path");
    await user.click(screen.getByTestId("browse-directory"));
    await waitFor(() => expect(pickDirectory).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("harness-path")).toHaveValue("C:\\typed\\path");
  });

  it("fills DSH_HOME from the folder browser on the profile step", async () => {
    const user = userEvent.setup();
    const pickDirectory = vi.fn().mockResolvedValue("C:\\Users\\test\\.dsh");
    const api = makeApi({
      pickDirectory,
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [],
      }),
    });
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\deepseek-harness");
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("browse-home"));
    await waitFor(() =>
      expect(screen.getByTestId("dsh-home")).toHaveValue("C:\\Users\\test\\.dsh"),
    );
  });

  it("keeps the legacy executable form when editing an old environment", async () => {
    const user = userEvent.setup();
    const environment: DshEnvironment = {
      schemaVersion: 1,
      id: "legacy-dsh",
      label: "Legacy DSH",
      harness: { mode: "executable", path: "C:\\tools\\dsh.exe" },
      dshHome: "C:\\Users\\test\\.dsh",
      profile: "default",
      endpoint: { host: "127.0.0.1", port: "auto" },
      ownership: "managed",
    };
    const api = makeApi();
    renderWizard(api, environment);
    await user.click(screen.getByTestId("wizard-next"));
    expect(screen.getByText("Legacy executable source (compat)")).toBeInTheDocument();
    expect(screen.getByTestId("harness-path")).toHaveValue("C:\\tools\\dsh.exe");
    // Repository-only options are hidden in the legacy form.
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("wizard-next"));
    expect(screen.queryByTestId("node-path")).not.toBeInTheDocument();
  });

  it("persists advanced repository fields (nodePath/cwd/extraArguments)", async () => {
    const user = userEvent.setup();
    const saveEnvironment = vi
      .fn()
      .mockResolvedValue({
        schemaVersion: 1,
        revision: 9,
        activeEnvironmentId: "custom-id",
        environments: [],
      } as EnvironmentCatalog);
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(repositoryDiscovery),
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [],
      }),
      probePort: vi.fn().mockResolvedValue({ schemaVersion: 1, port: 12345, inUse: false }),
      validateEnvironment: vi.fn().mockResolvedValue(validationOk),
      saveEnvironment,
    });
    renderWizard(api);

    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\deepseek-harness");
    await user.click(screen.getByTestId("discover-button"));
    await waitFor(() => expect(screen.getByTestId("harness-path")).toHaveValue("C:\\src\\deepseek-harness"));
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("dsh-home"), "C:\\Users\\test\\.dsh");
    await user.click(screen.getByTestId("wizard-next"));
    // Step 4 advanced: nodePath + cwd override + one extra argument
    await user.type(screen.getByTestId("node-path"), "C:\\Program Files\\nodejs\\node.exe");
    await user.clear(screen.getByTestId("cwd-input"));
    await user.type(screen.getByTestId("cwd-input"), "C:\\src\\deepseek-harness\\apps\\cli");
    await user.type(screen.getByTestId("extra-args"), "--verbose");
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("run-validation"));
    await waitFor(() => expect(screen.getByTestId("validation-ok")).toBeInTheDocument());
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));

    const saved = saveEnvironment.mock.calls[0][0] as DshEnvironment;
    expect(saved.nodePath).toBe("C:\\Program Files\\nodejs\\node.exe");
    expect(saved.harness.cwd).toBe("C:\\src\\deepseek-harness\\apps\\cli");
    expect(saved.harness.args).toEqual(["--verbose"]);
  });

  it("lets the user go back and change the mode", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    expect(screen.getByTestId("harness-path")).toBeInTheDocument();
    await user.click(screen.getByText("Back"));
    expect(screen.getByTestId("wizard-step-mode")).toBeInTheDocument();
  });

  it("blocks Next when required fields are empty", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    renderWizard(api);
    const next = screen.getByTestId("wizard-next");
    expect(next).toBeEnabled(); // step 1 has no requirements
    await user.click(next);
    // Step 2 starts empty in the repository form → Next stays disabled
    // until a directory is typed.
    expect(screen.getByTestId("wizard-next")).toBeDisabled();
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\deepseek-harness");
    expect(screen.getByTestId("wizard-next")).toBeEnabled();
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
      lifecycleMutation: "no",
      endpoint: { host: "127.0.0.1", port: 8080 },
      timeoutMs: 750,
      latencyMs: 2,
      observedAtUnixMs: 1787792400000,
      evidence: [],
    });
    const saveEnvironment = vi
      .fn()
      .mockResolvedValue({
        schemaVersion: 1,
        revision: 8,
        activeEnvironmentId: "attached-local",
        environments: [],
      } as EnvironmentCatalog);
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(discoveryReport([])),
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
    persistLang("en");
    render(
      <I18nProvider>
        <SetupWizard api={api} initialEnvironment={null} onSaved={onSaved} />
      </I18nProvider>,
    );

    // Choose attached on step 1 → the legacy executable placeholder form stays.
    await user.click(screen.getByLabelText(/Attached/));
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "dsh");
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("dsh-home"), "C:\\Users\\test\\.dsh");
    await user.click(screen.getByTestId("wizard-next"));
    // Advanced: attached mode needs a concrete port (auto skips probing).
    await user.clear(screen.getByTestId("port-input"));
    await user.type(screen.getByTestId("port-input"), "8080");
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("run-validation"));
    await waitFor(() => expect(screen.getByTestId("validation-ok")).toBeInTheDocument());
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));
    expect(probeAttachedEnvironment).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "local-dsh",
    });
    expect(screen.getByTestId("launch-message").textContent).toContain("reachable");
    expect(onSaved).toHaveBeenCalledTimes(1);
  });

  it("exposes repository-only options only in repository mode", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    renderWizard(api);
    // Managed repository form: node/cwd/args visible on step 4
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\deepseek-harness");
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("dsh-home"), "C:\\Users\\test\\.dsh");
    await user.click(screen.getByTestId("wizard-next"));
    expect(screen.getByTestId("node-path")).toBeInTheDocument();
    expect(screen.getByTestId("extra-args")).toBeInTheDocument();
  });

  it("saves an attached environment with auto port without probing", async () => {
    const user = userEvent.setup();
    const saveEnvironment = vi
      .fn()
      .mockResolvedValue({
        schemaVersion: 1,
        revision: 10,
        activeEnvironmentId: "local-dsh",
        environments: [],
      } as EnvironmentCatalog);
    const probeAttachedEnvironment = vi.fn().mockResolvedValue({});
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(discoveryReport([])),
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [],
      }),
      validateEnvironment: vi.fn().mockResolvedValue(validationOk),
      saveEnvironment,
      probeAttachedEnvironment,
    });
    renderWizard(api);

    await user.click(screen.getByLabelText(/Attached/));
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "dsh");
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("dsh-home"), "C:\\Users\\test\\.dsh");
    await user.click(screen.getByTestId("wizard-next"));
    // advanced keeps the default auto port
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("run-validation"));
    await waitFor(() => expect(screen.getByTestId("validation-ok")).toBeInTheDocument());
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));
    expect(probeAttachedEnvironment).not.toHaveBeenCalled();
    expect(screen.getByTestId("launch-message").textContent).toContain("Environment saved");
    expect(screen.queryByTestId("wizard-error")).not.toBeInTheDocument();
  });

  it("guides a fixed port instead of auto-starting on auto port", async () => {
    const user = userEvent.setup();
    const saveEnvironment = vi.fn().mockResolvedValue({
      schemaVersion: 1,
      revision: 14,
      activeEnvironmentId: "local-dsh",
      environments: [],
    } as EnvironmentCatalog);
    const startManagedEnvironment = vi.fn().mockResolvedValue({});
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(repositoryDiscovery),
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [],
      }),
      validateEnvironment: vi.fn().mockResolvedValue(validationOk),
      saveEnvironment,
      startManagedEnvironment,
    });
    renderWizard(api);
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("harness-path"), "C:\\src\\deepseek-harness");
    await user.click(screen.getByTestId("discover-button"));
    await waitFor(() => expect(screen.getByTestId("harness-path")).toHaveValue("C:\\src\\deepseek-harness"));
    await user.click(screen.getByTestId("wizard-next"));
    await user.type(screen.getByTestId("dsh-home"), "C:\\Users\\test\\.dsh");
    await user.click(screen.getByTestId("wizard-next"));
    // advanced keeps the default auto port
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("run-validation"));
    await waitFor(() => expect(screen.getByTestId("validation-ok")).toBeInTheDocument());
    await user.click(screen.getByTestId("wizard-next"));
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));
    expect(startManagedEnvironment).not.toHaveBeenCalled();
    expect(screen.getByTestId("launch-message").textContent).toContain("fixed port");
  });

  it("refuses to overwrite an existing environment with a colliding id", async () => {
    const user = userEvent.setup();
    const saveEnvironment = vi.fn().mockResolvedValue({
      schemaVersion: 1,
      revision: 12,
      activeEnvironmentId: "local-dsh",
      environments: [],
    } as EnvironmentCatalog);
    const startManagedEnvironment = vi.fn().mockResolvedValue({});
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(repositoryDiscovery),
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [],
      }),
      probePort: vi.fn().mockResolvedValue({ schemaVersion: 1, port: 12345, inUse: false }),
      validateEnvironment: vi.fn().mockResolvedValue(validationOk),
      saveEnvironment,
      startManagedEnvironment,
      getEnvironmentCatalog: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        revision: 3,
        activeEnvironmentId: "local-dsh",
        environments: [
          {
            schemaVersion: 1,
            id: "local-dsh",
            label: "Local DSH",
            harness: { mode: "repository", path: "C:\\src\\other" },
            dshHome: "C:\\Users\\test\\.dsh",
            profile: "default",
            endpoint: { host: "127.0.0.1", port: "auto" },
            ownership: "managed",
          },
        ],
      }),
    });
    renderWizard(api);
    await driveManagedToReview(api, user);
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() =>
      expect(screen.getByTestId("wizard-error").textContent).toContain("already used"),
    );
    expect(saveEnvironment).not.toHaveBeenCalled();
    expect(startManagedEnvironment).not.toHaveBeenCalled();
  });

  it("skips auto-start when the repository is not installed or built", async () => {
    const user = userEvent.setup();
    const saveEnvironment = vi.fn().mockResolvedValue({
      schemaVersion: 1,
      revision: 13,
      activeEnvironmentId: "local-dsh",
      environments: [],
    } as EnvironmentCatalog);
    const startManagedEnvironment = vi.fn().mockResolvedValue({});
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(
        discoveryReport([
          repositoryCandidate("candidate-0001", "C:\\src\\fresh-clone", {
            repository: { ...repoInfo, repoRoot: "C:\\src\\fresh-clone", needsInstall: true, needsBuild: true },
          }),
        ]),
      ),
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [],
      }),
      probePort: vi.fn().mockResolvedValue({ schemaVersion: 1, port: 12345, inUse: false }),
      validateEnvironment: vi.fn().mockResolvedValue(validationOk),
      saveEnvironment,
      startManagedEnvironment,
    });
    renderWizard(api);
    await driveManagedToReview(api, user, "C:\\src\\fresh-clone");
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));
    expect(startManagedEnvironment).not.toHaveBeenCalled();
    expect(screen.getByTestId("launch-message").textContent).toContain("Environment saved");
  });

  it("reports launch failure separately after a successful save", async () => {
    const user = userEvent.setup();
    const saveEnvironment = vi
      .fn()
      .mockResolvedValue({
        schemaVersion: 1,
        revision: 11,
        activeEnvironmentId: "local-dsh",
        environments: [],
      } as EnvironmentCatalog);
    const startManagedEnvironment = vi
      .fn()
      .mockRejectedValue(new Error("Managed start source is missing"));
    const api = makeApi({
      discoverHarnesses: vi.fn().mockResolvedValue(repositoryDiscovery),
      discoverProfiles: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        dshHome: "C:\\Users\\test\\.dsh",
        profiles: [],
      }),
      probePort: vi.fn().mockResolvedValue({ schemaVersion: 1, port: 12345, inUse: false }),
      validateEnvironment: vi.fn().mockResolvedValue(validationOk),
      saveEnvironment,
      startManagedEnvironment,
    });
    renderWizard(api);
    await driveManagedToReview(api, user);
    await user.click(screen.getByTestId("finish-save"));
    await waitFor(() => expect(saveEnvironment).toHaveBeenCalledTimes(1));
    // The save succeeded (revision badge) and the launch error is visible.
    expect(screen.getByTestId("saved-revision").textContent).toContain("11");
    expect(screen.getByTestId("wizard-error").textContent).toContain(
      "Managed start source is missing",
    );
  });

});