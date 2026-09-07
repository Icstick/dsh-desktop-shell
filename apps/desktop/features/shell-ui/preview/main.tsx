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
mockIPC((command) => {
  switch(command) {
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
