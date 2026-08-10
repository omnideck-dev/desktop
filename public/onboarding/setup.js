// Render logic for the onboarding window — ported from the sibling repo's
// web/setup.js, adapted to this repo's smaller SetupState shape (no
// setupReason/update/resume/repair distinctions, since there's no resume
// record here — see bootstrap.rs's doc comment for why). One render(state)
// function fully re-derives the DOM from each pushed state; no separate
// imperative "now show screen X" calls scattered through the flow.
const title = document.getElementById("title");
const detail = document.getElementById("detail");
const activity = document.getElementById("activity");
const status = document.getElementById("status");
const eyebrow = document.getElementById("eyebrow");
const primary = document.getElementById("primary");
const secondary = document.getElementById("secondary");
const spinner = document.getElementById("spinner");
const progressWrap = document.getElementById("progress-wrap");
const progressTrack = progressWrap.querySelector('[role="progressbar"]');
const progress = document.getElementById("progress");
const footnote = document.getElementById("footnote");
const diagnosticsPanel = document.getElementById("diagnostics");
const technicalDetails = diagnosticsPanel.querySelector("details");
const diagnosticList = document.getElementById("diagnostic-list");
const technicalOutput = document.getElementById("technical-output");
const actionError = document.getElementById("action-error");

let currentState = { stage: "welcome" };

// This window runs from the filesystem under its own capability, a
// different origin/surface from the dashboard — it can't read the
// dashboard's persisted theme choice, only the OS preference.
const darkQuery = window.matchMedia?.("(prefers-color-scheme: dark)");

function applyTheme() {
  document.documentElement.dataset.theme = darkQuery?.matches ? "dark" : "light";
}

applyTheme();
darkQuery?.addEventListener?.("change", applyTheme);

const DIAGNOSTIC_ICONS = { pass: "✓", issue: "!", waiting: "·" };
const STAGE_EYEBROWS = {
  welcome: "WELCOME",
  preparing: "SETTING UP",
  ready: "READY",
  error: "SETUP NEEDS ATTENTION",
};
// A native OS permission prompt is a different kind of wait than background
// work — "SETTING UP" would be a lie while the computer is actually idle,
// waiting on the user. Matches the sibling's setup-UX principle: keep native
// prompts visible, use truthful "waiting for you" copy, never a synthesized
// percentage (see render()'s progress handling below for the second half of
// that — awaitingPermission also forces the indeterminate spinner state).
const AWAITING_PERMISSION_EYEBROW = "WAITING FOR YOU";
// Stages where a wait is actually happening, so the note about it being
// one-time is true — not the opening splash, which a returning user (whose
// runtime is already ready) passes through in an instant.
const FOOTNOTE_STAGES = ["preparing"];
// Stages showing an outcome rather than work in progress — the spinner is
// redundant (or wrong) on any of these even if progress isn't shown either.
const SETTLED_STAGES = ["welcome", "ready", "error"];

function renderDiagnostics(state) {
  const diagnostics = Array.isArray(state.diagnostics) ? state.diagnostics : [];
  // Shown only on failure — during setup the activity line and progress bar
  // already say where things are; a list of phase names only invites "what
  // is that."
  const failed = state.stage === "error";
  diagnosticsPanel.hidden = !failed || diagnostics.length === 0;
  diagnosticList.replaceChildren();
  if (diagnosticsPanel.hidden) return;

  technicalDetails.open = false;
  technicalOutput.textContent = state.technical || "No further detail available.";
  diagnosticList.replaceChildren(
    ...diagnostics.map((diagnostic) => {
      const row = document.createElement("div");
      row.className = "diagnostic-row";
      row.dataset.status = diagnostic.status;

      const icon = document.createElement("span");
      icon.className = "diagnostic-icon";
      icon.textContent = DIAGNOSTIC_ICONS[diagnostic.status] || "–";
      const label = document.createElement("span");
      label.textContent = diagnostic.label;
      row.append(icon, label);
      return row;
    }),
  );
}

function render(state) {
  currentState = state;
  document.documentElement.dataset.stage = state.stage;
  document.documentElement.dataset.awaitingPermission = String(Boolean(state.awaitingPermission));
  title.textContent = state.title;
  detail.textContent = state.detail;
  eyebrow.textContent = state.awaitingPermission
    ? AWAITING_PERMISSION_EYEBROW
    : STAGE_EYEBROWS[state.stage] || "OMNIDECK";
  activity.textContent = state.activity || "";
  activity.hidden = !state.activity;
  status.textContent = state.status || "";
  status.hidden = !state.status;

  primary.hidden = !(state.canStart || state.canRetry || state.canOpen || state.primaryAction);
  primary.textContent =
    state.primaryLabel ||
    (state.canOpen ? "Continue" : state.canRetry ? "Try again" : "Set up Omnideck");
  primary.disabled = false;
  actionError.hidden = true;

  secondary.hidden = !state.secondaryAction;
  secondary.textContent = state.secondaryLabel || "";
  renderDiagnostics(state);

  const hasProgress = Number.isFinite(state.progress);
  const hasIndeterminateProgress = Boolean(state.indeterminate);
  progressWrap.hidden = !(hasProgress || hasIndeterminateProgress);
  progressWrap.classList.toggle("is-indeterminate", hasIndeterminateProgress);
  progress.style.width = hasProgress ? `${Math.round(state.progress * 100)}%` : "";
  if (hasProgress) {
    progressTrack.setAttribute("aria-valuenow", String(Math.round(state.progress * 100)));
    progressTrack.removeAttribute("aria-valuetext");
  } else {
    progressTrack.removeAttribute("aria-valuenow");
    if (hasIndeterminateProgress) progressTrack.setAttribute("aria-valuetext", "In progress");
    else progressTrack.removeAttribute("aria-valuetext");
  }
  spinner.hidden = !progressWrap.hidden || SETTLED_STAGES.includes(state.stage);
  footnote.hidden = !FOOTNOTE_STAGES.includes(state.stage);
}

function runPrimaryAction() {
  if (currentState.primaryAction) {
    return window.omnideckHost.runAction(currentState.primaryAction);
  }
  if (currentState.canOpen) return window.omnideckHost.openDashboard();
  if (currentState.canRetry) return window.omnideckHost.retry();
  return window.omnideckHost.beginSetup();
}

function reportActionFailure(error) {
  actionError.textContent = String(error?.message || error);
  actionError.hidden = false;
}

// Re-enabling in a `finally` matters: the button is only otherwise
// re-enabled by the next state push, and a rejected action doesn't always
// produce one — that would leave the only control on the screen dead with
// nothing explaining why.
primary.addEventListener("click", async () => {
  primary.disabled = true;
  actionError.hidden = true;
  try {
    await runPrimaryAction();
  } catch (error) {
    reportActionFailure(error);
  } finally {
    primary.disabled = false;
  }
});

secondary.addEventListener("click", async () => {
  if (!currentState.secondaryAction) return;
  secondary.disabled = true;
  actionError.hidden = true;
  try {
    await window.omnideckHost.runAction(currentState.secondaryAction);
  } catch (error) {
    reportActionFailure(error);
  } finally {
    secondary.disabled = false;
  }
});

window.omnideckHost.onState(render);
