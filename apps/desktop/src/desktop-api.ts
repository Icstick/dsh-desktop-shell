import { invoke } from "@tauri-apps/api/core";

import type {
  AttachedHealthReport,
  AttachedHealthRequest,
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
  EnvironmentCatalog,
  EnvironmentValidation,
  HarnessDiscoveryReport,
  HarnessDiscoveryRequest,
  ManagedRuntimeReport,
  ManagedRuntimeStartRequest,
  ManagedRuntimeStatusRequest,
  ManagedRuntimeStopRequest,
  ShellSnapshot,
} from "./contracts";

export interface DesktopApi {
  discoverHarnesses(request: HarnessDiscoveryRequest): Promise<HarnessDiscoveryReport>;
  evaluateDshSurfaceNavigation(
    request: DshSurfaceNavigationRequest,
  ): Promise<DshSurfaceNavigationDecision>;
  getDshSurfacePolicy(request: DshSurfacePolicyRequest): Promise<DshSurfacePolicy>;
  getDshSurfaceStatus(request: DshSurfaceStatusRequest): Promise<DshSurfaceStatus>;
  getEnvironmentCatalog(): Promise<EnvironmentCatalog>;
  getManagedRuntimeStatus(request: ManagedRuntimeStatusRequest): Promise<ManagedRuntimeReport>;
  getShellSnapshot(): Promise<ShellSnapshot>;
  mountDshSurface(request: DshSurfaceMountRequest): Promise<DshSurfaceStatus>;
  probeAttachedEnvironment(request: AttachedHealthRequest): Promise<AttachedHealthReport>;
  reloadDshSurface(request: DshSurfaceReloadRequest): Promise<DshSurfaceStatus>;
  saveEnvironment(environment: DshEnvironment): Promise<EnvironmentCatalog>;
  startManagedEnvironment(request: ManagedRuntimeStartRequest): Promise<ManagedRuntimeReport>;
  stopManagedEnvironment(request: ManagedRuntimeStopRequest): Promise<ManagedRuntimeReport>;
  unmountDshSurface(request: DshSurfaceUnmountRequest): Promise<DshSurfaceStatus>;
  updateDshSurfaceLayout(request: DshSurfaceLayoutRequest): Promise<DshSurfaceStatus>;
  validateEnvironment(environment: DshEnvironment): Promise<EnvironmentValidation>;
}

export const desktopApi: DesktopApi = {
  discoverHarnesses: (request) =>
    invoke<HarnessDiscoveryReport>("discover_harnesses", { request }),
  evaluateDshSurfaceNavigation: (request) =>
    invoke<DshSurfaceNavigationDecision>("evaluate_dsh_surface_navigation", { request }),
  getDshSurfacePolicy: (request) =>
    invoke<DshSurfacePolicy>("get_dsh_surface_policy", { request }),
  getDshSurfaceStatus: (request) =>
    invoke<DshSurfaceStatus>("get_dsh_surface_status", { request }),
  getEnvironmentCatalog: () => invoke<EnvironmentCatalog>("get_environment_catalog"),
  getManagedRuntimeStatus: (request) =>
    invoke<ManagedRuntimeReport>("get_managed_runtime_status", { request }),
  getShellSnapshot: () => invoke<ShellSnapshot>("get_shell_snapshot"),
  mountDshSurface: (request) =>
    invoke<DshSurfaceStatus>("mount_dsh_surface", { request }),
  probeAttachedEnvironment: (request) =>
    invoke<AttachedHealthReport>("probe_attached_environment", { request }),
  reloadDshSurface: (request) =>
    invoke<DshSurfaceStatus>("reload_dsh_surface", { request }),
  saveEnvironment: (environment) =>
    invoke<EnvironmentCatalog>("save_environment", { environment }),
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
