import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
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
    removeEnvironment: vi.fn().mockResolvedValue(catalog),
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

function renderList(overrides: Partial<ComponentProps<typeof EnvironmentList>> = {}) {
  const defaults = {
    api: makeApi(),
    catalog,
    activeEnvironmentId: "local-dsh",
    transitioning: false,
    runningEnvironmentId: null,
    onActivated: vi.fn(),
    onAddEnvironment: vi.fn(),
    onEdit: vi.fn(),
    onRemove: vi.fn().mockResolvedValue(undefined),
  };
  return render(<EnvironmentList {...defaults} {...overrides} />);
}

describe("EnvironmentList", () => {
  it("renders every environment with the active badge and card actions", () => {
    renderList();
    expect(screen.getByTestId("environment-local-dsh")).toBeInTheDocument();
    expect(screen.getByTestId("environment-work-dsh")).toBeInTheDocument();
    expect(screen.getByTestId("environment-dev-dsh")).toBeInTheDocument();
    expect(screen.getByText("active")).toBeInTheDocument();
    expect(screen.queryByTestId("activate-local-dsh")).not.toBeInTheDocument();
    expect(screen.getByTestId("activate-work-dsh")).toBeInTheDocument();
    expect(screen.getByTestId("remove-local-dsh")).toBeInTheDocument();
    expect(screen.getByTestId("add-environment")).toBeInTheDocument();
  });

  it("activates another environment and reports the new catalog", async () => {
    const user = userEvent.setup();
    const api = makeApi();
    const onActivated = vi.fn();
    renderList({ api, onActivated });
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

  it("opens the edit form for the clicked card", async () => {
    const user = userEvent.setup();
    const onEdit = vi.fn();
    renderList({ onEdit });
    await user.click(screen.getByTestId("edit-work-dsh"));
    expect(onEdit).toHaveBeenCalledTimes(1);
    expect(onEdit.mock.calls[0][0].id).toBe("work-dsh");
  });

  it("surfaces the backend message when removal fails", async () => {
    const user = userEvent.setup();
    const onRemove = vi
      .fn()
      .mockRejectedValue(new Error("Environment is not in the catalog."));
    renderList({ onRemove });
    await user.click(screen.getByTestId("remove-work-dsh"));
    await user.click(await screen.findByTestId("remove-confirm-work-dsh"));
    expect(
      await screen.findByText(/Environment is not in the catalog/),
    ).toBeInTheDocument();
  });

  it("shows the empty state with an add-environment entry point", async () => {
    const user = userEvent.setup();
    const onAddEnvironment = vi.fn();
    renderList({
      catalog: { schemaVersion: 1, revision: 0, activeEnvironmentId: null, environments: [] },
      activeEnvironmentId: null,
      onAddEnvironment,
    });
    expect(screen.getByText(/还没有配置环境/)).toBeInTheDocument();
    await user.click(screen.getByTestId("add-environment"));
    expect(onAddEnvironment).toHaveBeenCalledTimes(1);
  });

  it("removes a non-active environment after an inline confirmation", async () => {
    const user = userEvent.setup();
    const onRemove = vi.fn().mockResolvedValue(undefined);
    renderList({ onRemove });

    await user.click(screen.getByTestId("remove-work-dsh"));
    await screen.findByTestId("remove-confirm-work-dsh");
    // Neither the active nor the running notice applies to envB here.
    expect(screen.queryByTestId("remove-note-active-work-dsh")).not.toBeInTheDocument();
    expect(screen.queryByTestId("remove-note-running-work-dsh")).not.toBeInTheDocument();
    await user.click(screen.getByTestId("remove-confirm-work-dsh"));
    await waitFor(() => expect(onRemove).toHaveBeenCalledTimes(1));
    expect(onRemove.mock.calls[0][0].id).toBe("work-dsh");
  });

  it("cancelling the confirmation leaves the card untouched", async () => {
    const user = userEvent.setup();
    const onRemove = vi.fn().mockResolvedValue(undefined);
    renderList({ onRemove });
    await user.click(screen.getByTestId("remove-dev-dsh"));
    await screen.findByTestId("remove-confirm-dev-dsh");
    await user.click(screen.getByTestId("remove-cancel-dev-dsh"));
    expect(screen.queryByTestId("remove-confirm-dev-dsh")).not.toBeInTheDocument();
    expect(onRemove).not.toHaveBeenCalled();
  });

  it("warns about the active and running state before removing them", async () => {
    const user = userEvent.setup();
    const onRemove = vi.fn().mockResolvedValue(undefined);
    renderList({ runningEnvironmentId: "local-dsh", onRemove });

    await user.click(screen.getByTestId("remove-local-dsh"));
    await screen.findByTestId("remove-note-active-local-dsh");
    expect(screen.getByTestId("remove-note-running-local-dsh")).toBeInTheDocument();
    await user.click(screen.getByTestId("remove-confirm-local-dsh"));
    await waitFor(() => expect(onRemove).toHaveBeenCalledTimes(1));
    expect(onRemove.mock.calls[0][0].id).toBe("local-dsh");
  });
});
