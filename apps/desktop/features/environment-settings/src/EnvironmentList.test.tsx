import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { DshEnvironment, EnvironmentCatalog } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { EnvironmentList } from "./EnvironmentList";

function environment(id: string, label: string, ownership: DshEnvironment["ownership"]): DshEnvironment {
  return {
    schemaVersion: 1,
    id,
    label,
    harness: { mode: "executable", path: "dsh" },
    dshHome: "C:\\Users\\test\\.dsh",
    profile: "default",
    endpoint: { host: "127.0.0.1", port: "auto" },
    ownership,
  };
}

const catalog: EnvironmentCatalog = {
  schemaVersion: 1,
  revision: 3,
  activeEnvironmentId: "local-dsh",
  environments: [
    environment("local-dsh", "Local DSH", "managed"),
    environment("work-dsh", "Work DSH", "managed"),
    environment("dev-dsh", "Dev DSH", "attached"),
  ],
};

function makeApi(): DesktopApi {
  return {
    setActiveEnvironment: vi.fn().mockResolvedValue({
      ...catalog,
      activeEnvironmentId: "work-dsh",
    }),
    discoverHarnesses: vi.fn(),
    discoverProfiles: vi.fn(),
    probePort: vi.fn(),
    pickDirectory: vi.fn(),
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
  };
}

describe("EnvironmentList", () => {
  it("renders every environment with the active badge", () => {
    render(
      <EnvironmentList
        api={makeApi()}
        catalog={catalog}
        activeEnvironmentId="local-dsh"
        transitioning={false}
        onActivated={vi.fn()}
      />,
    );
    expect(screen.getByTestId("environment-local-dsh")).toBeInTheDocument();
    expect(screen.getByTestId("environment-work-dsh")).toBeInTheDocument();
    expect(screen.getByTestId("environment-dev-dsh")).toBeInTheDocument();
    expect(screen.getByText("active")).toBeInTheDocument();
    expect(screen.queryByTestId("activate-local-dsh")).not.toBeInTheDocument();
  });

  it("activates another environment and reports the new catalog", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    const onActivated = vi.fn();
    render(
      <EnvironmentList
        api={api}
        catalog={catalog}
        activeEnvironmentId="local-dsh"
        transitioning={false}
        onActivated={onActivated}
      />,
    );
    await user.click(screen.getByTestId("activate-work-dsh"));
    await waitFor(() => expect(api.setActiveEnvironment).toHaveBeenCalledWith({
      schemaVersion: 1,
      environmentId: "work-dsh",
    }));
    expect(onActivated).toHaveBeenCalledTimes(1);
    const [next, environment] = onActivated.mock.calls[0];
    expect(next.activeEnvironmentId).toBe("work-dsh");
    expect(environment.id).toBe("work-dsh");
  });

  it("shows the empty state without environments", () => {
    render(
      <EnvironmentList
        api={makeApi()}
        catalog={{ schemaVersion: 1, revision: 0, activeEnvironmentId: null, environments: [] }}
        activeEnvironmentId={null}
        transitioning={false}
        onActivated={vi.fn()}
      />,
    );
    expect(screen.getByText(/No environments saved yet/)).toBeInTheDocument();
  });
});
