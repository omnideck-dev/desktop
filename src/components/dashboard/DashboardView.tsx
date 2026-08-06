import { useInstances } from "../../hooks/useInstances";
import { cliErrorSummary } from "../../types/cli";
import DeckRow from "./DeckRow";

export default function DashboardView() {
  const { entries, loading, error } = useInstances();

  const engineUnreachable = error?.kind === "cli" && error.code === "ENGINE_NOT_FOUND";

  return (
    <div className="dashboard">
      <div className="dashboard__header">
        <h1>Decks</h1>
        <button className="primary-button" disabled title="Wired up in a later pass">
          + New Deck
        </button>
      </div>

      {error && (
        <div className={`banner ${engineUnreachable ? "banner--warning" : "banner--danger"}`}>
          {engineUnreachable
            ? "Container runtime unreachable — start Podman/Docker to resume managing Decks."
            : cliErrorSummary(error)}
        </div>
      )}

      {loading && entries === null ? (
        <div className="dashboard__empty">
          <p>Checking your Decks…</p>
        </div>
      ) : entries === null || entries.length === 0 ? (
        <div className="dashboard__empty">
          <p>No Decks yet.</p>
          <p className="dashboard__empty-sub">Create your first Deck to get started.</p>
        </div>
      ) : (
        <div className="dashboard__table-wrap">
          <table className="dashboard__table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Status</th>
                <th>Port</th>
                <th>CPU</th>
                <th>Memory</th>
                <th>Uptime</th>
                <th>Restarts</th>
                <th aria-hidden="true" />
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <DeckRow key={entry.name} entry={entry} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
