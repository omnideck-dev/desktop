import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CliError, RemoveResult, StreamEvent } from "../../types/cli";
import { cliErrorSummary } from "../../types/cli";

type VolumesChoice = "keep" | "delete";
type BackupChoice = "backup" | "no-backup";
type Stage = { name: string; state: StreamEvent["state"] };

/** DESIGN.md #5 — `remove` is destructive; the CLI requires explicit,
 * non-defaulted choices, so neither radio group below has a pre-selected
 * option. */
export default function RemoveDeckDialog({
  name,
  onClose,
  onRemoved,
}: {
  name: string;
  onClose: () => void;
  onRemoved: () => void;
}) {
  const [volumes, setVolumes] = useState<VolumesChoice | null>(null);
  const [backup, setBackup] = useState<BackupChoice | null>(null);
  const [stages, setStages] = useState<Stage[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<RemoveResult | null>(null);

  const canSubmit = volumes === "keep" || (volumes === "delete" && backup !== null);

  const unlisten = useRef<() => void>(() => {});
  useEffect(() => {
    listen<StreamEvent>("remove-progress", (evt) => {
      const { stage, state } = evt.payload;
      if (stage === "complete") return;
      setStages((prev) => {
        const idx = prev.findIndex((s) => s.name === stage);
        const next = { name: stage, state };
        if (idx === -1) return [...prev, next];
        const copy = [...prev];
        copy[idx] = next;
        return copy;
      });
    }).then((fn) => {
      unlisten.current = fn;
    });
    return () => unlisten.current();
  }, []);

  async function handleConfirm() {
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);
    setStages([]);
    try {
      const res = await invoke<RemoveResult>("remove_instance", {
        name,
        keepVolumes: volumes === "keep",
        backup: backup === "backup",
      });
      setResult(res);
      onRemoved();
    } catch (err) {
      setError(cliErrorSummary(err as CliError));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={submitting ? undefined : onClose}>
      <div className="modal-panel" onClick={(e) => e.stopPropagation()}>
        <div className="modal-panel__header">
          <h2>Remove {name}</h2>
          <button className="ghost-button" onClick={onClose} disabled={submitting}>
            Close
          </button>
        </div>

        {result ? (
          <div>
            <p>Deck "{name}" removed.</p>
            {result.backupPath && (
              <p className="dashboard__empty-sub">Backup saved to {result.backupPath}</p>
            )}
          </div>
        ) : (
          <>
            <fieldset className="remove-choice">
              <legend>Volumes</legend>
              <label>
                <input
                  type="radio"
                  name="volumes"
                  checked={volumes === "keep"}
                  onChange={() => setVolumes("keep")}
                  disabled={submitting}
                />
                Keep — data stays on disk, Deck can be re-added later
              </label>
              <label>
                <input
                  type="radio"
                  name="volumes"
                  checked={volumes === "delete"}
                  onChange={() => setVolumes("delete")}
                  disabled={submitting}
                />
                Delete — permanently remove this Deck's data
              </label>
            </fieldset>

            {volumes === "delete" && (
              <fieldset className="remove-choice">
                <legend>Before deleting</legend>
                <label>
                  <input
                    type="radio"
                    name="backup"
                    checked={backup === "backup"}
                    onChange={() => setBackup("backup")}
                    disabled={submitting}
                  />
                  Back up first (saved to your home directory)
                </label>
                <label>
                  <input
                    type="radio"
                    name="backup"
                    checked={backup === "no-backup"}
                    onChange={() => setBackup("no-backup")}
                    disabled={submitting}
                  />
                  Don't back up
                </label>
              </fieldset>
            )}

            {error && <p className="settings__error">{error}</p>}

            {stages.length > 0 && (
              <ul className="stage-list">
                {stages.map((s) => (
                  <li key={s.name} className={`stage-list__item stage-list__item--${s.state}`}>
                    <span>{s.name.replace(/_/g, " ")}</span>
                    <span className="stage-list__state">{s.state}</span>
                  </li>
                ))}
              </ul>
            )}

            <button
              className="primary-button primary-button--danger"
              onClick={handleConfirm}
              disabled={!canSubmit || submitting}
            >
              {submitting ? "Removing…" : "Remove Deck"}
            </button>
          </>
        )}
      </div>
    </div>
  );
}
