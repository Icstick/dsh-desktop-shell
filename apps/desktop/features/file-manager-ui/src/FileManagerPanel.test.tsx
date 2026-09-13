import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { FsDirReport, FsFileReport, FsRootsReport, FsStatReport } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { I18nProvider, persistLang } from "../../../src/i18n";
import { FileManagerPanel } from "./FileManagerPanel";

function rootsReport(): FsRootsReport {
  return {
    schemaVersion: 1,
    environmentId: "local-dsh",
    roots: [
      { id: "repo", label: "Repository", kind: "repository", path: "/repo", available: true, reason: null },
      { id: "dsh-home", label: "DSH home", kind: "dshHome", path: null, available: false, reason: "missing" },
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
      { name: "notes.md", kind: "file", size: 4, hidden: false },
    ],
  };
}

function fileReport(overrides: Partial<FsFileReport> = {}): FsFileReport {
  return {
    schemaVersion: 1,
    rootId: "repo",
    path: "README.md",
    size: 12,
    encoding: "utf-8",
    readOnly: false,
    reason: null,
    content: "hello workbench",
    ...overrides,
  };
}

function statReport(overrides: Partial<FsStatReport> = {}): FsStatReport {
  return {
    schemaVersion: 1,
    rootId: "repo",
    path: "README.md",
    size: 12,
    modifiedUnixMs: 1789250000000,
    editable: true,
    reason: null,
    ...overrides,
  };
}

function conflict(): unknown {
  return { code: "CONFLICT", message: "the file changed on disk since it was opened", retryable: false };
}

function fakeApi(overrides: Partial<DesktopApi> = {}): DesktopApi {
  return {
    fsListRoots: vi.fn(async () => rootsReport()),
    fsReadDir: vi.fn(async () => dirReport()),
    fsReadFile: vi.fn(async () => fileReport()),
    fsStat: vi.fn(async () => statReport()),
    fsWriteFile: vi.fn(async () => statReport({ size: 18 })),
    ...overrides,
  } as unknown as DesktopApi;
}

function renderPanel(api: DesktopApi) {
  persistLang("en");
  return render(
    <I18nProvider>
      <FileManagerPanel api={api} />
    </I18nProvider>,
  );
}

async function openReadme(api: DesktopApi) {
  renderPanel(api);
  await userEvent.click(await screen.findByRole("button", { name: "Repository" }));
  await userEvent.click(await screen.findByRole("button", { name: /README.md/ }));
  return screen.findByTestId("fm-editor");
}

describe("FileManagerPanel (FS-M2)", () => {
  it("keeps each root's entries inside that root", async () => {
    const api = fakeApi({
      fsListRoots: vi.fn(
        async (): Promise<FsRootsReport> => ({
          schemaVersion: 1,
          environmentId: "local-dsh",
          roots: [
            { id: "repo", label: "Repository", kind: "repository", path: "/repo", available: true, reason: null },
            { id: "dsh-home", label: "DSH home", kind: "dshHome", path: "/home", available: true, reason: null },
          ],
        }),
      ),
    });
    renderPanel(api);

    await userEvent.click(await screen.findByRole("button", { name: "Repository" }));
    await userEvent.click(screen.getByRole("button", { name: "DSH home" }));

    // One list per root, each nested in its own root block. A flat list put both
    // roots' rows in a single indistinguishable pile at the bottom.
    const blocks = Array.from(document.querySelectorAll(".file-manager__root"));
    expect(blocks).toHaveLength(2);
    for (const block of blocks) {
      const list = block.querySelector(".file-manager__rows");
      expect(list).not.toBeNull();
      expect(list?.textContent).toContain("README.md");
    }
    expect(blocks[0].textContent).toContain("Repository");
    expect(blocks[1].textContent).toContain("DSH home");
  });

  it("filters the loaded rows and reports the count", async () => {
    const api = fakeApi();
    renderPanel(api);
    await userEvent.click(await screen.findByRole("button", { name: "Repository" }));
    expect(await screen.findByRole("button", { name: /notes.md/ })).toBeTruthy();

    await userEvent.type(screen.getByTestId("fm-filter"), "read");
    await waitFor(() => expect(screen.queryByRole("button", { name: /notes.md/ })).toBeNull());
    expect(screen.getByTestId("fm-filter-count").textContent).toContain("1");
  });

  it("marks the buffer dirty and saves with the recorded baseline", async () => {
    const api = fakeApi();
    const editor = await openReadme(api);
    expect(screen.getByTestId("fm-save")).toBeDisabled();

    await userEvent.type(editor, "!");
    await waitFor(() => expect(screen.getByTestId("fm-dirty").textContent).toBe("Unsaved"));
    expect(screen.getByTestId("fm-save")).toBeEnabled();

    await userEvent.click(screen.getByTestId("fm-save"));
    await waitFor(() => expect(api.fsWriteFile).toHaveBeenCalledTimes(1));
    const request = (api.fsWriteFile as unknown as { mock: { calls: unknown[][] } }).mock.calls[0][0] as Record<string, unknown>;
    expect(request).toMatchObject({
      schemaVersion: 1,
      rootId: "repo",
      relativePath: "README.md",
      content: "hello workbench!",
      expectedSize: 12,
      expectedModifiedUnixMs: 1789250000000,
      force: false,
    });
    await waitFor(() => expect(screen.getByTestId("fm-dirty").textContent).toBe("In sync"));
  });

  it("opens the conflict flow and only overwrites when forced", async () => {
    const api = fakeApi({
      fsWriteFile: vi
        .fn()
        .mockRejectedValueOnce(conflict())
        .mockResolvedValueOnce(statReport({ size: 18 })),
    });
    const editor = await openReadme(api);
    await userEvent.type(editor, "!");
    await userEvent.click(screen.getByTestId("fm-save"));

    const dialog = await screen.findByTestId("fm-conflict");
    expect(dialog.textContent).toContain("changed on disk");

    await userEvent.click(screen.getByRole("button", { name: "Overwrite" }));
    await waitFor(() => expect(api.fsWriteFile).toHaveBeenCalledTimes(2));
    const forced = (api.fsWriteFile as unknown as { mock: { calls: unknown[][] } }).mock.calls[1][0] as Record<string, unknown>;
    expect(forced.force).toBe(true);
  });

  it("discarding the conflict re-reads the file instead of overwriting", async () => {
    const api = fakeApi({ fsWriteFile: vi.fn().mockRejectedValueOnce(conflict()) });
    const editor = await openReadme(api);
    await userEvent.type(editor, "!");
    await userEvent.click(screen.getByTestId("fm-save"));
    await screen.findByTestId("fm-conflict");

    await userEvent.click(screen.getByRole("button", { name: "Discard mine" }));
    await waitFor(() => expect(api.fsReadFile).toHaveBeenCalledTimes(2));
    expect(api.fsWriteFile).toHaveBeenCalledTimes(1);
    expect((screen.getByTestId("fm-editor") as HTMLTextAreaElement).value).toBe("hello workbench");
  });

  it("keeps read-only files non-writable", async () => {
    const api = fakeApi({
      fsReadFile: vi.fn(async () => fileReport({ readOnly: true, encoding: "binary", content: "!!!", size: 3 })),
      fsStat: vi.fn(async () => statReport({ editable: false, reason: "not utf-8" })),
    });
    await openReadme(api);
    expect(screen.getByRole("status").textContent).toMatch(/read-only/i);
    expect(screen.getByTestId("fm-save")).toBeDisabled();
    expect((screen.getByTestId("fm-editor") as HTMLTextAreaElement).readOnly).toBe(true);
  });

  it("asks before discarding unsaved changes when opening another file", async () => {
    const api = fakeApi();
    const editor = await openReadme(api);
    await userEvent.type(editor, "!");
    await userEvent.click(screen.getByRole("button", { name: /notes.md/ }));

    const confirm = await screen.findByTestId("fm-confirm");
    expect(confirm.textContent).toContain("Unsaved changes");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByTestId("fm-confirm")).toBeNull());
  });
});
