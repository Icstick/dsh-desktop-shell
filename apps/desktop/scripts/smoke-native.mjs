#!/usr/bin/env node
/**
 * Windows real-DSH WebView2 native smoke / compatibility matrix driver.
 *
 * Prereqs (Windows only):
 *   - The desktop shell debug build must be running with
 *     WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=<port>".
 *   - A real user-owned DSH source checkout with a prebuilt CLI entry and a
 *     Node executable (ADR-0012 repository recipe).
 *
 * Usage:
 *   node scripts/smoke-native.mjs --cdp-port 9333 --out <evidence.json>
 *     [--node-path C:\Program Files\nodejs\node.exe]
 *     [--entry D:\deepseek-harness\apps\cli\lib\bin.js]
 *     [--dsh-home C:\Users\<user>\.dsh]
 *     [--env-id managed-real]
 *
 * The script drives the Shell via the Tauri IPC seam (window.__TAURI_INTERNALS__)
 * exposed by the shell WebView, and drives the dsh-surface child WebView over
 * Chrome DevTools Protocol. Every assertion result is written to the evidence
 * JSON file; a non-zero exit marks a smoke failure.
 */

import { writeFileSync } from "node:fs";

const argv = process.argv.slice(2);
const args = {};
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (!a.startsWith("--")) continue;
  const eq = a.indexOf("=");
  if (eq !== -1) {
    args[a.slice(2, eq)] = a.slice(eq + 1);
  } else {
    const next = argv[i + 1];
    args[a.slice(2)] = next !== undefined && !next.startsWith("--") ? next : true;
  }
}

const CDP_PORT = Number(args["cdp-port"] ?? 9333);
const OUT = args.out ?? "smoke-evidence.json";
const ENV_ID = args["env-id"] ?? "managed-real";
const NODE_PATH = args["node-path"] ?? "C:\\Program Files\\nodejs\\node.exe";
const ENTRY = args.entry ?? "D:\\deepseek-harness\\apps\\cli\\lib\\bin.js";
const DSH_HOME = args["dsh-home"] ?? (process.env.DSH_HOME ?? "");

const evidence = {
  tool: "scripts/smoke-native.mjs",
  startedAt: new Date().toISOString(),
  cdpPort: CDP_PORT,
  environment: { id: ENV_ID, nodePath: NODE_PATH, entry: ENTRY, dshHome: DSH_HOME },
  results: [],
  failures: [],
};

function record(name, ok, detail) {
  evidence.results.push({ name, ok: Boolean(ok), detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? "  | " + JSON.stringify(detail) : ""}`);
  if (!ok) evidence.failures.push(name);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function httpJson(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  return res.json();
}

class CdpSession {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(JSON.stringify(msg.error)));
        else resolve(msg.result);
      }
    });
  }

  static async connect(wsUrl) {
    const ws = new WebSocket(wsUrl);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", () => reject(new Error("ws connect failed")), { once: true });
    });
    return new CdpSession(ws);
  }

  send(method, params = {}) {
    const id = ++this.id;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const r = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
    });
    if (r.exceptionDetails) {
      throw new Error("page exception: " + JSON.stringify(r.exceptionDetails).slice(0, 500));
    }
    return r.result?.value;
  }

  close() {
    try {
      this.ws.close();
    } catch {}
  }
}

async function targets() {
  return httpJson(`http://127.0.0.1:${CDP_PORT}/json`);
}

async function waitForTarget(predicate, timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    const list = await targets();
    last = list.find(predicate) ?? null;
    if (last) return last;
    await sleep(500);
  }
  throw new Error("target not found in time; last list: " + JSON.stringify(last ?? "[]"));
}

async function poll(fn, predicate, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    last = await fn();
    if (predicate(last)) return last;
    await sleep(500);
  }
  throw new Error(`poll timeout: ${label}; last=${JSON.stringify(last)}`);
}

// ------------------------- helpers ---------------------------------------

const pageInvoke = (cmd, payload) => `
(async () => {
  try {
    const r = await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(payload)});
    return { ok: true, value: r };
  } catch (e) {
    return { ok: false, error: String(e && e.message ? e.message : e) };
  }
})()`;

const ENV = {
  schemaVersion: 1,
  id: ENV_ID,
  label: "Real DSH checkout smoke (advisory 2026-08-28)",
  harness: { mode: "repository", path: ENTRY },
  dshHome: DSH_HOME,
  profile: "default",
  nodePath: NODE_PATH,
  endpoint: { host: "127.0.0.1", port: "auto" },
  ownership: "managed",
};

function surfaceRequest(kind, extra = {}) {
  return {
    schemaVersion: 1,
    environmentId: ENV_ID,
    expectedGeneration: extra.expectedGeneration,
    ...extra,
  };
}

// ------------------------- run -------------------------------------------

const shell = await waitForTarget((t) => t.url.startsWith("http://tauri.localhost") || t.url.includes("tauri"), 15000);
const shellWs = await CdpSession.connect(shell.webSocketDebuggerUrl);
await shellWs.send("Runtime.enable");

let generation = 0;

try {
  // 0. Shell is up and IPC seam exists.
  const ipcProbe = await shellWs.evaluate(`(typeof window.__TAURI_INTERNALS__ !== "undefined") && (typeof window.__TAURI_INTERNALS__.invoke === "function")`);
  record("shell ipc seam available", ipcProbe === true, ipcProbe);

  // 1. Snapshot: unconfigured before saving an environment.
  const snap0 = await shellWs.evaluate(pageInvoke("get_shell_snapshot", {}));
  record(
    "initial shell snapshot unconfigured",
    snap0.ok && snap0.value.runtimeState === "unconfigured" && snap0.value.environmentId === null,
    snap0,
  );

  // 2. Validation of the ADR-0012 repository recipe.
  const validation = await shellWs.evaluate(pageInvoke("validate_environment", { environment: ENV }));
  const preview = validation.ok ? validation.value.launchPreview : null;
  const previewDisplays = preview?.arguments?.map((a) => a.display) ?? null;
  const expectedDisplays = ["[prebuilt-entry]", "web", "--host", "127.0.0.1", "--port", "0", "--no-open"];
  record(
    "repository recipe validates",
    validation.ok && validation.value.valid === true,
    { issues: validation.ok ? validation.value.issues : validation.error },
  );
  record(
    "launch argv is node <entry> web --host 127.0.0.1 --port ... --no-open",
    preview != null && preview.executable === NODE_PATH && JSON.stringify(previewDisplays) === JSON.stringify(expectedDisplays),
    { executable: preview?.executable, previewDisplays, expectedDisplays },
  );

  // 3. Save the environment (persisted catalog).
  const saved = await shellWs.evaluate(pageInvoke("save_environment", { environment: ENV }));
  record(
    "environment persisted",
    saved.ok && saved.value.activeEnvironmentId === ENV_ID,
    saved.ok ? { revision: saved.value.revision } : saved.error,
  );

  // 4. Start the managed runtime against the REAL user-owned DSH checkout.
  const start = await shellWs.evaluate(pageInvoke("start_managed_environment", { request: { schemaVersion: 1, environmentId: ENV_ID } }));
  record("managed start accepted", start.ok, start.ok ? undefined : start.error);

  const healthy = await poll(
    () => shellWs.evaluate(pageInvoke("get_managed_runtime_status", { request: { schemaVersion: 1, environmentId: ENV_ID } })),
    (r) => r.ok && r.value.state === "healthy",
    90000,
    "managed runtime healthy",
  );
  const report = healthy.value;
  generation = report.generation;
  const endpoint = report.endpoint;
  record(
    "runtime healthy with verified owned endpoint",
    endpoint != null && endpoint.verification === "owned_generation_output_and_tcp" && endpoint.host === "127.0.0.1",
    { state: report.state, generation, endpoint },
  );
  record(
    "runtime report does not leak token/query/bootstrap url",
    !JSON.stringify(report).includes("token=") && !JSON.stringify(report).includes("?token"),
    report,
  );

  // 5. Mount the native dsh-surface child WebView with the private bootstrap URL.
  const mount = await shellWs.evaluate(
    pageInvoke("mount_dsh_surface", { request: surfaceRequest("mount", { expectedGeneration: generation, bounds: { x: 0, y: 0, width: 900, height: 700 }, visible: true }) }),
  );
  record("surface mount accepted", mount.ok, mount.ok ? undefined : mount.error);

  const readyStatus = await poll(
    () => shellWs.evaluate(pageInvoke("get_dsh_surface_status", { request: surfaceRequest("status", { expectedGeneration: generation }) })),
    (r) => r.ok && r.value.state === "ready",
    30000,
    "surface ready",
  );
  const surfReady = readyStatus.value;
  record(
    "surface ready with exact verified origin",
    surfReady.state === "ready" && surfReady.platform === "windows" && surfReady.verifiedOrigin.port === endpoint.port && surfReady.visible === true,
    surfReady,
  );
  record(
    "surface status does not leak token/query/bootstrap url",
    !JSON.stringify(surfReady).includes("token=") && !JSON.stringify(surfReady).includes("?token"),
    surfReady,
  );

  // 6. The child WebView reached the DSH clean root (no query).
  const childTarget = await waitForTarget(
    (t) => t.url.startsWith(`http://127.0.0.1:${endpoint.port}/`),
    30000,
  );
  record(
    "child webview at clean exact-origin root (no query)",
    childTarget.url === `http://127.0.0.1:${endpoint.port}/`,
    childTarget.url,
  );
  const childWs = await CdpSession.connect(childTarget.webSocketDebuggerUrl);
  await childWs.send("Runtime.enable");
  await childWs.send("Page.enable");

  const childDoc = await childWs.evaluate(`({ href: location.href, title: document.title, text: (document.body ? document.body.innerText : "").slice(0, 300) })`);
  record("DSH page loaded in child webview", childDoc.text.trim().length > 0, childDoc);

  // 7. Layout matrix: resize + hide/show.
  const hidden = await shellWs.evaluate(
    pageInvoke("update_dsh_surface_layout", { request: surfaceRequest("layout", { expectedGeneration: generation, bounds: { x: 20, y: 20, width: 640, height: 480 }, visible: false }) }),
  );
  const hiddenStatus = await poll(
    () => shellWs.evaluate(pageInvoke("get_dsh_surface_status", { request: surfaceRequest("status", { expectedGeneration: generation }) })),
    (r) => r.ok && (r.value.state === "hidden" || (r.value.state === "ready" && r.value.visible === false)),
    15000,
    "surface hidden",
  );
  record(
    "layout hide transitions to hidden",
    hidden.ok && hiddenStatus.value.state === "hidden" && hiddenStatus.value.visible === false,
    { resize: hidden.ok ? hidden.value : hidden.error, status: hiddenStatus.value.state },
  );

  const shown = await shellWs.evaluate(
    pageInvoke("update_dsh_surface_layout", { request: surfaceRequest("layout", { expectedGeneration: generation, bounds: { x: 0, y: 0, width: 1200, height: 800 }, visible: true }) }),
  );
  const shownStatus = await poll(
    () => shellWs.evaluate(pageInvoke("get_dsh_surface_status", { request: surfaceRequest("status", { expectedGeneration: generation }) })),
    (r) => r.ok && r.value.state === "ready" && r.value.visible === true,
    15000,
    "surface shown",
  );
  const childBounds = await childWs.evaluate(`({ innerWidth, innerHeight })`);
  record(
    "layout show resizes back to ready",
    shown.ok && shownStatus.value.state === "ready" && shownStatus.value.visible === true && childBounds.innerWidth > 0,
    { bounds: shown.ok ? shown.value.bounds : shown.error, childBounds },
  );

  // 8. Reload lifecycle.
  const reload = await shellWs.evaluate(
    pageInvoke("reload_dsh_surface", { request: surfaceRequest("reload", { expectedGeneration: generation }) }),
  );
  const reloadedStatus = await poll(
    () => shellWs.evaluate(pageInvoke("get_dsh_surface_status", { request: surfaceRequest("status", { expectedGeneration: generation }) })),
    (r) => r.ok && r.value.state === "ready",
    20000,
    "surface reloaded",
  );
  const reloadedDoc = await childWs.evaluate(`({ href: location.href })`);
  record(
    "reload returns to ready on same exact origin",
    reload.ok && reloadedStatus.value.state === "ready" && reloadedDoc.href.startsWith(`http://127.0.0.1:${endpoint.port}/`),
    { status: reloadedStatus.value.state, href: reloadedDoc.href },
  );

  // 9. Negative matrix: cross-origin navigation must be blocked.
  await childWs.send("Page.navigate", { url: "https://example.com/" });
  await sleep(3000);
  const afterCross = await childWs.evaluate(`({ href: location.href, readyState: document.readyState })`);
  record(
    "cross-origin navigation denied (page stays on exact origin)",
    afterCross.href.startsWith(`http://127.0.0.1:${endpoint.port}/`),
    afterCross,
  );

  // 10. Negative matrix: popup/new-window denied.
  const popup = await childWs.evaluate(`(() => { const w = window.open("https://example.com/"); return w === null ? "null" : (w === undefined ? "undefined" : "window"); })()`);
  await sleep(1500);
  const targetsAfterPopup = await targets();
  record(
    "new window/popup denied and no extra target",
    popup !== "window" && targetsAfterPopup.length === 2,
    { popup, targetCount: targetsAfterPopup.length },
  );

  // 11. Negative matrix: download denied (no observable download, page intact).
  const dlBefore = await childWs.evaluate(`({ href: location.href })`);
  await childWs.evaluate(`(() => { const a = document.createElement("a"); a.href = "data:text/plain,smoke"; a.download = "smoke.txt"; document.body.appendChild(a); a.click(); a.remove(); return true; })()`);
  await sleep(1500);
  const dlAfter = await childWs.evaluate(`({ href: location.href })`);
  const targetsAfterDl = await targets();
  record(
    "download attempt denied (no navigation, no extra target)",
    dlAfter.href === dlBefore.href && targetsAfterDl.length === 2,
    { before: dlBefore, after: dlAfter, targetCount: targetsAfterDl.length },
  );

  // 12. Negative matrix: page permissions denied without prompt.
  const permission = await childWs.evaluate(`(async () => {
    try { return "notify:" + (await Notification.requestPermission()); } catch (e) { return "notify:error:" + e.name; }
  })()`);
  const geo = await childWs.evaluate(`(async () => {
    return await new Promise((resolve) => {
      try {
        navigator.geolocation.getCurrentPosition(
          () => resolve("geo:granted"),
          (e) => resolve("geo:denied:" + e.code + ":" + e.message),
          { timeout: 5000 }
        );
      } catch (e) { resolve("geo:threw:" + e.message); }
    });
  })()`);
  record(
    "page permissions denied without prompt",
    permission.startsWith("notify:denied") && geo.startsWith("geo:denied"),
    { permission, geo },
  );

  // 13. Child webview has no privileged IPC bridge.
  const ipcChild = await childWs.evaluate(`({ internals: typeof window.__TAURI_INTERNALS__, tauri: typeof window.__TAURI__ })`);
  const ipcAttempt = await childWs.evaluate(`(async () => {
    try { const r = await window.__TAURI_INTERNALS__.invoke("get_shell_snapshot", {}); return "resolved:" + JSON.stringify(r); }
    catch (e) { return "rejected:" + String(e && e.message ? e.message : e).slice(0, 120); }
  })()`);
  record(
    "child webview has no privileged native bridge (ACL rejects invoke)",
    typeof ipcChild.internals === "string" && ipcAttempt.startsWith("rejected:") && ipcAttempt.includes("not allowed"),
    { ipcChild, ipcAttempt },
  );

  // 14. Explicit unmount (binding still alive) must close the child webview.
  const unmount = await shellWs.evaluate(
    pageInvoke("unmount_dsh_surface", { request: surfaceRequest("unmount", { expectedGeneration: generation }) }),
  );
  await sleep(1500);
  const targetsAfterUnmount = await targets();
  record(
    "explicit unmount closes child webview",
    unmount.ok && unmount.value.state === "unmounted" && targetsAfterUnmount.length === 1,
    { status: unmount.ok ? unmount.value : unmount.error, targetCount: targetsAfterUnmount.length },
  );
  childWs.close();

  // 15. Stop runtime: bootstrap credential lifecycle ends with owned process tree.
  const stop = await shellWs.evaluate(
    pageInvoke("stop_managed_environment", { request: { schemaVersion: 1, environmentId: ENV_ID, expectedGeneration: generation } }),
  );
  record("managed stop accepted", stop.ok, stop.ok ? undefined : stop.error);

  const stopped = await poll(
    () => shellWs.evaluate(pageInvoke("get_managed_runtime_status", { request: { schemaVersion: 1, environmentId: ENV_ID } })),
    (r) => r.ok && r.value.state === "stopped",
    30000,
    "runtime stopped",
  );
  record(
    "runtime stopped after stop request (endpoint released, process unowned)",
    stopped.ok && stopped.value.state === "stopped" && stopped.value.endpoint === null && stopped.value.processOwnership === "none" && stopped.value.instanceId === null,
    stopped.ok ? stopped.value : stopped.error,
  );

  // Surface commands must now fail closed: no verified current-generation binding.
  const postStopSurface = await shellWs.evaluate(
    pageInvoke("get_dsh_surface_status", { request: surfaceRequest("status", { expectedGeneration: generation }) }),
  );
  record(
    "surface binding lost after runtime stop (status fails closed)",
    !postStopSurface.ok && postStopSurface.error.includes("not a verified current-generation Surface binding"),
    postStopSurface,
  );
  const postStopMount = await shellWs.evaluate(
    pageInvoke("mount_dsh_surface", { request: surfaceRequest("mount", { expectedGeneration: generation, bounds: { x: 0, y: 0, width: 900, height: 700 }, visible: true }) }),
  );
  record(
    "surface mount rejected after stop (no stale binding reuse)",
    !postStopMount.ok && postStopMount.error.includes("not a verified current-generation Surface binding"),
    postStopMount,
  );
} catch (e) {
  record("smoke driver exception", false, String(e && e.stack ? e.stack : e));
} finally {
  shellWs.close();
}

evidence.finishedAt = new Date().toISOString();
evidence.passed = evidence.failures.length === 0;
writeFileSync(OUT, JSON.stringify(evidence, null, 2));
console.log(`\n=== SMOKE ${evidence.passed ? "PASSED" : "FAILED"}: ${evidence.results.length - evidence.failures.length}/${evidence.results.length} checks passed ===`);
console.log("evidence written to", OUT);
process.exit(evidence.passed ? 0 : 1);
