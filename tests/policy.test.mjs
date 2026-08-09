// Security-posture assertions for the dashboard's capability allowlist —
// ported from the sibling repo's tests/policy.test.mjs, keeping only the
// security-assertion half. The byte-for-byte-Electron-parity half is
// intentionally not ported: there's no Electron app here to diff against,
// and this repo isn't diffing against the sibling either (which is itself
// post-Electron) — see reference/desktop-hardening-migration-PLAN.md's
// "Explicitly NOT being ported".
//
// Onboarding used to run from a second, isolated "onboarding" window with
// its own capability grant — reverted to a single "main" window (React
// screen-swap instead of a window swap) after a real EGL/GPU-driver bug was
// suspected to be the two-window-at-startup pattern itself. That reversal
// alone didn't actually fix it (the bug reappeared in the next real-hardware
// test) — the working theory now is `bootstrap.rs`'s use of `tauri::ipc::
// Channel` and a `WebviewWindow` command parameter, both unique to this
// module and both since replaced with the same `app.emit()`/`listen()` +
// `AppHandle`-only pattern every other command in this app already uses
// successfully. See bootstrap.rs's doc comment and
// reference/desktop-hardening-migration-PLAN.md's "Decisions from review"
// for the full history. The onboarding-specific isolation tests that used
// to live here no longer apply — there's no second window or capability
// left to isolate — but the dashboard-bridge allowlist assertions below now
// also cover the 3 bootstrap commands folded into it.
//
// These assertions exist so a future PR can't silently widen the attack
// surface (a broader capability grant, a new command added to an allowlist
// without review, a command handler skipping the window-origin check) without
// a test failing.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const packageJson = JSON.parse(await read("../package.json"));
const tauriConf = JSON.parse(await read("../src-tauri/tauri.conf.json"));
const dashboardCapability = JSON.parse(await read("../src-tauri/capabilities/default.json"));
const dashboardPermission = await read("../src-tauri/permissions/dashboard-bridge.toml");
const vendor = JSON.parse(await read("../src-tauri/binaries/vendor-manifest.json"));
const libRust = await read("../src-tauri/src/lib.rs");
const bootstrapRust = await read("../src-tauri/src/bootstrap.rs");
const cliBridgeRust = await read("../src-tauri/src/cli_bridge.rs");
const useBootstrapTs = await read("../src/hooks/useBootstrap.ts");

test("bundles exactly one target-qualified logical sidecar", () => {
  assert.deepEqual(tauriConf.bundle.externalBin, ["binaries/omnideck"]);
  assert.equal(tauriConf.identifier, "dev.omnideck.desktop");
  assert.equal(tauriConf.productName, "Omnideck");
  assert.deepEqual(tauriConf.bundle.icon, [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.icns",
    "icons/icon.ico",
  ]);
});

test("there is exactly one window, and no withGlobalTauri escape hatch", () => {
  // withGlobalTauri existed only for the old vanilla-JS onboarding window
  // (window.__TAURI__, no bundler). The React app imports @tauri-apps/api
  // properly, so re-adding it would just widen what any loaded content can
  // reach for no reason.
  assert.deepEqual(
    tauriConf.app.windows.map((w) => w.label),
    ["main"],
  );
  assert.equal(tauriConf.app.withGlobalTauri, undefined);
});

test("dashboard capability is an enumerated allowlist, not core:default", () => {
  assert.deepEqual(dashboardCapability.windows, ["main"]);
  assert.ok(dashboardCapability.permissions.includes("dashboard-bridge"));
  assert.equal(dashboardCapability.permissions.includes("core:default"), false);
  assert.doesNotMatch(
    JSON.stringify(dashboardCapability),
    /shell:|process:|fs:|updater:|dialog:/i,
  );
});

test("dashboard permission's command list matches lib.rs's invoke_handler exactly", () => {
  const declared = [...dashboardPermission.matchAll(/^\s*"([a-z_]+)",?$/gm)].map((m) => m[1]);
  const dashboardCommands = [
    "list_instances",
    "cli_version_contract",
    "instance_status",
    "instance_logs",
    "start_instance",
    "stop_instance",
    "restart_instance",
    "update_instance",
    "instance_doctor",
    "instance_config",
    "suggest_new_deck_defaults",
    "add_instance",
    "remove_instance",
    "bootstrap",
    "begin_setup",
    "run_action",
  ];
  assert.deepEqual(declared.sort(), [...dashboardCommands].sort());
  for (const command of dashboardCommands) {
    assert.match(
      libRust,
      new RegExp(`(commands::${command}|bootstrap::${command})\\b`),
      `${command} must be registered in lib.rs's invoke_handler!`,
    );
  }
  // "open_dashboard" was the isolated onboarding window's window-swap
  // command — there's nothing left for it to do once there's only one
  // window, so it was removed rather than kept as a no-op.
  assert.doesNotMatch(dashboardPermission, /open_dashboard/);
  assert.doesNotMatch(libRust, /open_dashboard/);
});

test("the frontend only calls bootstrap's three commands, never a shell/process API", () => {
  const invoked = [...useBootstrapTs.matchAll(/invoke\(\s*"([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual([...new Set(invoked)].sort(), ["begin_setup", "bootstrap", "run_action"]);
  assert.doesNotMatch(useBootstrapTs, /plugin-shell|Command\.sidecar|executable|argv|workingDirectory/);
});

test("bootstrap.rs's commands are AppHandle-only, matching every other command in this app", () => {
  for (const command of ["bootstrap", "begin_setup", "run_action"]) {
    const fn = bootstrapRust.match(new RegExp(`pub (?:async )?fn ${command}\\([\\s\\S]*?\\n\\}`));
    assert.ok(fn, `expected to find ${command}()`);
    assert.doesNotMatch(fn[0], /WebviewWindow/, `${command} should not take a WebviewWindow parameter`);
  }
});

test("bootstrap.rs pushes state via app.emit, not a Channel", () => {
  assert.match(bootstrapRust, /const SETUP_STATE_EVENT: &str = "setup-state"/);
  assert.match(bootstrapRust, /app\.emit\(SETUP_STATE_EVENT/);
});

test("bootstrap.rs no longer manages window visibility or uses Channel/WebviewWindow", () => {
  // Two regressions this guards against, in order: (1) a second,
  // initially-hidden window created at startup, suspected of breaking
  // EGL/GPU-driver init on at least one real Intel/Mesa combination — fixed
  // by removing the second window; (2) that fix alone didn't hold on a
  // second real-hardware test, and `tauri::ipc::Channel` /
  // `WebviewWindow` turned out to be the only mechanisms unique to this
  // module versus the rest of the app's proven-working IPC patterns.
  // Asserting both are gone (not just unused) keeps either from creeping
  // back in as part of some future onboarding tweak. Doc comments (`//!`/
  // `///`) are excluded — they're allowed, and expected, to keep discussing
  // this history in prose; only actual code is checked.
  const code = bootstrapRust
    .split("\n")
    .filter((line) => !/^\s*(\/\/!|\/\/\/)/.test(line))
    .join("\n");
  for (const symbol of [
    "show_onboarding",
    "show_dashboard",
    "create_onboarding_window",
    "WebviewWindowBuilder",
    "WebviewUrl",
    "WebviewWindow",
    "ipc::Channel",
    "Channel<",
  ]) {
    assert.doesNotMatch(code, new RegExp(symbol), `${symbol} should not reappear in bootstrap.rs's code`);
  }
});

test("single-instance plugin is registered first and focuses the single main window", () => {
  const pluginOrder = [...libRust.matchAll(/\.plugin\((\w[\w:]*)/g)].map((m) => m[1]);
  assert.equal(pluginOrder[0], "tauri_plugin_single_instance::init");
  assert.match(libRust, /get_webview_window\("main"\)/);
  assert.doesNotMatch(libRust, /get_webview_window\("onboarding"\)/);
});

test("CLI sidecar is pinned by version + checksum for all six targets, floor-checked not exact-matched", () => {
  assert.equal(vendor.repository, "omnideck-dev/cli");
  assert.match(vendor.tag, /^v\d+\.\d+\.\d+$/);
  assert.equal(
    vendor.downloadBaseUrl,
    `https://github.com/omnideck-dev/cli/releases/download/${vendor.tag}`,
  );
  assert.deepEqual(
    vendor.targets.map(({ targetTriple }) => targetTriple).sort(),
    [
      "aarch64-apple-darwin",
      "aarch64-pc-windows-msvc",
      "aarch64-unknown-linux-gnu",
      "x86_64-apple-darwin",
      "x86_64-pc-windows-msvc",
      "x86_64-unknown-linux-gnu",
    ],
  );
  for (const target of vendor.targets) {
    assert.match(target.archiveSha256, /^[0-9a-f]{64}$/);
    assert.match(target.binarySha256, /^[0-9a-f]{64}$/);
  }
  assert.match(cliBridgeRust, /MINIMUM_CLI_VERSION: &str = "v0\.10\.0"/);
  assert.doesNotMatch(cliBridgeRust, /EXPECTED_CLI_VERSION|EXPECTED_CLI_COMMIT/);
  assert.match(cliBridgeRust, /EXPECTED_JSON_CONTRACT: u64 = 2/);
  assert.equal(packageJson.scripts["fetch:sidecars"], "node scripts/fetch-sidecars.mjs");
});

test("sidecar process output is bounded and every operation has a timeout", () => {
  assert.match(cliBridgeRust, /STDOUT_LIMIT: usize = 1_000_000/);
  assert.match(cliBridgeRust, /STDERR_LIMIT: usize = 256 \* 1024/);
  assert.match(cliBridgeRust, /INSPECTION_TIMEOUT: Duration = Duration::from_secs\(15\)/);
  assert.match(cliBridgeRust, /MUTATION_TIMEOUT: Duration = Duration::from_secs\(20 \* 60\)/);
  assert.match(cliBridgeRust, /fn append_bounded/);
  assert.match(cliBridgeRust, /struct LineBuffer/);
});
