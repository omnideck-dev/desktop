import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CliError, ListEntry } from "../types/cli";

const POLL_INTERVAL_MS = 2500;

export interface InstancesState {
  /** null until the first call (success or failure) resolves. */
  entries: ListEntry[] | null;
  /** True only for the very first in-flight fetch — background refreshes
   * never flip this back on, so the table doesn't flicker/reset scroll. */
  loading: boolean;
  /** The most recent poll's error, if any. Previous `entries` are kept
   * around rather than cleared, so a transient engine hiccup doesn't blank
   * a dashboard that was showing real data a moment ago. */
  error: CliError | null;
  /** Re-runs the list fetch immediately, outside the poll interval — call
   * after a lifecycle action resolves so the row reflects the new state
   * without waiting up to POLL_INTERVAL_MS. */
  refresh: () => void;
}

export function useInstances(): InstancesState {
  const [state, setState] = useState<Omit<InstancesState, "refresh">>({
    entries: null,
    loading: true,
    error: null,
  });
  const mounted = useRef(true);
  const pollRef = useRef<() => Promise<void>>(async () => {});

  useEffect(() => {
    mounted.current = true;

    async function poll() {
      try {
        const entries = await invoke<ListEntry[]>("list_instances");
        if (mounted.current) {
          setState({ entries, loading: false, error: null });
        }
      } catch (error) {
        if (mounted.current) {
          setState((prev) => ({
            entries: prev.entries,
            loading: false,
            error: error as CliError,
          }));
        }
      }
    }

    pollRef.current = poll;
    poll();
    const id = setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, []);

  const refresh = useCallback(() => {
    pollRef.current();
  }, []);

  return { ...state, refresh };
}
