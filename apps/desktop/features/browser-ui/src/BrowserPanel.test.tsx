import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BrowserEvent, BrowserReport } from "../../../src/contracts";
import type { DesktopApi } from "../../../src/desktop-api";
import { I18nProvider, persistLang } from "../../../src/i18n";
import { BrowserPanel } from "./BrowserPanel";

const { listenMock, eventHandlerRef } = vi.hoisted(() => {
  const listenMock = vi.fn();
  const eventHandlerRef: {
    current: ((event: { payload: BrowserEvent }) => void) | null;
  } = { current: null };
  return { listenMock, eventHandlerRef };
});

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock.mockImplementation(
    async (_channel: string, handler: (event: { payload: BrowserEvent }) => void) => {
      eventHandlerRef.current = handler;
      return () => {
        eventHandlerRef.current = null;
      };
    },
  ),
}));

// BrowserPanel renders through useI18n (default zh); assertions are
// written in English, so the default render helper pins English.
function renderPanel(api: DesktopApi) {
  persistLang("en");
  return render(
    <I18nProvider>
      <BrowserPanel api={api} />
    </I18nProvider>,
  );
}

function report(overrides: Partial<BrowserReport> = {}): BrowserReport {
  return {
    schemaVersion: 1,
    sessionId: "brw-test-1",
    state: "ready",
    mode: "human_surface",
    currentUrl: "https://example.com/",
    createdAtUnixMs: 1787792400000,
    lastActivityUnixMs: 1787792400100,
    error: null,
    ...overrides,
  };
}

function createApi(): DesktopApi {
  return {
    createBrowser: vi.fn().mockResolvedValue(report({ state: "created", currentUrl: null })),
    navigateBrowser: vi
      .fn()
      .mockImplementation(async (request: { sessionId: string; url: string }) =>
        report({ state: "ready", currentUrl: request.url }),
      ),
    closeBrowser: vi.fn().mockResolvedValue(report({ state: "closed", currentUrl: null })),
    listBrowsers: vi.fn().mockResolvedValue([]),
    snapshotBrowser: vi.fn().mockResolvedValue({ ...report(), text: "Example Domain" }),
  } as unknown as DesktopApi;
}

function emit(payload: BrowserEvent) {
  act(() => {
    eventHandlerRef.current?.({ payload });
  });
}

// The session id appears both in the panel chrome and the state grid.
// The recovery effect is async, so always await the session label.
function sessionLabel() {
  return screen.findByText("brw-test-1", { selector: ".browser-panel__session" });
}

describe("BrowserPanel", () => {
  beforeEach(() => {
    eventHandlerRef.current = null;
    listenMock.mockImplementation(
      async (_channel: string, handler: (event: { payload: BrowserEvent }) => void) => {
        eventHandlerRef.current = handler;
        return () => {
          eventHandlerRef.current = null;
        };
      },
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("creates a session and navigates when Open is submitted", async () => {
    const api = createApi();
    const user = userEvent.setup();
    renderPanel(api);

    await user.type(screen.getByRole("textbox", { name: "Browser URL" }), "example.com");
    await user.click(screen.getByRole("button", { name: "Open" }));

    expect(api.createBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      mode: "human_surface",
    });
    expect(api.navigateBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      url: "https://example.com",
    });
    expect(screen.getByRole("textbox", { name: "Browser URL" })).toHaveValue(
      "https://example.com",
    );
    expect(
      screen.getByText("https://example.com", { selector: ".browser-panel__url-value" }),
    ).toBeInTheDocument();
    expect(await sessionLabel()).toBeInTheDocument();
  });

  it("submits on Enter for keyboard-only operation", async () => {
    const api = createApi();
    const user = userEvent.setup();
    renderPanel(api);

    await user.type(
      screen.getByRole("textbox", { name: "Browser URL" }),
      "https://example.com/{enter}",
    );

    expect(api.createBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      mode: "human_surface",
    });
    expect(api.navigateBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      url: "https://example.com/",
    });
  });

  it("recovers a live session and navigates it without creating another", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ state: "ready", currentUrl: "https://example.com/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    const input = await screen.findByRole("textbox", { name: "Browser URL" });
    expect(await sessionLabel()).toBeInTheDocument();
    await user.clear(input);
    await user.type(input, "https://dsh.local/");
    await user.click(screen.getByRole("button", { name: "Open" }));

    expect(api.createBrowser).not.toHaveBeenCalled();
    expect(api.navigateBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      url: "https://dsh.local/",
    });
  });

  it("reloads the committed URL", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://example.com/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    expect(await sessionLabel()).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Reload" }));

    expect(api.navigateBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      url: "https://example.com/",
    });
  });

  it("keeps Reload disabled until a URL is committed", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ state: "created", currentUrl: null }),
    ]);
    renderPanel(api);

    expect(await sessionLabel()).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reload" })).toBeDisabled();
  });

  it("closes the backend session and clears the panel", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://example.com/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    expect(await sessionLabel()).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close" }));

    expect(api.closeBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      sessionId: "brw-test-1",
    });
    expect(screen.getByText("no browser session")).toBeInTheDocument();
  });

  it("updates the URL bar and state from navigation_changed events", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ state: "loading", currentUrl: "https://old.example/" }),
    ]);
    renderPanel(api);

    expect(await sessionLabel()).toBeInTheDocument();
    emit({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      kind: "navigation_changed",
      occurredAtUnixMs: 1787792400200,
      url: "https://news.example/",
    });

    expect(screen.getByRole("textbox", { name: "Browser URL" })).toHaveValue(
      "https://news.example/",
    );
    expect(
      screen.getByText("https://news.example/", { selector: ".browser-panel__url-value" }),
    ).toBeInTheDocument();
    expect(screen.getByText("ready", { selector: ".browser-panel__state" })).toBeInTheDocument();
  });

  it("ignores events for other sessions", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://example.com/" }),
    ]);
    renderPanel(api);

    expect(await sessionLabel()).toBeInTheDocument();
    emit({
      schemaVersion: 1,
      sessionId: "brw-other-1",
      kind: "navigation_changed",
      occurredAtUnixMs: 1787792400200,
      url: "https://other.example/",
    });

    expect(screen.getByRole("textbox", { name: "Browser URL" })).toHaveValue(
      "https://example.com/",
    );
  });

  it("surfaces load_failed events as an error state", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://broken.example/" }),
    ]);
    renderPanel(api);

    expect(await sessionLabel()).toBeInTheDocument();
    emit({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      kind: "load_failed",
      occurredAtUnixMs: 1787792400300,
      url: "https://broken.example/",
    });

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Page failed to load: https://broken.example/",
    );
    expect(screen.getByText("error", { selector: ".browser-panel__state" })).toBeInTheDocument();
  });

  it("clears the panel when the backend emits closed", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://example.com/" }),
    ]);
    renderPanel(api);

    expect(await sessionLabel()).toBeInTheDocument();
    emit({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      kind: "closed",
      occurredAtUnixMs: 1787792400400,
      url: null,
    });

    expect(screen.getByText("no browser session")).toBeInTheDocument();
  });

  it("does not overwrite a focused URL input on navigation events", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://example.com/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    const input = await screen.findByRole("textbox", { name: "Browser URL" });
    await user.click(input);
    emit({
      schemaVersion: 1,
      sessionId: "brw-test-1",
      kind: "navigation_changed",
      occurredAtUnixMs: 1787792400200,
      url: "https://redirected.example/",
    });

    expect(input).toHaveValue("https://example.com/");
  });

  it("surfaces command failures in an alert callout", async () => {
    const api = createApi();
    vi.mocked(api.navigateBrowser).mockRejectedValue({
      code: "UNAVAILABLE",
      message: "Navigation is unavailable.",
      retryable: false,
      correlationId: "desktop-browser-test",
    });
    const user = userEvent.setup();
    renderPanel(api);

    await user.type(screen.getByRole("textbox", { name: "Browser URL" }), "example.com");
    await user.click(screen.getByRole("button", { name: "Open" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Navigation is unavailable.");
  });

  it("stays usable when the Tauri event bridge is absent", async () => {
    listenMock.mockRejectedValueOnce(new Error("no tauri bridge"));
    const api = createApi();
    const user = userEvent.setup();
    renderPanel(api);

    await user.type(screen.getByRole("textbox", { name: "Browser URL" }), "example.com");
    await user.click(screen.getByRole("button", { name: "Open" }));

    expect(api.createBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      mode: "human_surface",
    });
    expect(await sessionLabel()).toBeInTheDocument();
  });
});
