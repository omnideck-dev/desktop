import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AddSuggestion, CliError, InstanceStatus, StreamEvent } from "../../types/cli";
import { cliErrorSummary } from "../../types/cli";

const NAME_RE = /^[a-zA-Z0-9][a-zA-Z0-9._-]*$/;
const MEMORY_RE = /^[0-9]+(\.[0-9]+)?[a-zA-Z]+$/;

function validate(name: string, port: string, memory: string): string | null {
  if (!NAME_RE.test(name)) {
    return "Name must start with a letter or number, then letters, numbers, dots, underscores, or hyphens.";
  }
  const portNum = Number(port);
  if (!Number.isInteger(portNum) || portNum < 1 || portNum > 65535) {
    return "Port must be a number between 1 and 65535.";
  }
  if (memory.trim() !== "" && !MEMORY_RE.test(memory.trim())) {
    return 'Memory must be a positive number plus a unit, e.g. "2g" or "512m".';
  }
  return null;
}

type Stage = { name: string; state: StreamEvent["state"]; detail?: string };

export default function NewDeckForm({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => void;
}) {
  const [name, setName] = useState("");
  const [port, setPort] = useState("");
  const [image, setImage] = useState("");
  const [memory, setMemory] = useState("");
  const [prefilled, setPrefilled] = useState(false);

  const [stages, setStages] = useState<Stage[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  const validationError = prefilled ? validate(name, port, memory) : null;

  useEffect(() => {
    invoke<AddSuggestion>("suggest_new_deck_defaults")
      .then((s) => {
        setName(s.name);
        setPort(s.webUiPort);
      })
      .finally(() => setPrefilled(true));
  }, []);

  const unlisten = useRef<() => void>(() => {});
  useEffect(() => {
    listen<StreamEvent>("add-progress", (evt) => {
      const { stage, state, detail } = evt.payload;
      if (stage === "complete") return;
      setStages((prev) => {
        const idx = prev.findIndex((s) => s.name === stage);
        const next = { name: stage, state, detail };
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

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const err = validate(name, port, memory);
    if (err) {
      setError(err);
      return;
    }
    setError(null);
    setSubmitting(true);
    setStages([]);
    try {
      await invoke<InstanceStatus>("add_instance", {
        name,
        port,
        image: image.trim() || null,
        memory: memory.trim() || null,
      });
      setSuccess(true);
      onCreated();
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
          <h2>New Deck</h2>
          <button className="ghost-button" onClick={onClose} disabled={submitting}>
            Close
          </button>
        </div>

        {success ? (
          <p>Deck "{name}" created.</p>
        ) : (
          <form className="new-deck-form" onSubmit={handleSubmit}>
            <label>
              Name
              <input value={name} onChange={(e) => setName(e.target.value)} disabled={submitting} />
            </label>
            <label>
              Port
              <input value={port} onChange={(e) => setPort(e.target.value)} disabled={submitting} />
            </label>
            <label>
              Image (optional)
              <input
                value={image}
                onChange={(e) => setImage(e.target.value)}
                disabled={submitting}
                placeholder="ghcr.io/omnideck-dev/omnideck:latest"
              />
            </label>
            <label>
              Memory (optional)
              <input
                value={memory}
                onChange={(e) => setMemory(e.target.value)}
                disabled={submitting}
                placeholder="2g"
              />
            </label>

            {(error || validationError) && (
              <p className="settings__error">{error ?? validationError}</p>
            )}

            {stages.length > 0 && (
              <ul className="stage-list">
                {stages.map((s) => (
                  <li key={s.name} className={`stage-list__item stage-list__item--${s.state}`}>
                    <span>{s.name.replace(/_/g, " ")}</span>
                    <span className="stage-list__state">{s.detail ?? s.state}</span>
                  </li>
                ))}
              </ul>
            )}

            <button className="primary-button" type="submit" disabled={submitting || !!validationError}>
              {submitting ? "Creating…" : "Create Deck"}
            </button>
          </form>
        )}
      </div>
    </div>
  );
}
