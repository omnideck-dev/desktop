import { useState } from "react";
import { useInstances } from "../../hooks/useInstances";
import type { ListEntry } from "../../types/cli";
import { cliErrorSummary } from "../../types/cli";
import DeckRow from "./DeckRow";
import InstanceConfigPanel from "./InstanceConfigPanel";
import InstanceDetailPanel from "./InstanceDetailPanel";
import LogsPanel from "./LogsPanel";
import NewDeckForm from "./NewDeckForm";
import RemoveDeckDialog from "./RemoveDeckDialog";

export default function DashboardView({
  onOpenInstance,
}: {
  onOpenInstance: (name: string, port: string) => void;
}) {
  const { entries, loading, error, refresh } = useInstances();
  const [logsFor, setLogsFor] = useState<string | null>(null);
  const [removeFor, setRemoveFor] = useState<string | null>(null);
  const [configFor, setConfigFor] = useState<string | null>(null);
  const [detailFor, setDetailFor] = useState<ListEntry | null>(null);
  const [showNewDeck, setShowNewDeck] = useState(false);

  const engineUnreachable = error?.kind === "cli" && error.code === "ENGINE_NOT_FOUND";

  return (
    <div className="dashboard">
      <div className="dashboard__header">
        <h1>Decks</h1>
        <button className="primary-button" onClick={() => setShowNewDeck(true)}>
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
                <DeckRow
                  key={entry.name}
                  entry={entry}
                  onChanged={refresh}
                  onOpenLogs={setLogsFor}
                  onRemove={setRemoveFor}
                  onOpenUi={onOpenInstance}
                  onOpenDetail={setDetailFor}
                  onOpenConfig={setConfigFor}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {logsFor && <LogsPanel name={logsFor} onClose={() => setLogsFor(null)} />}

      {removeFor && (
        <RemoveDeckDialog
          name={removeFor}
          onClose={() => setRemoveFor(null)}
          onRemoved={refresh}
        />
      )}

      {configFor && <InstanceConfigPanel name={configFor} onClose={() => setConfigFor(null)} />}

      {detailFor && (
        <InstanceDetailPanel
          entry={detailFor}
          onClose={() => setDetailFor(null)}
          onChanged={refresh}
        />
      )}

      {showNewDeck && (
        <NewDeckForm
          onClose={() => setShowNewDeck(false)}
          onCreated={() => {
            refresh();
            setShowNewDeck(false);
          }}
        />
      )}
    </div>
  );
}
