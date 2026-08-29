import { readFile, readdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = join(root, "src-tauri");
const expectedCommands = [
  "close_terminal",
  "create_terminal",
  "dismiss_notification",
  "discover_harnesses",
  "evaluate_dsh_surface_navigation",
  "get_dsh_surface_status",
  "get_dsh_surface_policy",
  "get_environment_catalog",
  "get_diagnostics",
  "get_managed_runtime_status",
  "get_shell_snapshot",
  "get_usage_snapshot",
  "mount_dsh_surface",
  "probe_attached_environment",
  "reload_dsh_surface",
  "save_environment",
  "list_terminals",
  "list_notifications",
  "notify_application",
  "resize_terminal",
  "start_managed_environment",
  "restart_managed_environment",
  "status_terminal",
  "stop_managed_environment",
  "write_terminal",
  "unmount_dsh_surface",
  "update_dsh_surface_layout",
  "validate_environment",
];

const [buildScript, rustEntry, tauriConfig, capabilityNames] = await Promise.all([
  readFile(join(tauriRoot, "build.rs"), "utf8"),
  readFile(join(tauriRoot, "src", "lib.rs"), "utf8"),
  readJson(join(tauriRoot, "tauri.conf.json")),
  readdir(join(tauriRoot, "capabilities")),
]);

const inventory = extractList(buildScript, /const COMMANDS:\s*&\[&str\]\s*=\s*&\[([^\]]*)\]/s);
const handlers = extractList(rustEntry, /generate_handler!\[([^\]]*)\]/s, /commands::([a-z0-9_]+)/g);
assertSame("AppManifest inventory", inventory, expectedCommands);
assertSame("invoke_handler inventory", handlers, expectedCommands);

const capabilities = await Promise.all(
  capabilityNames
    .filter((name) => name.endsWith(".json"))
    .map((name) => readJson(join(tauriRoot, "capabilities", name))),
);

if (capabilities.length !== 1) fail("exactly one Shell capability is required in the first M1 slice");
const [shellCapability] = capabilities;
assertSame("Shell capability webview labels", shellCapability.webviews ?? [], ["shell"]);
if ("windows" in shellCapability) fail("Shell capability must target only the trusted Shell webview");
if ("remote" in shellCapability) fail("Remote capability access is forbidden");

const expectedPermissions = [...expectedCommands.map((command) => `allow-${command.replaceAll("_", "-")}`), "core:event:default"];
assertSame("Shell custom command permissions", shellCapability.permissions ?? [], expectedPermissions);
assertSame("Configured capabilities", tauriConfig.app?.security?.capabilities ?? [], ["shell"]);

const configuredLabels = (tauriConfig.app?.windows ?? []).map((window) => window.label);
assertSame("Bundled window labels", configuredLabels, ["shell"]);
const capabilityTargets = capabilities.flatMap((capability) => [
  ...(capability.windows ?? []),
  ...(capability.webviews ?? []),
]);
if (configuredLabels.includes("dsh-surface") || capabilityTargets.includes("dsh-surface")) {
  fail("DSH Surface must not inherit the Shell capability");
}

const discoverySource = await readFile(join(tauriRoot, "src", "discovery.rs"), "utf8");
for (const forbidden of ["std::process::Command", "Command::new", "npm ", "pnpm "]) {
  if (discoverySource.includes(forbidden)) fail(`discovery must not execute candidates: found ${forbidden}`);
}

const attachedHealthSource = await readFile(join(tauriRoot, "src", "attached_health.rs"), "utf8");
for (const forbidden of ["std::process", "Command::new", ".kill(", "shutdown(", "start_environment"] ) {
  if (attachedHealthSource.includes(forbidden)) {
    fail(`Attached health must remain probe-only: found ${forbidden}`);
  }
}

const dshSurfacePolicySource = await readFile(join(tauriRoot, "src", "dsh_surface_policy.rs"), "utf8");
for (const forbidden of [
  "WebviewBuilder",
  "WebviewWindowBuilder",
  "initialization_script",
  "window.open",
  "shell::open",
  ".eval(",
]) {
  if (dshSurfacePolicySource.includes(forbidden)) {
    fail(`DSH Surface policy slice must not create, inject, or open a WebView target: found ${forbidden}`);
  }
}

const dshSurfaceSource = await readFile(join(tauriRoot, "src", "dsh_surface.rs"), "utf8");
for (const forbidden of [
  "initialization_script",
  ".eval(",
  "NewWindowResponse::Allow",
  "shell::open",
]) {
  if (dshSurfaceSource.includes(forbidden)) {
    fail(`Native DSH Surface must not inject, auto-open, or broaden loopback origin: found ${forbidden}`);
  }
}
for (const required of [
  'SURFACE_LABEL: &str = "dsh-surface"',
  'Url::parse("about:blank")',
  "NewWindowResponse::Deny",
  ".on_download(|_, _| false)",
  "COREWEBVIEW2_PERMISSION_STATE_DENY",
  "NavigationCompletedEventHandler",
  "SetIsPasswordAutosaveEnabled(false)",
  "SurfaceLifecycleState::UnsupportedPlatform",
]) {
  if (!dshSurfaceSource.includes(required)) {
    fail(`Native DSH Surface security gate is missing: ${required}`);
  }
}

for (const schemaName of [
  "dsh-surface-mount-request.schema.json",
  "dsh-surface-status-request.schema.json",
  "dsh-surface-layout-request.schema.json",
  "dsh-surface-reload-request.schema.json",
  "dsh-surface-unmount-request.schema.json",
]) {
  const schema = await readJson(join(root, "..", "..", "specs", "webview", schemaName));
  for (const forbidden of ["endpoint", "origin", "url", "label", "permission", "capability"] ) {
    if (forbidden in (schema.properties ?? {})) {
      fail(`${schemaName} must not accept caller-controlled ${forbidden}`);
    }
  }
}

const managedRuntimeSource = await readFile(join(tauriRoot, "src", "managed_runtime.rs"), "utf8");
for (const forbidden of [
  "cmd.exe",
  "powershell",
  "Command::new(\"sh\")",
  "Command::new(\"bash\")",
  ".arg(\"-c\")",
  "WebviewBuilder",
  "WebviewWindowBuilder",
]) {
  if (managedRuntimeSource.includes(forbidden)) {
    fail(`Managed runtime must use structured process APIs and create no WebView: found ${forbidden}`);
  }
}
for (const schemaName of [
  "managed-runtime-start-request.schema.json",
  "managed-runtime-status-request.schema.json",
  "managed-runtime-stop-request.schema.json",
]) {
  const schema = await readJson(join(root, "..", "..", "specs", "runtime", schemaName));
  for (const forbidden of ["executable", "args", "cwd", "host", "port", "endpoint", "instanceId"]) {
    if (forbidden in (schema.properties ?? {})) {
      fail(`${schemaName} must not accept caller-controlled ${forbidden}`);
    }
  }
}

console.log(`ACL validation passed: ${expectedCommands.length} commands, trusted Shell webview only, no remote access.`);

function extractList(source, blockPattern, itemPattern = /"([a-z0-9_]+)"/g) {
  const block = source.match(blockPattern)?.[1];
  if (!block) fail(`could not parse inventory using ${blockPattern}`);
  return [...block.matchAll(itemPattern)].map((match) => match[1]);
}

function assertSame(label, actual, expected) {
  const left = [...actual].sort();
  const right = [...expected].sort();
  if (JSON.stringify(left) !== JSON.stringify(right)) {
    fail(`${label} mismatch: ${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
  }
}

function fail(message) {
  throw new Error(message);
}

async function readJson(path) {
  return JSON.parse(await readFile(path, "utf8"));
}