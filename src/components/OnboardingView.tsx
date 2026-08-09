import { useState } from "react";
import type { SetupState } from "../types/setup";

const DIAGNOSTIC_ICONS: Record<string, string> = { pass: "✓", issue: "!", waiting: "·" };
const STAGE_EYEBROWS: Record<string, string> = {
  welcome: "WELCOME",
  preparing: "SETTING UP",
  ready: "READY",
  error: "SETUP NEEDS ATTENTION",
};
// Stages where a wait is actually happening, so the note about it being
// one-time is true — not the opening splash, which a returning user (whose
// runtime is already ready) never even sees (App.tsx skips straight to the
// dashboard for that case).
const FOOTNOTE_STAGES = new Set(["preparing"]);
// Stages showing an outcome rather than work in progress — a spinner is
// redundant (or wrong) on any of these even when no progress bar is shown.
const SETTLED_STAGES = new Set(["welcome", "ready", "error"]);

interface OnboardingViewProps {
  state: SetupState;
  actionError: string | null;
  actionPending: boolean;
  onBeginSetup: () => void;
  onRunAction: (action: string) => void;
  /** Purely local — dismisses this screen in favor of the dashboard. Never
   * a Tauri command: there's no window to show/hide anymore, so "Continue"
   * is just App.tsx swapping which component it renders. */
  onContinue: () => void;
}

/** Ported from the sibling repo's web/setup.js render(state) function (by
 * way of this repo's earlier vanilla-JS `public/onboarding/setup.js`) —
 * same state → DOM mapping, now state → JSX. One render pass fully
 * re-derives what's shown from each pushed SetupState; no separate
 * imperative "now show screen X" calls scattered through the flow. */
export default function OnboardingView({
  state,
  actionError,
  actionPending,
  onBeginSetup,
  onRunAction,
  onContinue,
}: OnboardingViewProps) {
  const [technicalOpen, setTechnicalOpen] = useState(false);

  function runPrimaryAction() {
    if (state.primaryAction) return onRunAction(state.primaryAction);
    if (state.canOpen) return onContinue();
    return onBeginSetup();
  }

  const primaryVisible = state.canStart || state.canRetry || state.canOpen || Boolean(state.primaryAction);
  const primaryLabel =
    state.primaryLabel || (state.canOpen ? "Continue" : state.canRetry ? "Try again" : "Set up Omnideck");

  const hasProgress = typeof state.progress === "number";
  const hasIndeterminateProgress = state.indeterminate;
  const progressVisible = hasProgress || hasIndeterminateProgress;
  const spinnerVisible = !progressVisible && !SETTLED_STAGES.has(state.stage);

  const diagnostics = state.stage === "error" ? (state.diagnostics ?? []) : [];
  const diagnosticsVisible = diagnostics.length > 0;

  return (
    <div className="onboarding-screen" data-stage={state.stage} aria-live="polite">
      <div className="onboarding-screen__panel">
        <div className="onboarding-screen__identity">
          <div className="onboarding-mark" aria-hidden="true">
            <span />
            <span />
            <span />
          </div>
          <p className="onboarding-brand">omnideck</p>
        </div>

        <div className="onboarding-screen__status">
          <p className="eyebrow">{STAGE_EYEBROWS[state.stage] ?? "OMNIDECK"}</p>
          <h1>{state.title}</h1>
          <p className="onboarding-detail">{state.detail}</p>

          {state.activity && <p className="onboarding-activity">{state.activity}</p>}

          {diagnosticsVisible && (
            <section className="onboarding-diagnostics">
              <div className="onboarding-diagnostics__heading">
                <span>DIAGNOSTICS</span>
              </div>
              <div className="onboarding-diagnostic-list">
                {diagnostics.map((diagnostic) => (
                  <div
                    key={diagnostic.id}
                    className="onboarding-diagnostic-row"
                    data-status={diagnostic.status}
                  >
                    <span className="onboarding-diagnostic-icon">
                      {DIAGNOSTIC_ICONS[diagnostic.status] ?? "–"}
                    </span>
                    <span>{diagnostic.label}</span>
                  </div>
                ))}
              </div>
              <details open={technicalOpen} onToggle={(event) => setTechnicalOpen(event.currentTarget.open)}>
                <summary>Technical details</summary>
                <pre>{state.technical || "No further detail available."}</pre>
              </details>
            </section>
          )}

          {progressVisible && (
            <div className={`onboarding-progress-wrap${hasIndeterminateProgress ? " is-indeterminate" : ""}`}>
              <div
                className="onboarding-progress-track"
                role="progressbar"
                aria-label="Setup progress"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={hasProgress ? Math.round((state.progress ?? 0) * 100) : undefined}
                aria-valuetext={!hasProgress && hasIndeterminateProgress ? "In progress" : undefined}
              >
                <div
                  className="onboarding-progress"
                  style={hasProgress ? { width: `${Math.round((state.progress ?? 0) * 100)}%` } : undefined}
                />
              </div>
            </div>
          )}

          {spinnerVisible && <div className="spinner" aria-label="Working" />}

          {primaryVisible && (
            <button
              type="button"
              className="onboarding-primary primary-button"
              disabled={actionPending}
              onClick={runPrimaryAction}
            >
              {primaryLabel}
            </button>
          )}
          {state.secondaryAction && (
            <button
              type="button"
              className="onboarding-secondary ghost-button"
              disabled={actionPending}
              onClick={() => onRunAction(state.secondaryAction as string)}
            >
              {state.secondaryLabel}
            </button>
          )}
          {actionError && (
            <p className="onboarding-action-error" role="alert">
              {actionError}
            </p>
          )}
        </div>

        {FOOTNOTE_STAGES.has(state.stage) && (
          <p className="onboarding-footnote">This only needs to happen once.</p>
        )}
      </div>
    </div>
  );
}
