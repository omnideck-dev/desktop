import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CliError, VersionInfo } from "../types/cli";

export type CliVersionState =
  | { status: "checking" }
  | { status: "ok"; info: VersionInfo }
  | { status: "error"; error: CliError };

/** Checked once at startup — this is the "can we safely talk to the CLI at
 * all" gate (AGENT.md: cli_bridge validates jsonContract and returns a
 * distinguishable error on mismatch). */
export function useCliVersion(): CliVersionState {
  const [state, setState] = useState<CliVersionState>({ status: "checking" });

  useEffect(() => {
    let cancelled = false;
    invoke<VersionInfo>("cli_version_contract")
      .then((info) => {
        if (!cancelled) setState({ status: "ok", info });
      })
      .catch((error: CliError) => {
        if (!cancelled) setState({ status: "error", error });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return state;
}
