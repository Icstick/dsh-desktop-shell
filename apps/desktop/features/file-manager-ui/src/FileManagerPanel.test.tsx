import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { FsDirReport, FsFileReport, FsRootsReport } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { I18nProvider, persistLang } from "../../../src/i18n";
import { FileManagerPanel } from "./FileManagerPanel";

function rootsReport(): FsRootsReport {
  return {
    schemaVersion: 1,
    environmentId: "local-dsh",
    roots: [
      {
        id: "repo",
        label: "Repository",
        kind: "repository",
        path: "/repo",
        available: true,
        reason: null,
      },
      {
        id: "dsh-home",
        label: "DSH home",
        kind: "dshHome",
        path: null,
        available: false,
        reason: "this environment does not point at a directory",
      },
    ],
  };
}

function dirReport(): FsDirReport {
  return {
    schemaVersion: 1,
    rootId: "repo",
    path: "",
    truncated: false,
    entries: [
      { name: "src", kind: "dir", size: 0, hidden: false },
      { name: "README.md", kind: "file", size: 12, hidden: false },
    ],
  };
}

function fileReport(overrides: Partial<FsFileReport> = {}): FsFileReport {
  return {
    schemaVersion: 1,
    rootId: "repo",
    path: "README.md",
    size: 14,
    encoding: "utf-8",
    readOnly: false,
    reason: null,
    content: "hello workbench",
    ...overrides,
  };
}

function fakeApi(overrides: Partial<DesktopApi> = {}): DesktopApi {
  return {
    fsListRoots: vi.fn(async () => rootsReport()),
    fsReadDir: vi.fn(async () => dirReport()),
    fsReadFile: vi.fn(async () => fileReport()),
    ...overrides,
  } as unknown as DesktopApi;
}

function renderPanel(api: DesktopApi) {
  // Assertions are written in English; the provider default is zh.
  persistLang("en");
  return render(
    <I18nProvider>
      <FileManagerPanel api={api} />
    </I18nProvider>,
  );
}

describe("FileManagerPanel", () => {
  it("lists roots, expands a directory and reads a file", async () => {
    const api = fakeApi();
    renderPanel(api);

    const root = await screen.findByRole("button", { name: "Repository" });
    expect(screen.getByText(/Unavailable/)).toBeTruthy();

    await userEvent.click(root);
    expect(await screen.findByRole("button", { name: /src/ })).toBeTruthy();

    await userEvent.click(screen.getByRole("button", { name: /README.md/ }));
    expect(await screen.findByText("hello workbench")).toBeTruthy();
    expect(api.fsReadDir).toHaveBeenCalledWith({
      schemaVersion: 1,
      rootId: "repo",
      relativePath: "",
      showHidden: false,
    });
  });

  it("shows the read-only banner the report asks for", async () => {
    const api = fakeApi({
      fsReadFile: vi.fn(async () => fileReport({ readOnly: true, encoding: "binary" })),
    });
    renderPanel(api);

    await userEvent.click(await screen.findByRole("button", { name: "Repository" }));
    await userEvent.click(await screen.findByRole("button", { name: /README.md/ }));
    const banner = await screen.findByRole("status");
    expect(banner.textContent).toMatch(/read-only/i);
  });

  it("surfaces a containment failure as an alert", async () => {
    const api = fakeApi({
      fsReadFile: vi.fn(async () => {
        throw new Error("the requested path escapes the root");
      }),
    });
    renderPanel(api);

    await userEvent.click(await screen.findByRole("button", { name: "Repository" }));
    await userEvent.click(await screen.findByRole("button", { name: /README.md/ }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/escapes the root/);
  });
});
