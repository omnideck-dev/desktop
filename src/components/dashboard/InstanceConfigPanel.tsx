import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CliError, ConfigInfo } from "../../types/cli";
import { cliErrorSummary } from "../../types/cli";

/** Read-only `config show --json` display — editing (`config set`) isn't
 * wired up yet. */
export default function InstanceConfigPanel({ name, onClose }: { name: string; onClose: () => void }) {
  const [config, setConfig] = useState<ConfigInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<ConfigInfo>("instance_config", { name })
      .then((c) => {
        if (!cancelled) setConfig(c);
      })
      .catch((err: CliError) => {
        if (!cancelled) setError(cliErrorSummary(err));
      });
    return () => {
      cancelled = true;
    };
  }, [name]);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-panel" onClick={(e) => e.stopPropagation()}>
        <div className="modal-panel__header">
          <h2>Config — {name}</h2>
          <button className="ghost-button" onClick={onClose}>
            Close
          </button>
        </div>

        {error && <p className="settings__error">{error}</p>}

        {config && (
          <dl className="settings__facts">
            <dt>Container name</dt>
            <dd>{config.containerName}</dd>
            <dt>Home volume</dt>
            <dd>{config.homeVolume}</dd>
            <dt>State volume</dt>
            <dd>{config.stateVolume}</dd>
            <dt>Memory</dt>
            <dd>{config.memory}</dd>
            <dt>Shm size</dt>
            <dd>{config.shmSize}</dd>
            <dt>Web UI port</dt>
            <dd>{config.webUiPort}</dd>
            <dt>Runtime</dt>
            <dd>{config.runtime}</dd>
            <dt>Image</dt>
            <dd>{config.image}</dd>
            <dt>Installed</dt>
            <dd>{config.installedAt}</dd>
          </dl>
        )}
      </div>
    </div>
  );
}
