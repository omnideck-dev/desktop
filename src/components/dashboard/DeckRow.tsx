import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import StatusDot from "../StatusDot";
import DropdownMenu from "../DropdownMenu";
import { InfoIcon, PauseIcon, PlayIcon, RefreshIcon } from "../icons";
import type { CliError, InstanceStatus, ListEntry } from "../../types/cli";
import { cliErrorSummary } from "../../types/cli";

/** Null live-stat fields render as a dash placeholder, never a fake "0%" —
 * JSON_MODE_SPEC.md §4 is explicit that null here means "not measured," not
 * "measured as zero." */
function stat(value: string | null): string {
  return value ?? "—";
}

type Action = "start" | "stop" | "restart" | "update";

const ACTION_COMMAND: Record<Action, string> = {
  start: "start_instance",
  stop: "stop_instance",
  restart: "restart_instance",
  update: "update_instance",
};

export default function DeckRow({
  entry,
  onOpenLogs,
  onRemove,
  onOpenUi,
  onOpenDetail,
  onOpenConfig,
  onChanged,
}: {
  entry: ListEntry;
  onOpenLogs: (name: string) => void;
  onRemove: (name: string) => void;
  onOpenUi: (name: string, port: string) => void;
  onOpenDetail: (entry: ListEntry) => void;
  onOpenConfig: (name: string) => void;
  onChanged: () => void;
}) {
  const [pending, setPending] = useState<Action | null>(null);
  const [error, setError] = useState<string | null>(null);

  const isActive = entry.status === "running" || entry.status === "paused";

  async function runAction(action: Action) {
    setPending(action);
    setError(null);
    try {
      await invoke<InstanceStatus>(ACTION_COMMAND[action], { name: entry.name });
    } catch (err) {
      setError(cliErrorSummary(err as CliError));
    } finally {
      setPending(null);
      onChanged();
    }
  }

  const busy = pending !== null;

  return (
    <tr className="deck-row" onClick={() => onOpenUi(entry.name, entry.webUiPort)}>
      <td className="deck-row__name">
        <div className="deck-row__name-main">{entry.name}</div>
        <div className="deck-row__name-sub">{entry.image}</div>
        {error && <div className="deck-row__error">{error}</div>}
      </td>
      <td>
        <StatusDot status={entry.status} />
      </td>
      <td className="deck-row__num">{entry.webUiPort}</td>
      <td className="deck-row__num">{stat(entry.cpu)}</td>
      <td className="deck-row__num">
        {entry.ram ? `${entry.ram} / ${entry.ramTotal}` : "—"}
      </td>
      <td className="deck-row__num">{stat(entry.uptime)}</td>
      <td className="deck-row__num">{entry.restarts ?? "—"}</td>
      <td className="deck-row__actions" onClick={(e) => e.stopPropagation()}>
        <div className="deck-row__actions-inner">
          <button
            className="icon-button"
            aria-label={isActive ? "Stop" : "Start"}
            title={isActive ? "Stop" : "Start"}
            disabled={busy}
            onClick={() => runAction(isActive ? "stop" : "start")}
          >
            {isActive ? <PauseIcon /> : <PlayIcon />}
          </button>

          <button
            className="icon-button"
            aria-label="Update"
            title="Pull latest image and recreate"
            disabled={busy}
            onClick={() => runAction("update")}
          >
            <RefreshIcon />
          </button>

          <button
            className="icon-button"
            aria-label="Details"
            title="Details"
            onClick={() => onOpenDetail(entry)}
          >
            <InfoIcon />
          </button>

          <DropdownMenu
            label={`More actions for ${entry.name}`}
            items={[
              { label: "Open UI", onClick: () => onOpenUi(entry.name, entry.webUiPort) },
              { label: "Restart", onClick: () => runAction("restart"), disabled: !isActive || busy },
              { label: "Logs", onClick: () => onOpenLogs(entry.name) },
              { label: "Config", onClick: () => onOpenConfig(entry.name) },
              { label: "Doctor", onClick: () => onOpenDetail(entry) },
              { label: "Remove", onClick: () => onRemove(entry.name), danger: true },
            ]}
          />
        </div>
      </td>
    </tr>
  );
}
