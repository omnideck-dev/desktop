import type { CliError } from "../types/cli";
import { cliErrorSummary } from "../types/cli";

/** Replaces the whole shell — DESIGN.md #12: a CLI-missing or
 * version-contract mismatch is a harder failure than any per-Deck error,
 * so it doesn't get a toast or a banner, it blocks. */
export default function BlockingScreen({ error }: { error: CliError }) {
  const title =
    error.kind === "contract_mismatch" || error.kind === "version_too_old"
      ? "Omnideck CLI version mismatch"
      : "Can't reach the omnideck CLI";

  return (
    <div className="blocking-screen">
      <div className="blocking-screen__card">
        <p className="eyebrow eyebrow--danger">SETUP REQUIRED</p>
        <h1>{title}</h1>
        <p className="blocking-screen__detail">{cliErrorSummary(error)}</p>
        {error.kind === "spawn" && (
          <p className="blocking-screen__detail">
            Make sure the omnideck CLI is installed and reachable on your PATH, then relaunch Omnideck.
          </p>
        )}
      </div>
    </div>
  );
}
