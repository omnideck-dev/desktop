// Mirrors src-tauri/src/cli_bridge.rs. Keep these two in sync by hand — the
// Rust structs are the source of truth (they're what actually deserializes
// the CLI's JSON), this file just gives the frontend the same shapes.

export interface VersionInfo {
  version: string;
  commit: string;
  date: string;
  jsonContract: number;
}

/** One row of `omnideck list --json`. See JSON_MODE_SPEC.md §4. */
export interface ListEntry {
  name: string;
  container: string;
  status: string;
  image: string;
  webUiPort: string;
  /** Null whenever the instance isn't active — never a fake "0%". */
  cpu: string | null;
  cpuPct: number | null;
  ram: string | null;
  ramTotal: string | null;
  ramPct: number | null;
  uptime: string | null;
  restarts: number | null;
  /** "" when no healthcheck is configured, never null. */
  health: string;
  created: string;
}

export type CliError =
  | { kind: "spawn"; message: string }
  | { kind: "no_output" }
  | { kind: "parse"; message: string; raw: string }
  | {
      kind: "cli";
      code: string;
      message: string;
      hint?: string;
      action?: string;
      actionValue?: string;
      instances?: string[];
    }
  | { kind: "contract_mismatch"; expected: number; actual: number };

export function cliErrorSummary(err: CliError): string {
  switch (err.kind) {
    case "spawn":
      return "Couldn't run the omnideck CLI — is it installed and on PATH?";
    case "no_output":
      return "The omnideck CLI produced no output.";
    case "parse":
      return `Couldn't understand the omnideck CLI's response: ${err.message}`;
    case "cli":
      return err.message;
    case "contract_mismatch":
      return `This app build expects omnideck CLI contract v${err.expected}, but found v${err.actual}.`;
  }
}
