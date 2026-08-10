// Security-posture assertions for the isolated onboarding window and the
// dashboard's capability allowlist — ported from the sibling repo's
// tests/policy.test.mjs, keeping only the security-assertion half. The
// byte-for-byte-Electron-parity half is intentionally not ported: there's
// no Electron app here to diff against, and this repo isn't diffing against
// the sibling either (which is itself post-Electron) — see
// reference/desktop-hardening-migration-PLAN.md's "Explicitly NOT being
// ported".
//
// History: this repo briefly went single-window (React screen-swap instead
// of a second "onboarding" window), suspecting the two-window pattern
// caused a real `EGL_BAD_PARAMETER` crash on some Linux hardware. It
// didn't — the actual cause was CI building the Linux AppImage on
// ubuntu-24.04 instead of a Fedora container matching this repo's own dev
// toolbox (see AGENT.md's "EGL_BAD_PARAMETER AppImage crash" section and
// `release.yml`'s `linux-x64` matrix entry). With that confirmed, this repo
// went back to the two-window design — closer to the sibling's own setup
// flow, which has since matured with real testing this file also now
// carries a portable slice of (the origin/URL check below, on top of what
// this repo already had before the brief single-window detour).
//
// These assertions exist so a future PR can't silently widen the attack
// surface (a broader capability grant, a new command added to an allowlist
// without review, `"main"` used where `"onboarding"` was meant) without a
// test failing.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const packageJson = JSON.parse(await read("../package.json"));
const tauriConf = JSON.parse(await read("../src-tauri/tauri.conf.json"));
const dashboardCapability = JSON.parse(await read("../src-tauri/capabilities/default.json"));
const onboardingCapability = JSON.parse(await read("../src-tauri/capabilities/onboarding.json"));
const dashboardPermission = await read("../src-tauri/permissions/dashboard-bridge.toml");
const onboardingPermission = await read("../src-tauri/permissions/onboarding-bridge.toml");
const vendor = JSON.parse(await read("../src-tauri/binaries/vendor-manifest.json"));
const libRust = await read("../src-tauri/src/lib.rs");
const bootstrapRust = await read("../src-tauri/src/bootstrap.rs");
const cliBridgeRust = await read("../src-tauri/src/cli_bridge.rs");
const hostAdapter = await read("../public/onboarding/host-adapter.js");

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

test("dashboard capability is an enumerated allowlist, not core:default", () => {
  assert.deepEqual(dashboardCapability.windows, ["main"]);
  assert.equal(dashboardCapability.windows.includes("onboarding"), false);
  assert.ok(dashboardCapability.permissions.includes("dashboard-bridge"));
  assert.equal(dashboardCapability.permissions.includes("core:default"), false);
  assert.doesNotMatch(
    JSON.stringify(dashboardCapability),
    /shell:|process:|fs:|updater:|dialog:/i,
  );
});

test("onboarding capability is local, scoped to \"onboarding\" only, and never \"main\"", () => {
  // The regression this guards against is specific and easy to reintroduce
  // by copy-paste from the sibling app: there, "main" *is* the setup
  // window, so its capability correctly says "main". Here "main" is the
  // dashboard — if this capability ever says "main" instead of
  // "onboarding", the bridge silently grants the wrong window instead of
  // the isolated one, or grants nothing at all.
  assert.equal(onboardingCapability.local, true);
  assert.deepEqual(onboardingCapability.windows, ["onboarding"]);
  assert.equal(onboardingCapability.windows.includes("main"), false);
  // Check the permissions grant specifically, not the whole file — the
  // capability's own description text legitimately mentions "core:"/
  // "opener:" while explaining their absence.
  assert.deepEqual(onboardingCapability.permissions, ["onboarding-bridge"]);
});

test("onboarding permission exposes only the four typed lifecycle commands", () => {
  assert.match(
    onboardingPermission,
    /commands\.allow = \["bootstrap", "begin_setup", "open_dashboard", "run_action"\]/,
  );
  assert.doesNotMatch(onboardingPermission, /spawn|execute|shell|filesystem|process/i);
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
  ];
  assert.deepEqual(declared.sort(), [...dashboardCommands].sort());
  for (const command of dashboardCommands) {
    assert.match(
      libRust,
      new RegExp(`commands::${command}\\b`),
      `${command} must be registered in lib.rs's invoke_handler!`,
    );
  }
});

test("onboarding's IPC surface only calls its four allowed commands", () => {
  const invoked = [...hostAdapter.matchAll(/run\("([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual([...new Set(invoked)].sort(), [
    "begin_setup",
    "bootstrap",
    "open_dashboard",
    "run_action",
  ]);
  assert.doesNotMatch(hostAdapter, /plugin-shell|Command\.sidecar|executable|argv|workingDirectory/);
});

test("every onboarding command authorizes window.label() == \"onboarding\", never \"main\"", () => {
  assert.match(bootstrapRust, /fn authorize_onboarding\(window: &WebviewWindow\)/);
  assert.match(bootstrapRust, /window\.label\(\) != "onboarding"/);
  // The exact bug this repo's own review caught: copying the sibling's
  // `window.label() != "main"` literally would authorize the wrong window,
  // since "main" is the dashboard here.
  assert.doesNotMatch(bootstrapRust, /window\.label\(\) != "main"/);
});

test("authorization also checks the window's actual URL, not just its label", () => {
  // Ported from the sibling's `authorize_local_setup`/`is_local_setup_url`
  // (`lib.rs`): a label alone isn't quite enough in principle — a
  // compromised or buggy navigation could point the "onboarding" window at
  // something else first. This is real defense-in-depth, not redundant
  // with the label check above (which guards a different mistake: reusing
  // this bridge from the wrong window entirely).
  assert.match(bootstrapRust, /fn is_local_setup_url\(url: &tauri::Url\) -> bool/);
  assert.match(bootstrapRust, /\("tauri", Some\("localhost"\)\)/);
  assert.match(bootstrapRust, /!is_local_setup_url\(&url\)/);
});

test("bootstrap only reveals the onboarding window when setup is actually needed", () => {
  // A narrow, deliberately fragile regex: extracts the `Ok(status) if
  // status.ready` match arm's body and asserts it does NOT call
  // show_onboarding. This is the exact shape of a real regression this
  // session found and fixed — bootstrap() unconditionally showing
  // onboarding on every launch, even when the runtime was already ready.
  const readyArm = bootstrapRust.match(
    /Ok\(status\) if status\.ready => \{([\s\S]*?)\}\n\s*Ok\(_\)/,
  );
  assert.ok(readyArm, "expected to find bootstrap()'s ready match arm");
  assert.doesNotMatch(readyArm[1], /show_onboarding/);
});

test("the onboarding window is created hidden by default", () => {
  assert.match(bootstrapRust, /"onboarding",\s*\n\s*WebviewUrl::App\("onboarding\/index\.html"\.into\(\)\)/);
  assert.match(bootstrapRust, /\.visible\(false\)/);
});

test("single-instance plugin is registered first and picks onboarding over main when visible", () => {
  const pluginOrder = [...libRust.matchAll(/\.plugin\((\w[\w:]*)/g)].map((m) => m[1]);
  assert.equal(pluginOrder[0], "tauri_plugin_single_instance::init");
  assert.match(libRust, /get_webview_window\("onboarding"\)/);
  assert.match(libRust, /filter\(\|window\| window\.is_visible\(\)\.unwrap_or\(false\)\)/);
});

test("CLI sidecar is pinned by version + checksum for all six targets, floor-checked not exact-matched", () => {
  assert.equal(vendor.repository, "omnideck-dev/cli");
  assert.match(vendor.tag, /^v\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/);
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
  assert.match(cliBridgeRust, /MINIMUM_CLI_VERSION: &str = "v0\.11\.0-alpha\.2"/);
  assert.doesNotMatch(cliBridgeRust, /EXPECTED_CLI_VERSION|EXPECTED_CLI_COMMIT/);
  assert.match(cliBridgeRust, /EXPECTED_JSON_CONTRACT: u64 = 3/);
  assert.equal(packageJson.scripts["fetch:sidecars"], "node scripts/fetch-sidecars.mjs");
});

test("bootstrap.rs handles the full contract-3 runtime-setup vocabulary", () => {
  // New in CLI contract 3 (v0.11.0-alpha.2): `substage`/`status` fields and
  // a `"permission"` state on `runtime ensure`'s NDJSON events, plus 4 new
  // error codes. Asserted here (not just in cargo test) so a future CLI
  // bump can't silently drop this coverage without a policy-test failure
  // too.
  assert.match(cliBridgeRust, /pub substage: Option<String>/);
  assert.match(cliBridgeRust, /pub status: Option<String>/);
  assert.match(bootstrapRust, /pub awaiting_permission: bool/);
  for (const code of [
    "PERMISSION_CANCELLED",
    "WINDOWS_FEATURES_FAILED",
    "PACKAGE_INDEX_FAILED",
    "INSTALLER_FAILED",
  ]) {
    assert.match(bootstrapRust, new RegExp(`"${code}"`), `error_state must handle ${code}`);
  }
});

test("sidecar process output is bounded and every operation has a timeout", () => {
  assert.match(cliBridgeRust, /STDOUT_LIMIT: usize = 1_000_000/);
  assert.match(cliBridgeRust, /STDERR_LIMIT: usize = 256 \* 1024/);
  assert.match(cliBridgeRust, /INSPECTION_TIMEOUT: Duration = Duration::from_secs\(15\)/);
  assert.match(cliBridgeRust, /MUTATION_TIMEOUT: Duration = Duration::from_secs\(20 \* 60\)/);
  assert.match(cliBridgeRust, /fn append_bounded/);
  assert.match(cliBridgeRust, /struct LineBuffer/);
});

test("both Linux AppImage build paths strip the known half-bundled libraries", async () => {
  // Regression guard for a real cross-distro crash: linuxdeploy bundles
  // libgcrypt.so.20 but not its version-locked pair libgpg-error.so.0 (the
  // latter is on linuxdeploy's own exclude list, the former isn't), so a
  // Fedora-built AppImage's bundled libgcrypt loaded against Ubuntu's older
  // libgpg-error at runtime and crashed with a missing symbol version. See
  // scripts/strip-unsafe-appimage-libs.sh and AGENT.md's AppImage issues
  // list for the full account.
  assert.match(packageJson.scripts["build:linux"], /strip-unsafe-appimage-libs\.sh/);
  const buildAppimageScript = await read("../scripts/build-appimage.sh");
  assert.match(buildAppimageScript, /strip-unsafe-appimage-libs\.sh/);
  const stripScript = await read("../scripts/strip-unsafe-appimage-libs.sh");
  assert.match(stripScript, /libgcrypt\.so/);
});
