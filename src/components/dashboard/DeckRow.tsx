import StatusDot from "../StatusDot";
import type { ListEntry } from "../../types/cli";

/** Null live-stat fields render as a dash placeholder, never a fake "0%" —
 * JSON_MODE_SPEC.md §4 is explicit that null here means "not measured," not
 * "measured as zero." */
function stat(value: string | null): string {
  return value ?? "—";
}

export default function DeckRow({ entry }: { entry: ListEntry }) {
  return (
    <tr className="deck-row">
      <td className="deck-row__name">
        <div className="deck-row__name-main">{entry.name}</div>
        <div className="deck-row__name-sub">{entry.image}</div>
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
      <td className="deck-row__actions">
        <button className="ghost-button" disabled title="Lifecycle actions land in a later pass">
          Open UI
        </button>
      </td>
    </tr>
  );
}
