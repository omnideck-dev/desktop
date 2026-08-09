import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SetupState } from "../types/setup";

const SETUP_STATE_EVENT = "setup-state";

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
 * show/hide anymore).
 *
 * Listens for `"setup-state"` events the same way `NewDeckForm.tsx` listens
 * for `"add-progress"` — not a `Channel`, which this hook used to use.
 * That was a real, unique-to-this-hook mechanism that turned out to be a
 * plausible cause of a startup crash on some hardware (see bootstrap.rs's
 * doc comment); this now matches the proven pattern everywhere else in the
 * app instead.
 *
 * `enabled` gates the initial `bootstrap` call so it only fires once
 * useCliVersion has confirmed the CLI itself is reachable — a broken CLI
 * already produces its own, more specific BlockingScreen; this hook is only
 * meaningful once talking to the CLI already works. */
export function useBootstrap(enabled: boolean): BootstrapController {
  const [state, setState] = useState<SetupState | null>(null);
  const [initiallyReady, setInitiallyReady] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionPending, setActionPending] = useState(false);
  const startedRef = useRef(false);
  // True until the first "setup-state" event arrives, so that event (and
  // only that one) can be used to decide `initiallyReady` — later events
  // (from begin_setup mid-flow) must not retroactively flip it.
  const awaitingInitialRef = useRef(true);
  // `listen()` is async — resolves once the event subscription is actually
  // registered on the Tauri side. Calling `invoke("bootstrap")` before that
  // resolves risks losing the very first "setup-state" event to a race, so
  // the mount effect below awaits this before invoking.
  const listenerReadyRef = useRef<Promise<UnlistenFn> | null>(null);

  useEffect(() => {
    let cancelled = false;
    const promise = listen<SetupState>(SETUP_STATE_EVENT, (event) => {
      setState(event.payload);
      if (awaitingInitialRef.current) {
        awaitingInitialRef.current = false;
        if (event.payload.stage === "ready") setInitiallyReady(true);
      }
    });
    listenerReadyRef.current = promise;
    return () => {
      cancelled = true;
      void promise.then((fn) => {
        if (!cancelled) fn();
      });
    };
  }, []);

  useEffect(() => {
    if (!enabled || startedRef.current) return;
    startedRef.current = true;
    void (listenerReadyRef.current ?? Promise.resolve())
      .then(() => invoke("bootstrap"))
      .catch((error) => setActionError(errorMessage(error)));
  }, [enabled]);

  const beginSetup = useCallback(() => {
    setActionPending(true);
    setActionError(null);
    invoke("begin_setup")
      .catch((error) => setActionError(errorMessage(error)))
      .finally(() => setActionPending(false));
  }, []);

  const runAction = useCallback((action: string) => {
    setActionPending(true);
    setActionError(null);
    invoke("run_action", { action })
      .catch((error) => setActionError(errorMessage(error)))
      .finally(() => setActionPending(false));
  }, []);

  return { state, initiallyReady, actionError, actionPending, beginSetup, runAction };
}
