import type { BackendOwnership, DshEnvironment, HarnessMode, ValidationIssue } from "../../../src/contracts";

const environmentIdPattern = /^[a-z][a-z0-9-]{1,63}$/;
const reservedArguments = ["--host", "--port", "--no-open", "--trusted-host"];

export interface EnvironmentDraft {
  id: string;
  label: string;
  harnessMode: HarnessMode;
  harnessPath: string;
  cwd: string;
  extraArguments: string;
  dshHome: string;
  profile: string;
  nodePath: string;
  port: string;
  ownership: BackendOwnership;
  autoRestartOnCrash: boolean;
  allowNativeAdapter: boolean;
}

export const initialEnvironmentDraft: EnvironmentDraft = {
  id: "local-dsh",
  label: "Local DSH",
  harnessMode: "executable",
  harnessPath: "dsh",
  cwd: "",
  extraArguments: "",
  dshHome: "",
  profile: "default",
  nodePath: "",
  port: "auto",
  ownership: "managed",
  // Backend semantics: an absent policy means conservative defaults
  // (autoRestartOnCrash=false); the UI always emits the policy explicitly,
  // so an explicit true here is honored by the Supervisor (ADR-0013).
  autoRestartOnCrash: true,
  allowNativeAdapter: false,
};

export function environmentToDraft(environment: DshEnvironment): EnvironmentDraft {
  return {
    id: environment.id,
    label: environment.label,
    harnessMode: environment.harness.mode,
    harnessPath: environment.harness.path,
    cwd: environment.harness.cwd ?? "",
    extraArguments: (environment.harness.args ?? []).join("\n"),
    dshHome: environment.dshHome,
    profile: environment.profile,
    nodePath: environment.nodePath ?? "",
    port: String(environment.endpoint.port),
    ownership: environment.ownership,
    autoRestartOnCrash: environment.policy?.autoRestartOnCrash ?? true,
    allowNativeAdapter: environment.policy?.allowNativeAdapter ?? false,
  };
}

export interface DraftConversion {
  environment: DshEnvironment | null;
  issues: ValidationIssue[];
}

export function convertEnvironmentDraft(draft: EnvironmentDraft): DraftConversion {
  const issues: ValidationIssue[] = [];
  const extraArguments = draft.extraArguments
    .split(/\r?\n/)
    .map((argument) => argument.trim())
    .filter(Boolean);

  if (!environmentIdPattern.test(draft.id)) {
    issues.push(issue("id", "MALFORMED_VALUE", "Use 2-64 lowercase letters, digits, or hyphens."));
  }
  if (!draft.label.trim() || draft.label.trim().length > 128) {
    issues.push(issue("label", "MALFORMED_VALUE", "Label must contain 1-128 characters."));
  }
  if (!draft.harnessPath.trim()) {
    issues.push(issue("harness.path", "UNAVAILABLE", "Select an existing DSH launch source."));
  }
  if (!draft.dshHome.trim()) {
    issues.push(issue("dshHome", "MALFORMED_VALUE", "DSH_HOME is required."));
  }
  if (!draft.profile.trim()) {
    issues.push(issue("profile", "MALFORMED_VALUE", "Profile is required."));
  }
  if (extraArguments.length > 64) {
    issues.push(issue("harness.args", "MALFORMED_VALUE", "At most 64 extra arguments are allowed."));
  }
  if (extraArguments.some(isReservedArgument)) {
    issues.push(
      issue(
        "harness.args",
        "UNAUTHORIZED",
        "Host, port, trusted-host, and browser-open policy are Supervisor-owned.",
      ),
    );
  }

  const port = parsePort(draft.port);
  if (port === null) {
    issues.push(issue("endpoint.port", "MALFORMED_VALUE", "Use auto or a port from 1024 to 65535."));
  }

  if (issues.length > 0 || port === null) {
    return { environment: null, issues };
  }

  return {
    issues: [],
    environment: {
      schemaVersion: 1,
      id: draft.id,
      label: draft.label.trim(),
      harness: {
        mode: draft.harnessMode,
        path: draft.harnessPath.trim(),
        ...(draft.cwd.trim() ? { cwd: draft.cwd.trim() } : {}),
        ...(extraArguments.length ? { args: extraArguments } : {}),
      },
      dshHome: draft.dshHome.trim(),
      profile: draft.profile.trim(),
      ...(draft.nodePath.trim() ? { nodePath: draft.nodePath.trim() } : {}),
      endpoint: { host: "127.0.0.1", port },
      ownership: draft.ownership,
      policy: {
        autoRestartOnCrash: draft.autoRestartOnCrash,
        allowNativeAdapter: draft.allowNativeAdapter,
      },
    },
  };
}

function parsePort(value: string): "auto" | number | null {
  if (value === "auto") return "auto";
  if (!/^\d+$/.test(value)) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= 1024 && parsed <= 65535 ? parsed : null;
}

function isReservedArgument(argument: string) {
  return reservedArguments.some(
    (reserved) => argument === reserved || argument.startsWith(`${reserved}=`),
  );
}

function issue(field: string, code: string, message: string): ValidationIssue {
  return { field, code, message };
}
