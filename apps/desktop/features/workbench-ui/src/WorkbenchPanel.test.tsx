import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { DesktopApi } from "../../../src/desktop-api";
import { I18nProvider, persistLang } from "../../../src/i18n";
import { WorkbenchPanel } from "./WorkbenchPanel";

/** Enough of the API for both tabs to mount: the fs side and the git side. */
function fakeApi(): DesktopApi {
  return {
    fsListRoots: vi.fn(async () => ({
      schemaVersion: 1,
      environmentId: "local-dsh",
      roots: [{ id: "repo", label: "Repository", kind: "repository", path: "/repo", available: true, reason: null }],
    })),
    gitStatus: vi.fn(async () => ({
      schemaVersion: 1,
      root: "D:\\repo",
      branch: "main",
      detached: false,
      clean: true,
      truncated: false,
      entries: [],
    })),
    gitDiff: vi.fn(),
    gitLog: vi.fn(async () => ({ schemaVersion: 1, root: "D:\\repo", truncated: false, entries: [] })),
    gitBranches: vi.fn(async () => ({ schemaVersion: 1, root: "D:\\repo", branches: [], current: null })),
  } as unknown as DesktopApi;
}

function renderWorkbench(api: DesktopApi) {
  persistLang("en");
  return render(
    <I18nProvider>
      <WorkbenchPanel api={api} />
    </I18nProvider>,
  );
}

describe("WorkbenchPanel tabs (GIT-M1)", () => {
  it("starts on Files and swaps the body when Git is selected", async () => {
    const api = fakeApi();
    renderWorkbench(api);

    expect(screen.getByTestId("wb-tab-files").getAttribute("aria-selected")).toBe("true");
    expect(await screen.findByTestId("fm-toolbar")).toBeTruthy();
    expect(screen.queryByTestId("git-toolbar")).toBeNull();
    expect(api.gitStatus).not.toHaveBeenCalled();

    await userEvent.click(screen.getByTestId("wb-tab-git"));

    expect(await screen.findByTestId("git-toolbar")).toBeTruthy();
    expect(screen.queryByTestId("fm-toolbar")).toBeNull();
    await waitFor(() => expect(api.gitStatus).toHaveBeenCalledTimes(1));
  });

  it("moves between tabs with the arrow keys", async () => {
    renderWorkbench(fakeApi());
    await screen.findByTestId("fm-toolbar");

    screen.getByTestId("wb-tab-files").focus();
    await userEvent.keyboard("{ArrowRight}");

    expect(await screen.findByTestId("git-toolbar")).toBeTruthy();
    expect(screen.getByTestId("wb-tab-git").getAttribute("aria-selected")).toBe("true");

    await userEvent.keyboard("{ArrowLeft}");
    expect(await screen.findByTestId("fm-toolbar")).toBeTruthy();
    expect(screen.getByTestId("wb-tab-files").getAttribute("aria-selected")).toBe("true");
  });
});
