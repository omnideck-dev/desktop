import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CliError, DoctorResult, InstanceStatus, ListEntry } from "../../types/cli";
import { cliErrorSummary } from "../../types/cli";

const STATUS_LABEL: Record<string, string> = {
  pass: "Pass",
  fail: "Fail",
  warn: "Warning",
  info: "Info",
};

/** DESIGN.md #6 — full stat set + doctor --json per-check status, with a
 * repair CTA driven by each failing check's action/actionValue. */
export default function InstanceDetailPanel({
  entry,
  onClose,
  onChanged,
}: {
  entry: ListEntry;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [status, setStatus] = useState<InstanceStatus | null>(null);
  const [doctor, setDoctor] = useState<DoctorResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [actionPending, setActionPending] = useState<string | null>(null);

  async function load() {
    setError(null);
    try {
      const [s, d] = await Promise.all([
        invoke<InstanceStatus>("instance_status", { name: entry.name }),
        invoke<DoctorResult>("instance_doctor", { name: entry.name }),
      ]);
      setStatus(s);
      setDoctor(d);
    } catch (err) {
      setError(cliErrorSummary(err as CliError));
    }
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entry.name]);

  async function runRepair(action: string, actionValue: string) {
    setActionPending(action);
    try {
      const command = action === "start_instance" ? "start_instance" : "update_instance";
      await invoke(command, { name: actionValue });
      onChanged();
      await load();
    } catch (err) {
      setError(cliErrorSummary(err as CliError));
    } finally {
      setActionPending(null);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-panel" onClick={(e) => e.stopPropagation()}>
        <div className="modal-panel__header">
          <h2>{entry.name}</h2>
          <button className="ghost-button" onClick={onClose}>
            Close
          </button>
        </div>

        {error && <p className="settings__error">{error}</p>}

        {status && (
          <dl className="settings__facts">
            <dt>Status</dt>
            <dd>{status.status}</dd>
            <dt>Image</dt>
            <dd>{status.image}</dd>
            <dt>Engine</dt>
            <dd>{status.engine}</dd>
            <dt>Web UI port</dt>
            <dd>{status.webUiPort}</dd>
            <dt>Home volume</dt>
            <dd>
              {status.homeVolume.name} ({status.homeVolume.exists ? "exists" : "missing"})
            </dd>
            <dt>State volume</dt>
            <dd>
              {status.stateVolume.name} ({status.stateVolume.exists ? "exists" : "missing"})
            </dd>
            <dt>Local AI</dt>
            <dd>{status.ollama.reachable ? `reachable (${status.ollama.host})` : "not reachable"}</dd>
          </dl>
        )}

        {doctor && (
          <ul className="stage-list">
            {doctor.checks.map((check) => (
              <li key={check.label} className={`stage-list__item stage-list__item--${check.status}`}>
                <span>
                  {check.label} — {STATUS_LABEL[check.status] ?? check.status}
                  {check.detail && (
                    <span className="stage-list__detail"> · {check.detail}</span>
                  )}
                </span>
                {check.action && check.actionLabel && check.actionValue && (
                  <button
                    className="ghost-button"
                    disabled={actionPending !== null}
                    onClick={() => runRepair(check.action!, check.actionValue!)}
                  >
                    {actionPending === check.action ? "…" : check.actionLabel}
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
