export type RuntimeState =
  | "unconfigured"
  | "stopped"
  | "starting"
  | "healthy"
  | "crashed"
  | "stopping"
  | "attached"
  | "detached"
  | "degraded"
  | "unavailable";

export interface ShellSnapshot {
  phase: "shell-mvp";
  runtimeState: RuntimeState;
  environmentId: string | null;
  generation: number;
}

export type HarnessMode = "repository" | "executable" | "command";
export type BackendOwnership = "managed" | "attached";

export interface DshEnvironment {
  schemaVersion: 1;
  id: string;
  label: string;
  harness: {
    mode: HarnessMode;
    path: string;
    cwd?: string;
    args?: string[];
  };
  dshHome: string;
  profile: string;
  nodePath?: string;
  endpoint: {
    host: "127.0.0.1";
    port: "auto" | number;
  };
  ownership: BackendOwnership;
  policy?: {
    autoRestartOnCrash?: boolean;
    allowNativeAdapter?: boolean;
  };
}

export interface ValidationIssue {
  field: string;
  code: string;
  message: string;
}

export interface LaunchArgumentPreview {
  category: string;
  display: string;
}

export interface LaunchPreview {
  source: HarnessMode;
  executable: string;
  cwd: string | null;
  ownership: BackendOwnership;
  endpoint: string;
  arguments: LaunchArgumentPreview[];
}

export interface EnvironmentValidation {
  valid: boolean;
  issues: ValidationIssue[];
  launchPreview: LaunchPreview | null;
}

export interface EnvironmentCatalog {
  schemaVersion: 1;
  revision: number;
  activeEnvironmentId: string | null;
  environments: DshEnvironment[];
}

export type DiscoverySource = "explicit" | "dsh_path" | "path" | "global";
export type DiscoveryCandidateStatus =
  | "available"
  | "missing"
  | "requires_recipe"
  | "unverified";

export interface HarnessDiscoveryRequest {
  schemaVersion: 1;
  explicitPaths: string[];
  includeDshPath: boolean;
  includePath: boolean;
}

export interface DiscoveryEvidence {
  code: string;
  severity: "info" | "warning" | "error";
  message: string;
}

export interface HarnessCandidate {
  id: string;
  source: DiscoverySource;
  mode: "executable" | "repository";
  requestedPath: string;
  canonicalPath: string | null;
  status: DiscoveryCandidateStatus;
  launchable: boolean;
  version: string | null;
  evidence: DiscoveryEvidence[];
}

export interface HarnessDiscoveryReport {
  schemaVersion: 1;
  scannedSources: DiscoverySource[];
  deferredSources: DiscoverySource[];
  candidates: HarnessCandidate[];
}

export interface AttachedHealthRequest {
  schemaVersion: 1;
  environmentId: string;
}

export interface AttachedHealthEvidence {
  code: string;
  severity: "info" | "warning" | "error";
  message: string;
}

export interface AttachedHealthReport {
  schemaVersion: 1;
  environmentId: string;
  ownership: "attached";
  state: "attached" | "detached";
  reachability: "reachable" | "refused" | "timeout" | "io_error";
  identity: "unverified";
  processOwnership: "external";
  lifecycleMutation: "denied";
  endpoint: {
    host: "127.0.0.1";
    port: number;
  };
  timeoutMs: 750;
  latencyMs: number | null;
  observedAtUnixMs: number;
  evidence: AttachedHealthEvidence[];
}

export interface ManagedRuntimeStartRequest {
  schemaVersion: 1;
  environmentId: string;
}

export interface ManagedRuntimeStatusRequest {
  schemaVersion: 1;
  environmentId: string;
}

export interface ManagedRuntimeStopRequest {
  schemaVersion: 1;
  environmentId: string;
  expectedGeneration: number;
}

export interface ManagedRuntimeEvidence {
  code: string;
  severity: "info" | "warning" | "error";
  message: string;
}

export interface ManagedRuntimeReport {
  schemaVersion: 1;
  environmentId: string;
  ownership: "managed";
  state: "stopped" | "starting" | "healthy" | "stopping" | "crashed";
  generation: number;
  instanceId: string | null;
  processOwnership: "none" | "owned";
  lifecycleMutation: "allowed";
  readiness: "not_started" | "waiting" | "verified" | "failed";
  endpoint: {
    scheme: "http";
    host: "127.0.0.1";
    port: number;
    source: "managed_process_output";
    verification: "owned_generation_output_and_tcp";
  } | null;
  stopDisposition: "not_requested" | "graceful" | "forced";
  observedAtUnixMs: number;
  evidence: ManagedRuntimeEvidence[];
}

export interface DesktopCommandError {
  code: string;
  message: string;
  retryable: boolean;
  correlationId: string;
}

export interface DshSurfacePolicyRequest {
  schemaVersion: 1;
  environmentId: string;
}

export interface DshSurfacePolicy {
  schemaVersion: 1;
  environmentId: string;
  surfaceLabel: "dsh-surface";
  allowedOrigin: {
    scheme: "http";
    host: "127.0.0.1";
    port: number;
  };
  sameOriginMainFrame: "allow";
  externalHttpNavigation: "delegate_with_user_action";
  newWindow: "deny";
  downloads: "deny";
  permissions: "deny";
  privilegedIpc: "denied";
  domInjection: "denied";
  rendererPatch: "denied";
  automaticExternalOpen: false;
}

export type DshSurfaceNavigationKind =
  | "main_frame"
  | "new_window"
  | "download"
  | "permission";

export interface DshSurfaceNavigationRequest {
  schemaVersion: 1;
  environmentId: string;
  candidateUrl: string;
  navigationKind: DshSurfaceNavigationKind;
  userGesture: boolean;
}

export interface DshSurfaceNavigationDecision {
  schemaVersion: 1;
  environmentId: string;
  navigationKind: DshSurfaceNavigationKind;
  candidateOrigin: string | null;
  disposition: "allow_in_surface" | "delegate_external" | "deny";
  reason:
    | "SAME_ORIGIN"
    | "EXTERNAL_HTTP_USER_ACTION"
    | "EXTERNAL_NAVIGATION_NO_USER_GESTURE"
    | "INVALID_URL"
    | "CREDENTIALS_FORBIDDEN"
    | "SCHEME_FORBIDDEN"
    | "LOOPBACK_ORIGIN_MISMATCH"
    | "NEW_WINDOW_DENIED"
    | "DOWNLOAD_DENIED"
    | "PERMISSION_DENIED"
    | "POLICY_UNAVAILABLE";
  userGestureRequired: boolean;
  userConfirmationRequired: boolean;
  privilegedIpc: "denied";
  openAutomatically: false;
}

export interface DshSurfaceBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface DshSurfaceMountRequest {
  schemaVersion: 1;
  environmentId: string;
  expectedGeneration: number;
  bounds: DshSurfaceBounds;
  visible: boolean;
}

export interface DshSurfaceStatusRequest {
  schemaVersion: 1;
  environmentId: string;
  expectedGeneration: number;
}

export interface DshSurfaceLayoutRequest extends DshSurfaceMountRequest {}

export interface DshSurfaceReloadRequest extends DshSurfaceStatusRequest {}

export interface DshSurfaceUnmountRequest extends DshSurfaceStatusRequest {}

export interface DshSurfaceStatus {
  schemaVersion: 1;
  environmentId: string;
  generation: number;
  surfaceLabel: "dsh-surface";
  state:
    | "unmounted"
    | "mounting"
    | "loading"
    | "ready"
    | "hidden"
    | "error"
    | "stale"
    | "unsupported_platform";
  platform: "windows" | "macos" | "linux" | "other";
  verifiedOrigin: {
    scheme: "http";
    host: "127.0.0.1";
    port: number;
  };
  bounds: DshSurfaceBounds | null;
  visible: boolean;
  policy: {
    sameOriginNavigation: "allow";
    crossOriginNavigation: "deny";
    newWindow: "deny";
    downloads: "deny";
    pagePermissions: "deny";
    privilegedIpc: "denied";
    domInjection: "denied";
    automaticExternalOpen: false;
  };
  error: {
    code: "UNAVAILABLE" | "UNAUTHORIZED" | "STALE_GENERATION" | "MALFORMED_MESSAGE";
    reason:
      | "unsupported_platform"
      | "surface_create_failed"
      | "surface_operation_failed"
      | "runtime_binding_lost"
      | "stale_generation";
    message: string;
  } | null;
}
