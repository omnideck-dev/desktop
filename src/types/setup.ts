// Mirrors src-tauri/src/bootstrap.rs's SetupState/Diagnostic (serde
// rename_all = "camelCase"). Keep these two in sync by hand, same
// convention as types/cli.ts.

export interface Diagnostic {
  id: string;
  label: string;
  status: "pass" | "issue" | "waiting";
}

export interface SetupState {
  stage: "welcome" | "preparing" | "ready" | "error";
  title: string;
  detail: string;
  progress: number | null;
  indeterminate: boolean;
  canStart: boolean;
  canRetry: boolean;
  canOpen: boolean;
  activity: string | null;
  primaryAction: string | null;
  primaryLabel: string | null;
  secondaryAction: string | null;
  secondaryLabel: string | null;
  diagnostics: Diagnostic[] | null;
  technical: string | null;
}
