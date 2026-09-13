import { invoke } from "@tauri-apps/api/core";

import type {
  AttachedHealthReport,
  BrowserCloseRequest,
  BrowserCreateRequest,
  BrowserEvent,
  BrowserNavigateRequest,
  BrowserReport,
  BrowserSnapshotReport,
  BrowserSnapshotRequest,
  TerminalCreateRequest,
  TerminalOutputEvent,
  TerminalReport,
  TerminalResizeRequest,
  TerminalSessionRequest,
  TerminalWriteRequest,
  NotificationDismissRequest,
  NotificationReport,
  NotificationRequest,
  AttachedHealthRequest,
  DiagnosticsReport,
  DiagnosticsRequest,
  DshEnvironment,
  DshSurfaceNavigationDecision,
  DshSurfaceNavigationRequest,
  DshSurfaceLayoutRequest,
  DshSurfaceMountRequest,
  DshSurfacePolicy,
  DshSurfacePolicyRequest,
  DshSurfaceReloadRequest,
  DshSurfaceStatus,
  DshSurfaceStatusRequest,
  DshSurfaceUnmountRequest,
  DiscoverProfilesReport,
  DiscoverProfilesRequest,
  EnvironmentCatalog,
  EnvironmentValidation,
  FsDirReport,
  FsFileReport,
  FsReadDirRequest,
  FsReadFileRequest,
  FsRootsReport,
  FsRootsRequest,
  FsStatReport,
  FsStatRequest,
  FsWriteFileRequest,
  GitBranchesReport,
  GitBranchesRequest,
  GitDiffReport,
  GitDiffRequest,
  GitCommitRequest,
  GitDiscardRequest,
  GitLogReport,
  GitLogRequest,
  GitMutationReport,
  GitStageRequest,
  GitStatusReport,
  GitStatusRequest,
  GitUnstageRequest,
  HarnessDiscoveryReport,
  HarnessDiscoveryRequest,
  ProbePortReport,
  ProbePortRequest,
  SetActiveEnvironmentRequest,
  ManagedRuntimeReport,
  ManagedRuntimeRestartRequest,
  ManagedRuntimeStartRequest,
  ManagedRuntimeStatusRequest,
  ManagedRuntimeStopRequest,
  ShellSnapshot,
  UsageSnapshot,
  UsageSnapshotRequest,
} from "./contracts";

export interface DesktopApi {
  closeBrowser(request: BrowserCloseRequest): Promise<BrowserReport>;
  createBrowser(request: BrowserCreateRequest): Promise<BrowserReport>;
  dismissNotification(request: NotificationDismissRequest): Promise<void>;
  listBrowsers(): Promise<BrowserReport[]>;
  listNotifications(): Promise<NotificationReport[]>;
  navigateBrowser(request: BrowserNavigateRequest): Promise<BrowserReport>;
  notifyApplication(request: NotificationRequest): Promise<NotificationReport>;
  snapshotBrowser(request: BrowserSnapshotRequest): Promise<BrowserSnapshotReport>;
  closeTerminal(request: TerminalSessionRequest): Promise<void>;
  createTerminal(request: TerminalCreateRequest): Promise<TerminalReport>;
  listTerminals(): Promise<TerminalReport[]>;
  resizeTerminal(request: TerminalResizeRequest): Promise<TerminalReport>;
  statusTerminal(request: TerminalSessionRequest): Promise<TerminalReport>;
  writeTerminal(request: TerminalWriteRequest): Promise<void>;
  discoverHarnesses(request: HarnessDiscoveryRequest): Promise<HarnessDiscoveryReport>;
  discoverProfiles(request: DiscoverProfilesRequest): Promise<DiscoverProfilesReport>;
  probePort(request: ProbePortRequest): Promise<ProbePortReport>;
  pickDirectory(): Promise<string | null>;
  removeEnvironment(environmentId: string): Promise<EnvironmentCatalog>;
  setActiveEnvironment(request: SetActiveEnvironmentRequest): Promise<EnvironmentCatalog>;
  evaluateDshSurfaceNavigation(
    request: DshSurfaceNavigationRequest,
  ): Promise<DshSurfaceNavigationDecision>;
  getDshSurfacePolicy(request: DshSurfacePolicyRequest): Promise<DshSurfacePolicy>;
  getDshSurfaceStatus(request: DshSurfaceStatusRequest): Promise<DshSurfaceStatus>;
  getDiagnostics(request: DiagnosticsRequest): Promise<DiagnosticsReport>;
  getEnvironmentCatalog(): Promise<EnvironmentCatalog>;
  getManagedRuntimeStatus(request: ManagedRuntimeStatusRequest): Promise<ManagedRuntimeReport>;
  getShellSnapshot(): Promise<ShellSnapshot>;
  getUsageSnapshot(request: UsageSnapshotRequest): Promise<UsageSnapshot>;
  mountDshSurface(request: DshSurfaceMountRequest): Promise<DshSurfaceStatus>;
  probeAttachedEnvironment(request: AttachedHealthRequest): Promise<AttachedHealthReport>;
  reloadDshSurface(request: DshSurfaceReloadRequest): Promise<DshSurfaceStatus>;
  restartManagedEnvironment(request: ManagedRuntimeRestartRequest): Promise<ManagedRuntimeReport>;
  saveEnvironment(environment: DshEnvironment): Promise<EnvironmentCatalog>;
  startManagedEnvironment(request: ManagedRuntimeStartRequest): Promise<ManagedRuntimeReport>;
  stopManagedEnvironment(request: ManagedRuntimeStopRequest): Promise<ManagedRuntimeReport>;
  unmountDshSurface(request: DshSurfaceUnmountRequest): Promise<DshSurfaceStatus>;
  updateDshSurfaceLayout(request: DshSurfaceLayoutRequest): Promise<DshSurfaceStatus>;
  validateEnvironment(environment: DshEnvironment): Promise<EnvironmentValidation>;
  fsListRoots(request: FsRootsRequest): Promise<FsRootsReport>;
  fsReadDir(request: FsReadDirRequest): Promise<FsDirReport>;
  fsReadFile(request: FsReadFileRequest): Promise<FsFileReport>;
  fsStat(request: FsStatRequest): Promise<FsStatReport>;
  fsWriteFile(request: FsWriteFileRequest): Promise<FsStatReport>;
  gitStatus(request: GitStatusRequest): Promise<GitStatusReport>;
  gitDiff(request: GitDiffRequest): Promise<GitDiffReport>;
  gitLog(request: GitLogRequest): Promise<GitLogReport>;
  gitBranches(request: GitBranchesRequest): Promise<GitBranchesReport>;
  gitStage(request: GitStageRequest): Promise<GitMutationReport>;
  gitUnstage(request: GitUnstageRequest): Promise<GitMutationReport>;
  gitCommit(request: GitCommitRequest): Promise<GitMutationReport>;
  gitDiscard(request: GitDiscardRequest): Promise<GitMutationReport>;
}

export const desktopApi: DesktopApi = {
  closeBrowser: (request) => invoke<BrowserReport>("close_browser", { request }),
  createBrowser: (request) => invoke<BrowserReport>("create_browser", { request }),
  dismissNotification: (request) =>
    invoke<void>("dismiss_notification", { request }),
  listBrowsers: () => invoke<BrowserReport[]>("list_browsers"),
  listNotifications: () => invoke<NotificationReport[]>("list_notifications"),
  navigateBrowser: (request) =>
    invoke<BrowserReport>("navigate_browser", { request }),
  notifyApplication: (request) =>
    invoke<NotificationReport>("notify_application", { request }),
  snapshotBrowser: (request) =>
    invoke<BrowserSnapshotReport>("snapshot_browser", { request }),
  closeTerminal: (request) => invoke<void>("close_terminal", { request }),
  createTerminal: (request) => invoke<TerminalReport>("create_terminal", { request }),
  listTerminals: () => invoke<TerminalReport[]>("list_terminals"),
  resizeTerminal: (request) => invoke<TerminalReport>("resize_terminal", { request }),
  statusTerminal: (request) => invoke<TerminalReport>("status_terminal", { request }),
  writeTerminal: (request) => invoke<void>("write_terminal", { request }),
  discoverHarnesses: (request) =>
    invoke<HarnessDiscoveryReport>("discover_harnesses", { request }),
  discoverProfiles: (request) =>
    invoke<DiscoverProfilesReport>("discover_profiles", { request }),
  probePort: (request) => invoke<ProbePortReport>("probe_port", { request }),
  pickDirectory: () => invoke<string | null>("pick_directory"),
  removeEnvironment: (environmentId) =>
    invoke<EnvironmentCatalog>("remove_environment", { environmentId }),
  setActiveEnvironment: (request) =>
    invoke<EnvironmentCatalog>("set_active_environment", { request }),
  evaluateDshSurfaceNavigation: (request) =>
    invoke<DshSurfaceNavigationDecision>("evaluate_dsh_surface_navigation", { request }),
  getDshSurfacePolicy: (request) =>
    invoke<DshSurfacePolicy>("get_dsh_surface_policy", { request }),
  getDshSurfaceStatus: (request) =>
    invoke<DshSurfaceStatus>("get_dsh_surface_status", { request }),
  getDiagnostics: (request) =>
    invoke<DiagnosticsReport>("get_diagnostics", { request }),
  getEnvironmentCatalog: () => invoke<EnvironmentCatalog>("get_environment_catalog"),
  getManagedRuntimeStatus: (request) =>
    invoke<ManagedRuntimeReport>("get_managed_runtime_status", { request }),
  getShellSnapshot: () => invoke<ShellSnapshot>("get_shell_snapshot"),
  getUsageSnapshot: (request) =>
    invoke<UsageSnapshot>("get_usage_snapshot", { request }),
  fsListRoots: (request) => invoke<FsRootsReport>("fs_list_roots", { request }),
  fsReadDir: (request) => invoke<FsDirReport>("fs_read_dir", { request }),
  fsReadFile: (request) => invoke<FsFileReport>("fs_read_file", { request }),
  fsStat: (request) => invoke<FsStatReport>("fs_stat", { request }),
  fsWriteFile: (request) => invoke<FsStatReport>("fs_write_file", { request }),
  gitStatus: (request) => invoke<GitStatusReport>("git_status", { request }),
  gitDiff: (request) => invoke<GitDiffReport>("git_diff", { request }),
  gitLog: (request) => invoke<GitLogReport>("git_log", { request }),
  gitBranches: (request) => invoke<GitBranchesReport>("git_branches", { request }),
  gitStage: (request) => invoke<GitMutationReport>("git_stage", { request }),
  gitUnstage: (request) => invoke<GitMutationReport>("git_unstage", { request }),
  gitCommit: (request) => invoke<GitMutationReport>("git_commit", { request }),
  gitDiscard: (request) => invoke<GitMutationReport>("git_discard", { request }),
  mountDshSurface: (request) =>
    invoke<DshSurfaceStatus>("mount_dsh_surface", { request }),
  probeAttachedEnvironment: (request) =>
    invoke<AttachedHealthReport>("probe_attached_environment", { request }),
  reloadDshSurface: (request) =>
    invoke<DshSurfaceStatus>("reload_dsh_surface", { request }),
  saveEnvironment: (environment) =>
    invoke<EnvironmentCatalog>("save_environment", { environment }),
  restartManagedEnvironment: (request) =>
    invoke<ManagedRuntimeReport>("restart_managed_environment", { request }),
  startManagedEnvironment: (request) =>
    invoke<ManagedRuntimeReport>("start_managed_environment", { request }),
  stopManagedEnvironment: (request) =>
    invoke<ManagedRuntimeReport>("stop_managed_environment", { request }),
  unmountDshSurface: (request) =>
    invoke<DshSurfaceStatus>("unmount_dsh_surface", { request }),
  updateDshSurfaceLayout: (request) =>
    invoke<DshSurfaceStatus>("update_dsh_surface_layout", { request }),
  validateEnvironment: (environment) =>
    invoke<EnvironmentValidation>("validate_environment", { environment }),
};