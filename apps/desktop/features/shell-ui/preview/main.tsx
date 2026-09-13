import { createRoot } from "react-dom/client";
import { mockIPC } from "@tauri-apps/api/mocks";
import { App } from "../../../src/App";
import type { DshEnvironment, EnvironmentCatalog, NotificationReport, UsageSnapshot } from "../../../src/contracts";
import "../src/shell.css";

// Separate Vite entry, never imported by the production app. Every command is mocked.
const example: DshEnvironment = {
  schemaVersion: 1, id: "demo-workspace", label: "日常工作区 / Daily workspace",
  harness: { mode: "repository", path: "D:/Example/deepseek-harness", cwd: "D:/Example/projects" },
  dshHome: "D:/Example/dsh-home", profile: "web", ownership: "managed",
  endpoint: { host: "127.0.0.1", port: 3080 },
  policy: { autoRestartOnCrash: false, allowNativeAdapter: false },
};
const catalog: EnvironmentCatalog = {schemaVersion:1, revision:1, activeEnvironmentId:null, environments:[example]};
let notifications: NotificationReport[] = [];
const usage: UsageSnapshot = {schemaVersion:1, generatedAtUnixMs:1788696000000, records:[{schemaVersion:1,source:"dsh · 示例会话 / Sample session",period:{start:"2026-09-06T12:00:00Z",end:"2026-09-06T13:00:00Z"},inputTokens:128640,outputTokens:24680,cost:3.72,currency:"CNY",isEstimate:false,recordedAtUnixMs:1788696000000}], totals:{inputTokens:128640,outputTokens:24680,estimateCount:0,cost:3.72,currency:"CNY"}};

/* --- Workbench fixtures (WI-M10-WORKBENCH-FS / -GIT): a small fake repo so
   the visual preview can show both tabs without a desktop backend. --- */
const repoRoot = "D:/Example/deepseek-harness";
const workbenchRoots = {
  schemaVersion: 1,
  environmentId: example.id,
  roots: [
    { id: "repo", label: "Repository · deepseek-harness", kind: "repository", path: repoRoot, available: true, reason: null },
    { id: "dsh-home", label: "DSH home", kind: "dshHome", path: "D:/Example/dsh-home", available: true, reason: null },
    { id: "cwd", label: "Harness cwd", kind: "cwd", path: null, available: false, reason: "the directory does not exist" },
  ],
};
const dirs: Record<string, Array<{ name: string; kind: string; size: number; hidden: boolean }>> = {
  "": [
    { name: "crates", kind: "dir", size: 0, hidden: false },
    { name: "apps", kind: "dir", size: 0, hidden: false },
    { name: "AGENTS.md", kind: "file", size: 0, hidden: false },
    { name: "README.md", kind: "file", size: 0, hidden: false },
  ],
  "crates": [
    { name: "dsh-daemon", kind: "dir", size: 0, hidden: false },
    { name: "local-transport", kind: "dir", size: 0, hidden: false },
  ],
  "crates/local-transport": [
    { name: "src", kind: "dir", size: 0, hidden: false },
    { name: "Cargo.toml", kind: "file", size: 612, hidden: false },
  ],
  "crates/local-transport/src": [
    { name: "lib.rs", kind: "file", size: 0, hidden: false },
    { name: "peer.rs", kind: "file", size: 0, hidden: false },
    { name: "uds.rs", kind: "file", size: 0, hidden: false },
  ],
};
/**
 * File contents keyed by repo-relative path. The read/stat mocks answer with the
 * path that was actually requested - an earlier version returned one hardcoded
 * file for every read, which made the preview look broken when it was only the
 * fixture lying.
 */
const files: Record<string, string> = {
  "README.md": [
    "# DeepSeek Desktop Shell",
    "",
    "A desktop shell for the DeepSeek Harness: environments, terminal, browser,",
    "notifications, usage and the dev workbench.",
    "",
    "## Workbench",
    "",
    "- Files: browse and edit the environment-linked roots.",
    "- Git: read status and diffs, stage, commit, discard.",
    "",
  ].join("\n"),
  "AGENTS.md": [
    "# Agent Operating Contract",
    "",
    "Read START_HERE.md, tracking/project.yaml and tracking/CURRENT.md before you",
    "claim anything. One work item per session, claimed with a branch and an",
    "evidence plan; state lives in tracking/, interfaces in specs/, reasons in ADR.",
    "",
    "Applies to every agent, automation and human contributor in this repository.",
    "",
  ].join("\n"),
  "crates/local-transport/Cargo.toml": [
    "[package]",
    'name = "dsh-local-transport"',
    'version = "0.2.0"',
    'edition = "2024"',
    "",
    "[dependencies]",
    'serde = { version = "1", features = ["derive"] }',
    "",
  ].join("\n"),
  "crates/local-transport/src/lib.rs": [
    "//! Local transport carriers: TCP, Windows named pipes and Unix sockets.",
    "",
    "mod peer;",
    "mod uds;",
    "",
    "pub use peer::PeerInfo;",
    "pub use uds::UdsListener;",
    "",
  ].join("\n"),
  "crates/local-transport/src/peer.rs": [
    "//! Windows named-pipe peer identity (preview fixture).",
    "",
    "pub(crate) fn peer_process_id(handle: RawHandle) -> io::Result<u32> {",
    "    let info = query_pipe_peer(handle)?;",
    "    Ok(info.process_id)",
    "}",
    "",
    "fn query_pipe_peer(handle: RawHandle) -> io::Result<PeerInfo> {",
    "    // GetNamedPipeClientProcessId is the only reliable source here.",
    "    unsafe { PEER.with(|cell| cell.get(handle)) }",
    "}",
    "",
  ].join("\n"),
  "crates/local-transport/src/uds.rs": [
    "//! Unix domain socket carrier (preview fixture).",
    "",
    "pub struct UdsListener {",
    "    listener: UnixListener,",
    "    path: PathBuf,",
    "}",
    "",
    "impl UdsListener {",
    "    pub fn bind(path: &Path) -> io::Result<Self> {",
    "        let listener = UnixListener::bind(path)?;",
    "        Ok(Self { listener, path: path.to_path_buf() })",
    "    }",
    "}",
    "",
  ].join("\n"),
};
const sizeOf = (path: string) => files[path]?.length ?? 0;
const fileAt = (path: string) => files[path] ?? "// (empty preview fixture)\n";
const sampleDiff = [
  "diff --git a/crates/local-transport/src/peer.rs b/crates/local-transport/src/peer.rs",
  "index 3f9a1c2..8b41e77 100644",
  "--- a/crates/local-transport/src/peer.rs",
  "+++ b/crates/local-transport/src/peer.rs",
  "@@ -14,8 +14,11 @@ fn query_pipe_peer(handle: RawHandle) -> io::Result<PeerInfo> {",
  "     let mut pid = 0u32;",
  "-    let ok = unsafe { GetNamedPipeClientProcessId(handle, &mut pid) };",
  "+    // The pipe handle is only valid until the peer closes it.",
  "+    let ok = unsafe { GetNamedPipeClientProcessId(handle, &mut pid) };",
  "     if ok == 0 {",
  "-        return Err(io::Error::last_os_error());",
  "+        let error = io::Error::last_os_error();",
  "+        return Err(error);",
  "     }",
  "     Ok(PeerInfo { process_id: pid })",
  " }",
  "@@ -30,3 +33,4 @@ pub(crate) fn verify(peer: &PeerInfo) -> bool {",
  "     peer.process_id != std::process::id()",
  " }",
  "+// end of file",
  "",
].join("\n");
let workbenchEntries = [
    { path: "crates/local-transport/src/peer.rs", indexStatus: "M", worktreeStatus: " ", staged: true, unstaged: false, untracked: false },
    { path: "apps/desktop/features/git-panel-ui/src/GitPanel.tsx", indexStatus: "A", worktreeStatus: "M", staged: true, unstaged: true, untracked: false },
    { path: "apps/desktop/features/shell-ui/src/shell.css", indexStatus: " ", worktreeStatus: "M", staged: false, unstaged: true, untracked: false },
    { path: "apps/desktop/features/workbench-ui/src/WorkbenchPanel.tsx", indexStatus: "A", worktreeStatus: " ", staged: true, unstaged: false, untracked: false },
    { path: "docs/roadmap/PLAN-GIT-M1.md", indexStatus: "?", worktreeStatus: "?", staged: false, unstaged: false, untracked: true },
];
const workbenchStatus = () => ({
  schemaVersion: 1,
  root: repoRoot,
  branch: "feat/m10-workbench-git-m2",
  detached: false,
  clean: workbenchEntries.length === 0,
  truncated: false,
  entries: workbenchEntries,
});
/** GIT-M2: the preview mutations really move the fixture, so the screen answers. */
const workbenchMutation = (
  operation: string,
  request: { path?: string; all?: boolean },
) => {
  if (operation === "stage" || operation === "unstage") {
    const staging = operation === "stage";
    workbenchEntries = workbenchEntries.map((entry) =>
      !request.all && entry.path !== request.path
        ? entry
        : {
            ...entry,
            staged: staging,
            unstaged: !staging,
            untracked: false,
            indexStatus: staging ? (entry.indexStatus === "?" ? "A" : "M") : " ",
            worktreeStatus: staging ? " " : "M",
          },
    );
  } else if (operation === "commit") {
    workbenchEntries = workbenchEntries.filter((entry) => !entry.staged);
  } else if (operation === "discard") {
    workbenchEntries = workbenchEntries.filter((entry) => entry.path !== request.path);
  }
  return {
    schemaVersion: 1,
    root: repoRoot,
    operation,
    path: request.path ?? null,
    detail: null,
    status: workbenchStatus(),
  };
};
const workbenchLog = {
  schemaVersion: 1,
  root: repoRoot,
  truncated: false,
  entries: [
    { hash: "d0bf7e3a1c9e4f2b8d6a5c3e1f0b9a8d7c6e5f41", author: "Mikage", authoredAtUnixMs: 1789251600000, subject: "fix(workbench): clippy needless-borrow in the git panel" },
    { hash: "957765f0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6", author: "Mikage", authoredAtUnixMs: 1789248000000, subject: "feat(workbench): GIT-M1 backend - read-only status/diff/log/branches" },
    { hash: "fac107c9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3", author: "Mikage", authoredAtUnixMs: 1789161600000, subject: "docs(tracking): WI-M10-WORKBENCH-FS done; claim WORKBENCH-GIT" },
  ],
};
const workbenchBranches = {
  schemaVersion: 1,
  root: repoRoot,
  branches: ["main", "feat/m10-workbench-fs", "feat/m10-workbench-git-m1"],
  current: "feat/m10-workbench-git-m1",
};

mockIPC((command, payload) => {
  switch(command) {
    case "fs_list_roots": return workbenchRoots;
    case "fs_read_dir": {
      const request = (payload as { request?: { rootId?: string; relativePath?: string } } | undefined)?.request;
      const relative = request?.relativePath ?? "";
      const entries = (dirs[relative] ?? []).map((entry) => {
        if (entry.kind !== "file") return entry;
        const path = relative === "" ? entry.name : relative + "/" + entry.name;
        return { ...entry, size: sizeOf(path) };
      });
      return {
        schemaVersion: 1,
        rootId: request?.rootId ?? "repo",
        path: relative,
        truncated: false,
        entries,
      };
    }
    case "fs_read_file": {
      const request = (payload as { request?: { rootId?: string; relativePath?: string } } | undefined)?.request;
      const relative = request?.relativePath ?? "";
      return {
        schemaVersion: 1,
        rootId: request?.rootId ?? "repo",
        path: relative,
        size: sizeOf(relative),
        encoding: "utf-8",
        readOnly: false,
        reason: null,
        content: fileAt(relative),
      };
    }
    case "fs_stat": {
      const request = (payload as { request?: { rootId?: string; relativePath?: string } } | undefined)?.request;
      const relative = request?.relativePath ?? "";
      return {
        schemaVersion: 1,
        rootId: request?.rootId ?? "repo",
        path: relative,
        size: sizeOf(relative),
        modifiedUnixMs: 1789251600000,
        editable: true,
        reason: null,
      };
    }
    case "git_status": return workbenchStatus();
    case "git_stage":
    case "git_unstage":
    case "git_commit":
    case "git_discard": {
      const request = ((payload as { request?: Record<string, unknown> } | undefined)?.request ?? {});
      return workbenchMutation(command.slice(4), request);
    }
    case "git_diff": {
      const request = (payload as { request?: { path?: string; staged?: boolean } } | undefined)?.request;
      return { schemaVersion: 1, root: repoRoot, scope: request?.staged ? "staged" : "worktree", path: request?.path ?? null, text: sampleDiff, additions: 5, deletions: 2, truncated: false };
    }
    case "git_log": return workbenchLog;
    case "git_branches": return workbenchBranches;
    case "get_shell_snapshot": return {phase:"shell-mvp",runtimeState:"unconfigured",environmentId:null,generation:0};
    case "get_environment_catalog": return catalog;
    case "get_usage_snapshot": return usage;
    case "list_notifications": return notifications;
    case "dismiss_notification": notifications=[]; return;
    case "list_terminals": case "list_browsers": return [];
    case "discover_harnesses": return {schemaVersion:1,candidates:[]};
    case "discover_profiles": return {schemaVersion:1,dshHome:example.dshHome,profiles:[]};
    case "pick_directory": return null;
    case "probe_port": return {schemaVersion:1,port:3080,inUse:false};
    case "validate_environment": return {valid:true,issues:[],launchPreview:null};
    default: throw {message:"视觉预览：此操作需要真实桌面后端 / Requires the desktop backend."};
  }
}, {shouldMockEvents:true});

createRoot(document.getElementById("root")!).render(<><App /><div className="visual-preview-label">视觉预览 · 示例数据 · 不连接桌面后端 / Preview only</div></>);
