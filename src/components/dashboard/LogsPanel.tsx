import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CliError, LogsResult } from "../../types/cli";
import { cliErrorSummary } from "../../types/cli";

const TAIL_OPTIONS = [50, 200, 1000];

/** DESIGN.md #8 — historical per-Deck logs (`logs --json`, non-follow).
 * Follow/live streaming is a separate later pass. */
export default function LogsPanel({ name, onClose }: { name: string; onClose: () => void }) {
  const [tail, setTail] = useState(50);
  const [lines, setLines] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLines(null);
    setError(null);
    invoke<LogsResult>("instance_logs", { name, tail })
      .then((result) => {
        if (!cancelled) setLines(result.lines);
      })
      .catch((err: CliError) => {
        if (!cancelled) setError(cliErrorSummary(err));
      });
    return () => {
      cancelled = true;
    };
  }, [name, tail]);

  async function copyAll() {
    if (!lines) return;
    await navigator.clipboard.writeText(lines.join("\n"));
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-panel logs-panel" onClick={(e) => e.stopPropagation()}>
        <div className="modal-panel__header">
          <h2>Logs — {name}</h2>
          <button className="ghost-button" onClick={onClose}>
            Close
          </button>
        </div>

        <div className="logs-panel__controls">
          <div className="segmented">
            {TAIL_OPTIONS.map((n) => (
              <button
                key={n}
                className={`segmented__item ${tail === n ? "is-selected" : ""}`}
                onClick={() => setTail(n)}
              >
                Last {n}
              </button>
            ))}
          </div>
          <button className="ghost-button" onClick={copyAll} disabled={!lines}>
            {copied ? "Copied" : "Copy to clipboard"}
          </button>
        </div>

        {error ? (
          <p className="settings__error">{error}</p>
        ) : lines === null ? (
          <p>Loading…</p>
        ) : lines.length === 0 ? (
          <p className="dashboard__empty-sub">No log lines.</p>
        ) : (
          <pre className="logs-panel__output">{lines.join("\n")}</pre>
        )}
      </div>
    </div>
  );
}
