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

function createApi(seedSeq = 0): DesktopApi {
  let seq = seedSeq;
  return {
    createBrowser: vi.fn().mockImplementation(async () =>
      report({ sessionId: `brw-test-${++seq}`, state: "created", currentUrl: null }),
    ),
    navigateBrowser: vi
      .fn()
      .mockImplementation(async (request: { sessionId: string; url: string }) =>
        report({ sessionId: request.sessionId, state: "ready", currentUrl: request.url }),
      ),
    closeBrowser: vi.fn().mockImplementation(async (request: { sessionId: string }) =>
      report({ sessionId: request.sessionId, state: "closed", currentUrl: null }),
    ),
    listBrowsers: vi.fn().mockResolvedValue([]),
    snapshotBrowser: vi.fn().mockResolvedValue({ ...report(), text: "Example Domain" }),
  } as unknown as DesktopApi;
}

function emit(payload: BrowserEvent) {
  act(() => {
    eventHandlerRef.current?.({ payload });
  });
}

function event(overrides: Partial<BrowserEvent>): BrowserEvent {
  return {
    schemaVersion: 1,
    sessionId: "brw-test-1",
    kind: "navigation_changed",
    occurredAtUnixMs: 1787792400200,
    url: "https://example.com/",
    title: null,
    ...overrides,
  };
}

// The recovery effect is async, so always await the active session marker:
// the definition-grid row shows the full session id.
async function sessionGridValue() {
  return screen.findByText("brw-test-1", { selector: ".browser-panel__session-id" });
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
    expect(await sessionGridValue()).toBeInTheDocument();
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

  it("fills a never-navigated tab in place", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ state: "created", currentUrl: null }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    const input = await screen.findByRole("textbox", { name: "Browser URL" });
    expect(await sessionGridValue()).toBeInTheDocument();
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

  it("opening a URL never displaces a navigated tab: it creates a new one", async () => {
    const api = createApi(1);
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ sessionId: "brw-test-1", state: "ready", currentUrl: "https://one.example/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    const input = await screen.findByRole("textbox", { name: "Browser URL" });
    expect(await sessionGridValue()).toBeInTheDocument();
    await user.clear(input);
    await user.type(input, "https://two.example/");
    await user.click(screen.getByRole("button", { name: "Open" }));

    // A new session is created and navigated; the original tab is intact.
    expect(api.createBrowser).toHaveBeenCalledTimes(1);
    expect(api.navigateBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      sessionId: "brw-test-2",
      url: "https://two.example/",
    });
    expect(screen.getByRole("tab", { name: "one.example" })).toBeInTheDocument();
    expect(
      await screen.findByText("brw-test-2", { selector: ".browser-panel__session-id" }),
    ).toBeInTheDocument();
  });

  it("reloads the committed URL", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://example.com/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    expect(await sessionGridValue()).toBeInTheDocument();
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

    expect(await sessionGridValue()).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reload" })).toBeDisabled();
  });

  it("closes the backend session and clears the panel", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://example.com/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    expect(await sessionGridValue()).toBeInTheDocument();
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

    expect(await sessionGridValue()).toBeInTheDocument();
    emit(event({ kind: "navigation_changed", url: "https://news.example/" }));

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

    expect(await sessionGridValue()).toBeInTheDocument();
    emit(event({ sessionId: "brw-other-1", url: "https://other.example/" }));

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

    expect(await sessionGridValue()).toBeInTheDocument();
    emit(event({
      kind: "load_failed",
      url: "https://broken.example/",
    }));

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

    expect(await sessionGridValue()).toBeInTheDocument();
    emit(event({ kind: "closed", url: null }));

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
    emit(event({ url: "https://redirected.example/" }));

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

    expect(api.createBrowser).toHaveBeenCalledTimes(1);
  });

  it("restores multiple live sessions as tabs", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ sessionId: "brw-test-1", currentUrl: "https://one.example/" }),
      report({ sessionId: "brw-test-2", currentUrl: "https://two.example/" }),
    ]);
    renderPanel(api);

    expect(await screen.findByRole("tab", { name: "one.example" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "two.example" })).toBeInTheDocument();
    // First session is active and its URL is in the bar.
    expect(screen.getByRole("textbox", { name: "Browser URL" })).toHaveValue(
      "https://one.example/",
    );
    expect(
      screen.getByText("brw-test-1", { selector: ".browser-panel__session-id" }),
    ).toBeInTheDocument();
  });

  it("new-tab creates and activates a second session", async () => {
    // brw-test-1 is already restored from listBrowsers; the new session
    // must get the next id.
    const api = createApi(1);
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ currentUrl: "https://one.example/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    expect(await sessionGridValue()).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "New tab" }));

    expect(api.createBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      mode: "human_surface",
    });
    expect(
      await screen.findByText("brw-test-2", { selector: ".browser-panel__session-id" }),
    ).toBeInTheDocument();
    // The new tab is active; the old session is still listed.
    expect(screen.getByRole("tab", { name: "one.example" })).toBeInTheDocument();
  });

  it("does not displace the active navigated tab when opening another URL", async () => {
    const api = createApi(2);
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ sessionId: "brw-test-1", currentUrl: "https://one.example/" }),
      report({ sessionId: "brw-test-2", currentUrl: "https://two.example/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    // Activate the second tab.
    await user.click(await screen.findByRole("tab", { name: "two.example" }));
    expect(
      await screen.findByText("brw-test-2", { selector: ".browser-panel__session-id" }),
    ).toBeInTheDocument();

    await user.clear(screen.getByRole("textbox", { name: "Browser URL" }));
    await user.type(
      screen.getByRole("textbox", { name: "Browser URL" }),
      "https://three.example/",
    );
    await user.click(screen.getByRole("button", { name: "Open" }));

    // A fresh tab is created and navigated; the active tab was NOT bumped.
    expect(api.createBrowser).toHaveBeenCalledTimes(1);
    expect(api.navigateBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      sessionId: "brw-test-3",
      url: "https://three.example/",
    });
    expect(screen.getByRole("tab", { name: "two.example" })).toBeInTheDocument();
    expect(
      await screen.findByText("brw-test-3", { selector: ".browser-panel__session-id" }),
    ).toBeInTheDocument();
  });

  it("updates a tab label from title_changed events", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ sessionId: "brw-test-1", currentUrl: "https://one.example/" }),
      report({ sessionId: "brw-test-2", currentUrl: "https://two.example/" }),
    ]);
    renderPanel(api);

    await screen.findByRole("tab", { name: "one.example" });
    emit(event({
      sessionId: "brw-test-2",
      kind: "title_changed",
      url: "https://two.example/",
      title: "Two Docs",
    }));

    expect(screen.getByRole("tab", { name: "Two Docs" })).toBeInTheDocument();
    // The other tab still falls back to its host.
    expect(screen.getByRole("tab", { name: "one.example" })).toBeInTheDocument();
  });

  it("closing the active tab switches to a neighbour", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ sessionId: "brw-test-1", currentUrl: "https://one.example/" }),
      report({ sessionId: "brw-test-2", currentUrl: "https://two.example/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    await user.click(await screen.findByRole("tab", { name: "two.example" }));
    expect(
      await screen.findByText("brw-test-2", { selector: ".browser-panel__session-id" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close" }));

    expect(api.closeBrowser).toHaveBeenCalledWith({
      schemaVersion: 1,
      sessionId: "brw-test-2",
    });
    // Neighbouring tab takes over and its URL fills the bar.
    expect(
      await screen.findByText("brw-test-1", { selector: ".browser-panel__session-id" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Browser URL" })).toHaveValue(
      "https://one.example/",
    );
  });

  it("switching tabs refills the URL bar", async () => {
    const api = createApi();
    vi.mocked(api.listBrowsers).mockResolvedValue([
      report({ sessionId: "brw-test-1", currentUrl: "https://one.example/" }),
      report({ sessionId: "brw-test-2", currentUrl: "https://two.example/" }),
    ]);
    const user = userEvent.setup();
    renderPanel(api);

    await user.click(await screen.findByRole("tab", { name: "two.example" }));

    expect(screen.getByRole("textbox", { name: "Browser URL" })).toHaveValue(
      "https://two.example/",
    );
  });
});
