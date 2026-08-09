import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InstanceStatus } from "../types/cli";

const POLL_INTERVAL_MS = 3000;

/** DESIGN.md #7 — chrome around an iframe to a Deck's own web app.
 *
 * "Connection refused" isn't reliably detectable from a cross-origin (or
 * even same-origin-different-port) iframe's load events — a refused
 * connection typically still fires `onLoad` with the browser's own error
 * page as content. Instead of guessing from the iframe, this polls
 * `instance_status` directly and swaps to a placeholder the moment the
 * underlying Deck isn't `running` anymore, which is the actual condition
 * DESIGN.md cares about ("Deck was stopped/removed while its tab was open"). */
export default function InstanceWebviewTab({ name, port }: { name: string; port: string }) {
  const [loaded, setLoaded] = useState(false);
  const [running, setRunning] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function check() {
      try {
        const status = await invoke<InstanceStatus>("instance_status", { name });
        if (!cancelled) setRunning(status.status === "running");
      } catch {
        if (!cancelled) setRunning(false);
      }
    }

    check();
    const id = setInterval(check, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [name]);

  if (!running) {
    return (
      <div className="instance-tab__placeholder">
        <p>"{name}" isn't running.</p>
        <p className="dashboard__empty-sub">Start it from the Dashboard to reconnect.</p>
      </div>
    );
  }

  return (
    <div className="instance-tab">
      {!loaded && (
        <div className="instance-tab__loading">
          <div className="spinner" aria-label="Loading" />
        </div>
      )}
      <iframe
        key={`${name}-${port}`}
        src={`http://127.0.0.1:${port}`}
        className="instance-tab__frame"
        title={name}
        onLoad={() => setLoaded(true)}
      />
    </div>
  );
}
