// Environment partition edit form (env quick-edit D3): sectioned, step-less
// dialog for editing an existing environment. Tests run without an
// I18nProvider, so the default zh locale applies; assertions prefer testids
// and input values over copy.

import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";

import type {
  DiscoverProfilesReport,
  DshEnvironment,
  EnvironmentCatalog,
  EnvironmentValidation,
} from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { EnvironmentEditForm } from "./EnvironmentEditForm";

function managedRepoEnvironment(overrides: Partial<DshEnvironment> = {}): DshEnvironment {
  return {
    schemaVersion: 1,
    id: "dev-repo",
    label: "Dev Repo",
    harness: {
      mode: "repository",
      path: "C:\\src\\deepseek-harness",
      cwd: "C:\\src\\deepseek-harness",
    },
    dshHome: "C:\\Users\\dev\\.dsh",
    profile: "dev",
    nodePath: "C:\\Program Files\\nodejs\\node.exe",
    endpoint: { host: "127.0.0.1", port: 3081 },
    ownership: "managed",
    policy: { autoRestartOnCrash: true, allowNativeAdapter: false },
    ...overrides,
  };
}

function attachedEnvironment(overrides: Partial<DshEnvironment> = {}): DshEnvironment {
  return {
    schemaVersion: 1,
    id: "prod-dsh",
    label: "Prod Attached",
    harness: { mode: "executable", path: "dsh" },
    dshHome: "C:\\Users\\ops\\.dsh",
    profile: "default",
    endpoint: { host: "127.0.0.1", port: 3080 },
    ownership: "attached",
    ...overrides,
  };
}

function catalogOf(environment: DshEnvironment): EnvironmentCatalog {
  return {
    schemaVersion: 1,
    revision: 4,
    activeEnvironmentId: environment.id,
    environments: [environment],
  };
}

function validationOk(environment: DshEnvironment): EnvironmentValidation {
  return {
    valid: true,
    issues: [],
    launchPreview: {
      source: environment.harness.mode,
      executable: environment.harness.path,
      cwd: environment.harness.cwd ?? null,
      ownership: environment.ownership,
      endpoint: "127.0.0.1:" + environment.endpoint.port,
      arguments: [],
    },
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
    getEnvironmentCatalog: vi.fn(),
    getManagedRuntimeStatus: vi.fn(),
    getShellSnapshot: vi.fn(),
    getUsageSnapshot: vi.fn(),
    getDiagnostics: vi.fn(),
    probeAttachedEnvironment: vi.fn(),
    removeEnvironment: vi.fn(),
    saveEnvironment: vi.fn(),
    startManagedEnvironment: vi.fn(),
    stopManagedEnvironment: vi.fn(),
    restartManagedEnvironment: vi.fn(),
    validateEnvironment: vi.fn(),
    ...overrides,
  };
}

type Props = ComponentProps<typeof EnvironmentEditForm>;

function renderEdit(overrides: Partial<Props> = {}) {
  const environment = overrides.environment ?? managedRepoEnvironment();
  const api = overrides.api ?? makeApi();
  const defaults: Props = {
    api,
    environment,
    catalog: overrides.catalog ?? catalogOf(environment),
    busy: false,
    onClose: vi.fn(),
    onSaved: vi.fn(),
  };
  return {
    api,
    environment,
    ...render(<EnvironmentEditForm {...defaults} {...overrides} />),
  };
}

describe("EnvironmentEditForm", () => {
  it("renders every section prefilled from the environment; id and ownership are read-only", () => {
    renderEdit();
    for (const section of ["name", "source", "data", "endpoint", "advanced", "ownership"]) {
      expect(screen.getByTestId("edit-section-" + section)).toBeInTheDocument();
    }
    expect(screen.getByTestId("edit-title")).toHaveTextContent("Dev Repo");
    expect(screen.getByTestId("edit-label")).toHaveValue("Dev Repo");
    expect(screen.getByTestId("edit-harness-path")).toHaveValue("C:\\src\\deepseek-harness");
    expect(screen.getByTestId("edit-cwd")).toHaveTextContent("C:\\src\\deepseek-harness");
    expect(screen.getByTestId("edit-dsh-home")).toHaveValue("C:\\Users\\dev\\.dsh");
    expect(screen.getByTestId("edit-profile")).toHaveValue("dev");
    expect(screen.getByTestId("edit-port")).toHaveValue("3081");
    expect(screen.getByTestId("edit-node-path")).toHaveValue(
      "C:\\Program Files\\nodejs\\node.exe",
    );
    // id is a read-only badge (no editable input carries it).
    expect(screen.getByTestId("edit-id")).toHaveTextContent("dev-repo");
    expect(screen.getByTestId("edit-id-note")).toBeInTheDocument();
    // ownership badge + locked note; localized mode badge.
    expect(screen.getByTestId("edit-ownership")).toHaveTextContent("Managed");
    expect(screen.getByTestId("edit-ownership-note")).toBeInTheDocument();
    expect(screen.getByTestId("edit-mode")).toHaveTextContent("源码仓库");
    // policy values are shown read-only as boolean badges.
    expect(screen.getByTestId("edit-policy-autorestart")).toHaveTextContent("true");
    expect(screen.getByTestId("edit-policy-adapter")).toHaveTextContent("false");
    // a clean environment starts without backend-issue or error banners.
    expect(screen.queryByTestId("edit-issues")).not.toBeInTheDocument();
    expect(screen.queryByTestId("edit-error")).not.toBeInTheDocument();
  });

  it("saves an edited label under the same id through validate -> save -> onSaved", async () => {
    const user = userEvent.setup();
    const environment = managedRepoEnvironment();
    const validate = vi.fn().mockResolvedValue(validationOk(environment));
    const save = vi.fn().mockResolvedValue({ ...catalogOf(environment), revision: 9 });
    const api = makeApi({ validateEnvironment: validate, saveEnvironment: save });
    const onSaved = vi.fn();
    renderEdit({ api, environment, onSaved });

    await user.clear(screen.getByTestId("edit-label"));
    await user.type(screen.getByTestId("edit-label"), "Renamed Repo");
    await user.click(screen.getByTestId("edit-save"));

    await waitFor(() => expect(validate).toHaveBeenCalledTimes(1));
    const validated = validate.mock.calls[0][0] as DshEnvironment;
    expect(validated.id).toBe("dev-repo");
    expect(validated.label).toBe("Renamed Repo");
    expect(validated.ownership).toBe("managed");
    expect(validated.dshHome).toBe("C:\\Users\\dev\\.dsh");
    expect(validated.profile).toBe("dev");
    expect(validated.endpoint.port).toBe(3081);
    expect(validated.harness.mode).toBe("repository");

    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    const saved = save.mock.calls[0][0] as DshEnvironment;
    expect(saved.id).toBe("dev-repo");
    expect(saved.label).toBe("Renamed Repo");

    await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1));
    const [savedEnv, savedCatalog, result] = onSaved.mock.calls[0] as [
      DshEnvironment,
      EnvironmentCatalog,
      EnvironmentValidation,
    ];
    expect(savedEnv.id).toBe("dev-repo");
    expect(savedCatalog.revision).toBe(9);
    expect(result.valid).toBe(true);
  });

  it("blocks saving on an invalid port and shows the field error", async () => {
    const user = userEvent.setup();
    const validate = vi.fn().mockResolvedValue(validationOk(managedRepoEnvironment()));
    const api = makeApi({ validateEnvironment: validate });
    renderEdit({ api });

    await user.clear(screen.getByTestId("edit-port"));
    await user.type(screen.getByTestId("edit-port"), "abc");

    expect(screen.getByTestId("edit-save")).toBeDisabled();
    const error = screen.getByTestId("edit-port-error");
    expect(error).toHaveAttribute("role", "alert");
    expect(error.textContent).toMatch(/1024 to 65535/);
    expect(validate).not.toHaveBeenCalled();
    expect(api.saveEnvironment).not.toHaveBeenCalled();
  });

  it("shows backend issues when validation fails and never calls save", async () => {
    const user = userEvent.setup();
    const environment = managedRepoEnvironment();
    const validate = vi.fn().mockResolvedValue({
      valid: false,
      issues: [
        {
          field: "endpoint.port",
          code: "PORT_IN_USE",
          message: "Port 3081 is already in use.",
        },
      ],
      launchPreview: null,
    });
    const save = vi.fn();
    const api = makeApi({ validateEnvironment: validate, saveEnvironment: save });
    const onSaved = vi.fn();
    renderEdit({ api, environment, onSaved });

    await user.click(screen.getByTestId("edit-save"));
    await waitFor(() => expect(validate).toHaveBeenCalledTimes(1));

    const issues = screen.getByTestId("edit-issues");
    expect(issues).toHaveAttribute("role", "alert");
    expect(within(issues).getByText(/already in use/)).toBeInTheDocument();
    expect(within(issues).getByText("endpoint.port")).toBeInTheDocument();
    expect(save).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
    // Save stays available for a retry after the user fixes the problem.
    expect(screen.getByTestId("edit-save")).toBeEnabled();
  });

  it("dismisses through the cancel button without touching the api", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    const onClose = vi.fn();
    renderEdit({ api, onClose });

    await user.click(screen.getByTestId("edit-cancel"));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(api.validateEnvironment).not.toHaveBeenCalled();
    expect(api.saveEnvironment).not.toHaveBeenCalled();
  });

  it("only shows the nodePath editor for managed repository environments", () => {
    // managed + repository: shown (also covered by the first test).
    const repo = renderEdit();
    expect(repo.queryByTestId("edit-node-path")).not.toBeNull();
    repo.unmount();
    // attached: nodePath is not editable.
    const attached = renderEdit({ environment: attachedEnvironment() });
    expect(attached.queryByTestId("edit-node-path")).not.toBeInTheDocument();
    expect(attached.getByTestId("edit-section-advanced")).toBeInTheDocument();
    attached.unmount();
    // managed + executable: not editable either.
    const executable = renderEdit({
      environment: managedRepoEnvironment({
        id: "legacy-dsh",
        label: "Legacy Exec",
        harness: { mode: "executable", path: "dsh" },
        nodePath: undefined,
        endpoint: { host: "127.0.0.1", port: "auto" },
        policy: undefined,
      }),
    });
    expect(executable.queryByTestId("edit-node-path")).not.toBeInTheDocument();
  });

  it("fills harness.path and dshHome from the directory picker", async () => {
    const user = userEvent.setup();
    const pickDirectory = vi
      .fn()
      .mockResolvedValueOnce("C:\\picked\\repo")
      .mockResolvedValueOnce("C:\\picked\\home");
    const api = makeApi({ pickDirectory });
    renderEdit({ api });

    await user.click(screen.getByTestId("edit-browse-harness"));
    await waitFor(() =>
      expect(screen.getByTestId("edit-harness-path")).toHaveValue("C:\\picked\\repo"),
    );
    await user.click(screen.getByTestId("edit-browse-home"));
    await waitFor(() =>
      expect(screen.getByTestId("edit-dsh-home")).toHaveValue("C:\\picked\\home"),
    );
    expect(pickDirectory).toHaveBeenCalledTimes(2);
  });

  it("lists scanned profiles and lets the user pick one; dshHome edits clear the list", async () => {
    const user = userEvent.setup();
    const environment = managedRepoEnvironment();
    const report: DiscoverProfilesReport = {
      schemaVersion: 1,
      dshHome: environment.dshHome,
      profiles: [
        { name: "dev", path: "C:\\Users\\dev\\.dsh\\profiles\\dev", hasRootConfig: true },
        {
          name: "sandbox",
          path: "C:\\Users\\dev\\.dsh\\profiles\\sandbox",
          hasRootConfig: false,
        },
      ],
    };
    const discoverProfiles = vi.fn().mockResolvedValue(report);
    const api = makeApi({ discoverProfiles });
    renderEdit({ api, environment });

    await user.click(screen.getByTestId("edit-scan-profiles"));
    await waitFor(() =>
      expect(discoverProfiles).toHaveBeenCalledWith({
        schemaVersion: 1,
        dshHome: "C:\\Users\\dev\\.dsh",
      }),
    );
    const options = await screen.findByTestId("edit-profile-options");
    expect(within(options).getByText("sandbox")).toBeInTheDocument();
    // The radio's accessible name includes the "no cordis.yml" warning text.
    await user.click(within(options).getByRole("radio", { name: /sandbox/ }));
    expect(screen.getByTestId("edit-profile")).toHaveValue("sandbox");

    // Changing dshHome invalidates previously scanned candidates.
    await user.clear(screen.getByTestId("edit-dsh-home"));
    await user.type(screen.getByTestId("edit-dsh-home"), "C:\\other\\.dsh");
    expect(screen.queryByTestId("edit-profile-options")).not.toBeInTheDocument();
  });

  it("keeps every commit control disabled while the ShellApp is busy", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    const onClose = vi.fn();
    renderEdit({ api, busy: true, onClose });

    expect(screen.getByTestId("edit-save")).toBeDisabled();
    expect(screen.getByTestId("edit-cancel")).toBeDisabled();
    expect(screen.getByTestId("edit-close")).toBeDisabled();
    expect(screen.getByTestId("edit-browse-harness")).toBeDisabled();
    await user.click(screen.getByTestId("edit-close")).catch(() => undefined);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("explains the effective cwd per mode when none is configured", () => {
    // repository + empty cwd: defaults to the repository root.
    const repo = renderEdit({
      environment: managedRepoEnvironment({
        harness: { mode: "repository", path: "C:\\src\\deepseek-harness" },
      }),
    });
    expect(repo.getByTestId("edit-cwd")).toHaveTextContent("默认使用仓库根目录");
    repo.unmount();
    // executable + empty cwd: not set.
    const exec = renderEdit({ environment: attachedEnvironment() });
    expect(exec.getByTestId("edit-cwd")).toHaveTextContent("未设置");
  });

  it("shows ownership-appropriate port guidance", () => {
    const managed = renderEdit({ environment: managedRepoEnvironment() });
    expect(managed.getByTestId("edit-port-hint")).toHaveTextContent("固定端口");
    managed.unmount();
    const attached = renderEdit({ environment: attachedEnvironment() });
    expect(attached.getByTestId("edit-port-hint")).toHaveTextContent("实际端口");
  });

  it("surfaces api failures as an alert banner and keeps the dialog open", async () => {
    const user = userEvent.setup();
    const environment = managedRepoEnvironment();
    const validate = vi.fn().mockResolvedValue(validationOk(environment));
    const save = vi.fn().mockRejectedValue(new Error("disk full"));
    const api = makeApi({ validateEnvironment: validate, saveEnvironment: save });
    const onSaved = vi.fn();
    const onClose = vi.fn();
    renderEdit({ api, environment, onSaved, onClose });

    await user.click(screen.getByTestId("edit-save"));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    const banner = screen.getByTestId("edit-error");
    expect(banner).toHaveAttribute("role", "alert");
    expect(banner.textContent).toContain("保存失败");
    expect(onSaved).not.toHaveBeenCalled();
    // The form stays open for correction/retry.
    expect(screen.getByTestId("edit-save")).toBeEnabled();
  });
});
