import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type {
  GitBranchesReport,
  GitDiffReport,
  GitLogReport,
  GitMutationReport,
  GitStatusReport,
} from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { I18nProvider, persistLang } from "../../../src/i18n";
import { GitPanel } from "./GitPanel";

function statusReport(overrides: Partial<GitStatusReport> = {}): GitStatusReport {
  return {
    schemaVersion: 1,
    root: "D:\\repo",
    branch: "main",
    detached: false,
    clean: false,
    truncated: false,
    entries: [
      { path: "src/main.rs", indexStatus: "M", worktreeStatus: " ", staged: true, unstaged: false, untracked: false },
      { path: "README.md", indexStatus: " ", worktreeStatus: "M", staged: false, unstaged: true, untracked: false },
      { path: "notes.txt", indexStatus: "?", worktreeStatus: "?", staged: false, unstaged: false, untracked: true },
    ],
    ...overrides,
  };
}

function diffReport(overrides: Partial<GitDiffReport> = {}): GitDiffReport {
  return {
    schemaVersion: 1,
    root: "D:\\repo",
    scope: "worktree",
    path: "README.md",
    text: "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old line\n+new line\n",
    additions: 1,
    deletions: 1,
    truncated: false,
    ...overrides,
  };
}

function logReport(): GitLogReport {
  return {
    schemaVersion: 1,
    root: "D:\\repo",
    truncated: false,
    entries: [
      { hash: "0123456789abcdef0123456789abcdef01234567", author: "DSH Test", authoredAtUnixMs: 1789250000000, subject: "first commit" },
    ],
  };
}

function branchesReport(): GitBranchesReport {
  return { schemaVersion: 1, root: "D:\\repo", branches: ["main", "feature"], current: "main" };
}

/** What every mutation answers with: the status it produced. */
function mutationReport(
  operation: "stage" | "unstage" | "commit" | "discard",
  overrides: Partial<GitMutationReport> = {},
): GitMutationReport {
  return {
    schemaVersion: 1,
    root: "D:\\repo",
    operation,
    path: null,
    detail: null,
    status: statusReport(),
    ...overrides,
  };
}

function unavailable(): unknown {
  return { code: "UNAVAILABLE", message: "git is not installed", retryable: false };
}

function fakeApi(overrides: Partial<DesktopApi> = {}): DesktopApi {
  return {
    gitStatus: vi.fn(async () => statusReport()),
    gitDiff: vi.fn(async () => diffReport()),
    gitLog: vi.fn(async () => logReport()),
    gitBranches: vi.fn(async () => branchesReport()),
    gitStage: vi.fn(async () => mutationReport("stage")),
    gitUnstage: vi.fn(async () => mutationReport("unstage")),
    gitCommit: vi.fn(async () => mutationReport("commit")),
    gitDiscard: vi.fn(async () => mutationReport("discard")),
    ...overrides,
  } as unknown as DesktopApi;
}

function renderPanel(api: DesktopApi) {
  persistLang("en");
  return render(
    <I18nProvider>
      <GitPanel api={api} />
    </I18nProvider>,
  );
}

function calls(mock: unknown): Array<Record<string, unknown>> {
  return (mock as { mock: { calls: unknown[][] } }).mock.calls.map(
    (call) => call[0] as Record<string, unknown>,
  );
}

describe("GitPanel (GIT-M1)", () => {
  it("groups the status into facets with counts and reads the worktree diff", async () => {
    const api = fakeApi();
    renderPanel(api);

    await waitFor(() => expect(screen.getByTestId("git-count-staged").textContent).toBe("1"));
    expect(screen.getByTestId("git-count-unstaged").textContent).toBe("1");
    expect(screen.getByTestId("git-count-untracked").textContent).toBe("1");
    expect(screen.getByTestId("git-branch").textContent).toBe("main");

    await userEvent.click(screen.getByRole("button", { name: "README.md" }));

    await waitFor(() => expect(api.gitDiff).toHaveBeenCalledTimes(1));
    expect(calls(api.gitDiff)[0]).toEqual({ schemaVersion: 1, path: "README.md", staged: false });
    expect(screen.getByTestId("git-scope").textContent).toBe("Worktree");
    expect(screen.getByTestId("git-additions").textContent).toBe("+1");
    expect(screen.getByTestId("git-deletions").textContent).toBe("\u22121");
    const diff = screen.getByTestId("git-diff");
    expect(diff.textContent).toContain("+new line");
    expect(diff.querySelector('[data-kind="added"]')?.textContent).toBe("+new line\n");
    expect(diff.querySelector('[data-kind="removed"]')?.textContent).toBe("-old line\n");
  });

  it("asks the index for a staged entry", async () => {
    const api = fakeApi();
    renderPanel(api);

    await userEvent.click(await screen.findByRole("button", { name: "src/main.rs" }));

    await waitFor(() => expect(api.gitDiff).toHaveBeenCalledTimes(1));
    expect(calls(api.gitDiff)[0]).toEqual({ schemaVersion: 1, path: "src/main.rs", staged: true });
  });

  it("says the tree is clean instead of showing an empty list", async () => {
    const api = fakeApi({
      gitStatus: vi.fn(async () => statusReport({ entries: [], clean: true })),
    });
    renderPanel(api);

    expect((await screen.findByTestId("git-clean")).textContent).toContain("clean");
    expect(screen.queryByTestId("git-diff")).toBeNull();
  });

  it("shows the backend reason when the repository is unavailable", async () => {
    const api = fakeApi({
      gitStatus: vi.fn().mockRejectedValue(unavailable()),
      gitLog: vi.fn().mockRejectedValue(unavailable()),
      gitBranches: vi.fn().mockRejectedValue(unavailable()),
    });
    renderPanel(api);

    const alert = await screen.findByTestId("git-error");
    expect(alert.textContent).toContain("Git is unavailable");
    expect(alert.textContent).toContain("git is not installed");
  });

  it("says an untracked file has no diff yet", async () => {
    const api = fakeApi({
      gitDiff: vi.fn(async () => diffReport({ path: "notes.txt", text: "", additions: 0, deletions: 0 })),
    });
    renderPanel(api);

    await userEvent.click(await screen.findByRole("button", { name: "notes.txt" }));

    expect((await screen.findByTestId("git-diff-empty")).textContent).toContain("no diff to show yet");
  });

  it("filters the change list and reports the count", async () => {
    const api = fakeApi();
    renderPanel(api);

    expect(await screen.findByRole("button", { name: "notes.txt" })).toBeTruthy();
    await userEvent.type(screen.getByTestId("git-filter"), "read");

    await waitFor(() => expect(screen.queryByRole("button", { name: "notes.txt" })).toBeNull());
    expect(screen.getByTestId("git-filter-count").textContent).toContain("1");
  });

  it("keeps the history and the branch list behind a disclosure (placeholder)", async () => {
    const api = fakeApi();
    renderPanel(api);

    expect(screen.queryByTestId("git-log")).toBeNull();
    await userEvent.click(await screen.findByRole("button", { name: /Recent commits/ }));
    expect(screen.getByTestId("git-log").textContent).toContain("first commit");

    await userEvent.click(screen.getByRole("button", { name: /Branches/ }));
    expect(screen.getByTestId("git-branches").textContent).toContain("feature");
  });
});
describe("GitPanel (GIT-M2)", () => {
  it("stages a path and folds the status it gets back in", async () => {
    const api = fakeApi({
      gitStage: vi.fn(async () =>
        mutationReport("stage", { path: "README.md", status: statusReport({ entries: [], clean: true }) }),
      ),
    });
    renderPanel(api);

    await userEvent.click(await screen.findByRole("button", { name: "Stage README.md" }));

    await waitFor(() => expect(api.gitStage).toHaveBeenCalledTimes(1));
    expect(calls(api.gitStage)[0]).toEqual({ schemaVersion: 1, path: "README.md" });
    // The list renders the status the mutation answered with.
    expect((await screen.findByTestId("git-clean")).textContent).toContain("clean");
  });

  it("offers whole-tree stage and unstage", async () => {
    const api = fakeApi();
    renderPanel(api);

    await userEvent.click(await screen.findByTestId("git-stage-all"));
    await waitFor(() => expect(api.gitStage).toHaveBeenCalledTimes(1));
    expect(calls(api.gitStage)[0]).toEqual({ schemaVersion: 1, all: true });

    await userEvent.click(screen.getByTestId("git-unstage-all"));
    await waitFor(() => expect(api.gitUnstage).toHaveBeenCalledTimes(1));
    expect(calls(api.gitUnstage)[0]).toEqual({ schemaVersion: 1, all: true });
  });

  it("unstage all is idle while nothing is staged", async () => {
    const api = fakeApi({
      gitStatus: vi.fn(async () =>
        statusReport({
          entries: [
            { path: "README.md", indexStatus: " ", worktreeStatus: "M", staged: false, unstaged: true, untracked: false },
          ],
        }),
      ),
    });
    renderPanel(api);

    expect(await screen.findByTestId("git-unstage-all")).toBeDisabled();
    expect(screen.getByTestId("git-stage-all")).toBeEnabled();
  });

  it("gates the commit box on staged work and a message", async () => {
    const api = fakeApi();
    renderPanel(api);

    const commitButton = await screen.findByTestId("git-commit");
    expect(commitButton).toBeDisabled();
    expect(screen.getByTestId("git-commit-staged").textContent).toContain("1");

    await userEvent.type(screen.getByTestId("git-commit-message"), "fix: something");
    await waitFor(() => expect(commitButton).toBeEnabled());

    await userEvent.click(commitButton);
    await waitFor(() => expect(api.gitCommit).toHaveBeenCalledTimes(1));
    expect(calls(api.gitCommit)[0]).toEqual({ schemaVersion: 1, message: "fix: something" });
    await waitFor(() =>
      expect((screen.getByTestId("git-commit-message") as HTMLTextAreaElement).value).toBe(""),
    );
  });

  it("keeps commit disabled when the index is empty", async () => {
    const api = fakeApi({
      gitStatus: vi.fn(async () => statusReport({ entries: [], clean: true })),
    });
    renderPanel(api);

    await userEvent.type(await screen.findByTestId("git-commit-message"), "nothing to say");
    expect(screen.getByTestId("git-commit")).toBeDisabled();
    expect(screen.getByTestId("git-commit-staged").textContent).toContain("0");
  });

  it("only discards after the confirmation word is typed", async () => {
    const api = fakeApi();
    renderPanel(api);

    await userEvent.click(await screen.findByRole("button", { name: "Discard README.md" }));

    const dialog = await screen.findByTestId("git-discard-dialog");
    expect(dialog.textContent).toContain("cannot be undone");
    const confirm = screen.getByTestId("git-discard-confirm");
    expect(confirm).toBeDisabled();

    await userEvent.type(screen.getByTestId("git-discard-word"), "DISCAR");
    expect(confirm).toBeDisabled();
    await userEvent.type(screen.getByTestId("git-discard-word"), "D");
    await waitFor(() => expect(confirm).toBeEnabled());

    await userEvent.click(confirm);
    await waitFor(() => expect(api.gitDiscard).toHaveBeenCalledTimes(1));
    expect(calls(api.gitDiscard)[0]).toEqual({ schemaVersion: 1, path: "README.md" });
    await waitFor(() => expect(screen.queryByTestId("git-discard-dialog")).toBeNull());
  });

  it("renders the backend message when a mutation fails", async () => {
    const api = fakeApi({
      gitStage: vi.fn().mockRejectedValue({
        code: "UNAVAILABLE",
        message: "there is nothing staged to commit",
        retryable: false,
      }),
    });
    renderPanel(api);

    await userEvent.click(await screen.findByRole("button", { name: "Stage README.md" }));

    const alert = await screen.findByTestId("git-mutation-error");
    expect(alert.textContent).toContain("nothing staged");
  });
});

