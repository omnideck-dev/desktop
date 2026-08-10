// Thin adapter between the Tauri IPC bridge and setup.js's render logic —
// ported from the sibling repo's web/host-adapter.js. Keeping this
// separation means setup.js's DOM/render code can be tested (or read)
// without needing a real Tauri runtime; only this file touches
// `window.__TAURI__`.
(() => {
  let listener = null;
  let running = false;

  function core() {
    const value = window.__TAURI__?.core;
    if (!value?.invoke || !value?.Channel) {
      throw new Error("The Tauri host bridge is unavailable.");
    }
    return value;
  }

  function stateChannel() {
    const channel = new (core().Channel)();
    channel.onmessage = (state) => listener?.(state);
    return channel;
  }

  async function run(command, args = {}) {
    if (running) return;
    running = true;
    try {
      return await core().invoke(command, args);
    } finally {
      running = false;
    }
  }

  function beginSetup() {
    return run("begin_setup", { onEvent: stateChannel() });
  }

  window.omnideckHost = Object.freeze({
    beginSetup,
    retry: beginSetup,
    openDashboard: () => run("open_dashboard"),
    runAction: (action) => run("run_action", { action }),
    onState(callback) {
      listener = callback;
      return () => {
        if (listener === callback) listener = null;
      };
    },
  });

  // A rejected *automatic* bootstrap() has no click handler wrapping it
  // (unlike setup.js's primary/secondary buttons, which catch their own
  // errors) — without this, a CLI-spawn failure at load would leave the
  // user staring at index.html's neutral "Starting Omnideck…" copy
  // forever, with the real error only visible in devtools. Reuses the
  // existing action-error affordance rather than adding a new UI state.
  // Ported from the sibling's `reportBootstrapFailure`.
  function reportBootstrapFailure(error) {
    const actionError = document.getElementById("action-error");
    if (!actionError) return;
    actionError.textContent = String(error?.message || error);
    actionError.hidden = false;
  }

  async function bootstrap() {
    try {
      await run("bootstrap", { onEvent: stateChannel() });
    } catch (error) {
      reportBootstrapFailure(error);
    }
  }

  window.addEventListener(
    "DOMContentLoaded",
    () => {
      setTimeout(() => void bootstrap(), 0);
    },
    { once: true },
  );
})();
