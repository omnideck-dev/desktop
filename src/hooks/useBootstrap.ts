import { useCallback, useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import type { SetupState } from "../types/setup";

export interface BootstrapController {
  state: SetupState | null;
  /** True once the initial `bootstrap` check has resolved and found the
   * shared runtime already ready — lets App.tsx skip the onboarding screen
   * entirely on a normal day-to-day launch. Distinct from `state.canOpen`:
   * that's also true after `begin_setup` finishes mid-flow, where a
   * "Continue" click is wanted (see OnboardingView) rather than an
   * automatic skip. */
  initiallyReady: boolean;
  actionError: string | null;
  actionPending: boolean;
  beginSetup: () => void;
  runAction: (action: string) => void;
}

function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message);
  }
  return String(error);
}

/** Drives the shared-runtime bootstrap flow (bootstrap.rs) — the same 3
 * commands (`bootstrap`/`begin_setup`/`run_action`) an earlier, isolated
 * onboarding window used, now called from the dashboard's own "main"
 * window (see bootstrap.rs's doc comment for why there's no window to
 * show/hide anymore). `enabled` gates the initial `bootstrap` call so it
 * only fires once useCliVersion has confirmed the CLI itself is reachable
 * — a broken CLI already produces its own, more specific BlockingScreen;
 * this hook is only meaningful once talking to the CLI already works. */
export function useBootstrap(enabled: boolean): BootstrapController {
  const [state, setState] = useState<SetupState | null>(null);
  const [initiallyReady, setInitiallyReady] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionPending, setActionPending] = useState(false);
  const startedRef = useRef(false);

  const invokeWithChannel = useCallback(
    async (command: string, onFirst?: (first: SetupState) => void) => {
      const channel = new Channel<SetupState>();
      let seenFirst = false;
      channel.onmessage = (next) => {
        setState(next);
        if (!seenFirst) {
          seenFirst = true;
          onFirst?.(next);
        }
      };
      await invoke(command, { onEvent: channel });
    },
    [],
  );

  useEffect(() => {
    if (!enabled || startedRef.current) return;
    startedRef.current = true;
    void invokeWithChannel("bootstrap", (first) => {
      if (first.stage === "ready") setInitiallyReady(true);
    }).catch((error) => setActionError(errorMessage(error)));
  }, [enabled, invokeWithChannel]);

  const beginSetup = useCallback(() => {
    setActionPending(true);
    setActionError(null);
    invokeWithChannel("begin_setup")
      .catch((error) => setActionError(errorMessage(error)))
      .finally(() => setActionPending(false));
  }, [invokeWithChannel]);

  const runAction = useCallback((action: string) => {
    setActionPending(true);
    setActionError(null);
    invoke("run_action", { action })
      .catch((error) => setActionError(errorMessage(error)))
      .finally(() => setActionPending(false));
  }, []);

  return { state, initiallyReady, actionError, actionPending, beginSetup, runAction };
}
