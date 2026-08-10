// Runs the real public/onboarding/host-adapter.js inside Node's vm module
// against a fake window.__TAURI__ — proves the exact IPC invocation
// sequence without needing a real Tauri runtime. Ported from the sibling
// repo (`omnideck/desktop`)'s tests/host-adapter.test.mjs technique,
// adapted to this repo's simpler adapter: ours has no `reason`-based
// auto-resume (`bootstrap`'s pushed state decides what's shown; `begin_setup`
// only ever runs from an explicit user action in setup.js) and no return
// value from `bootstrap` to branch on — it's driven entirely by the
// `SetupState` pushed over the Channel, not by inspecting a command result.
// So the sibling's "resume bootstrap starts setup exactly once" test has no
// analog here; what does port is the shape: `bootstrap` runs exactly once
// on load, the `running` re-entrancy guard actually guards, and a rejected
// automatic bootstrap reports itself instead of leaving the screen stuck on
// index.html's neutral "Starting Omnideck…" copy forever (this test file
// is what caught that this repo's adapter was missing that entirely —
// see host-adapter.js's `reportBootstrapFailure`, ported from the
// sibling's, once this test proved the gap).
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const adapter = await readFile(new URL("../public/onboarding/host-adapter.js", import.meta.url), "utf8");

function harness({ invokeImpl } = {}) {
  const listeners = new Map();
  const invocations = [];
  const channels = [];
  const actionError = { hidden: true, textContent: "" };

  class Channel {
    constructor() {
      channels.push(this);
    }
  }

  const window = {
    __TAURI__: {
      core: {
        Channel,
        invoke: async (command, args) => {
          invocations.push({ command, hasChannel: Boolean(args?.onEvent) });
          if (invokeImpl) return invokeImpl(command, args);
          return undefined;
        },
      },
    },
    addEventListener(type, callback, options) {
      listeners.set(type, { callback, options });
    },
  };
  const context = vm.createContext({
    document: {
      getElementById: (id) => (id === "action-error" ? actionError : null),
    },
    // A real `setTimeout(fn, 0)` races Node's own event loop against this
    // test's `await` — the sibling's harness sidesteps that by making
    // setTimeout resolve on the microtask queue instead, same here.
    setTimeout: (callback) => queueMicrotask(callback),
    window,
  });
  vm.runInContext(adapter, context);
  return { actionError, channels, invocations, listeners, window };
}

async function flush() {
  await new Promise((resolve) => setImmediate(resolve));
}

test("bootstrap runs exactly once on DOMContentLoaded, with a state channel", async () => {
  const harnessResult = harness();
  const dom = harnessResult.listeners.get("DOMContentLoaded");
  assert.equal(dom.options?.once, true, "the listener must be registered { once: true }");
  dom.callback();
  await flush();
  assert.deepEqual(harnessResult.invocations, [{ command: "bootstrap", hasChannel: true }]);
});

test("a rejected automatic bootstrap reports itself instead of leaving the screen stuck", async () => {
  const harnessResult = harness({
    invokeImpl: async (command) => {
      if (command === "bootstrap") throw new Error("bridge unavailable");
    },
  });
  harnessResult.listeners.get("DOMContentLoaded").callback();
  await flush();
  assert.deepEqual(harnessResult.invocations, [{ command: "bootstrap", hasChannel: true }]);
  assert.equal(harnessResult.actionError.hidden, false);
  assert.equal(harnessResult.actionError.textContent, "bridge unavailable");

  // And the bridge isn't wedged — a later explicit call still runs.
  await harnessResult.window.omnideckHost.beginSetup();
  assert.deepEqual(harnessResult.invocations, [
    { command: "bootstrap", hasChannel: true },
    { command: "begin_setup", hasChannel: true },
  ]);
});

test("beginSetup and retry are the same operation, each with a fresh channel", async () => {
  const harnessResult = harness();
  await harnessResult.window.omnideckHost.beginSetup();
  await harnessResult.window.omnideckHost.retry();
  assert.deepEqual(harnessResult.invocations, [
    { command: "begin_setup", hasChannel: true },
    { command: "begin_setup", hasChannel: true },
  ]);
  // A distinct Channel per call — reusing one would let a stale onmessage
  // handler from a finished call keep firing into a new one.
  assert.equal(harnessResult.channels.length, 2);
  assert.notEqual(harnessResult.channels[0], harnessResult.channels[1]);
});

test("openDashboard and runAction invoke the right command with the right args", async () => {
  const harnessResult = harness();
  await harnessResult.window.omnideckHost.openDashboard();
  await harnessResult.window.omnideckHost.runAction("quit");
  assert.deepEqual(harnessResult.invocations, [
    { command: "open_dashboard", hasChannel: false },
    { command: "run_action", hasChannel: false },
  ]);
});

test("overlapping calls are ignored while one is already in flight", async () => {
  let release;
  const inFlight = new Promise((resolve) => {
    release = resolve;
  });
  const harnessResult = harness({
    invokeImpl: async (command) => {
      if (command === "begin_setup") await inFlight;
    },
  });
  const first = harnessResult.window.omnideckHost.beginSetup();
  const second = harnessResult.window.omnideckHost.runAction("retry");
  release();
  await Promise.all([first, second]);
  // The second call landed while `running` was still true from the first,
  // so it's silently dropped — only one invocation actually reached
  // `core().invoke`.
  assert.deepEqual(harnessResult.invocations, [{ command: "begin_setup", hasChannel: true }]);
});

test("onState delivers channel messages to the registered listener until unsubscribed", async () => {
  const harnessResult = harness();
  const received = [];
  const unsubscribe = harnessResult.window.omnideckHost.onState((state) => received.push(state));

  // Drive a real call so the adapter's own `stateChannel()` wires up
  // `channel.onmessage` — constructing a bare `Channel` by hand wouldn't
  // exercise that wiring at all.
  await harnessResult.window.omnideckHost.beginSetup();
  const [channel] = harnessResult.channels;
  channel.onmessage({ stage: "welcome" });
  unsubscribe();
  channel.onmessage({ stage: "ready" });

  assert.deepEqual(received, [{ stage: "welcome" }]);
});
