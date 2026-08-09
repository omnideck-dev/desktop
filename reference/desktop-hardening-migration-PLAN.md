# Hardening migration plan: pulling `omnideck/desktop` improvements into this repo

**Read first:** `AGENT.md`, `desktop_tauri_rewrite.md`. This doc assumes you already know this repo's
target architecture (multi-instance "Decks" dashboard, CLI-delegated per `desktop-cli-unification-ISSUE.md`).

## Why this doc exists

`omnideck/desktop` (path: `/var/home/ron/Code/omnideck/omnideck/desktop/`, a sibling repo) is a
*different, already-shipped* Tauri app — a 1:1 Electron replacement, single hardcoded instance, no
CLI-delegated multi-instance model. It predates the CLI-unification rewrite this repo implements and
will eventually be replaced by this repo per `reference/desktop-cli-unification-ISSUE.md`.

It is NOT the same codebase as this one and nothing here should be merged wholesale. But it has real,
shipped release/security engineering this repo doesn't have yet, because it went through actual release
cycles (currently `0.1.0-alpha.8`, with `TESTING.md`/`RELEASING.md`/CI release matrix) that this repo
hasn't reached. This plan extracts the specific patterns worth adopting here, adapted to this repo's
multi-instance architecture, and leaves behind everything that's specific to the old single-instance app
(hardcoded container/volume names, the Electron-parity copy/progress assertions in
`tests/policy.test.mjs`). See "Decisions from review" below — as of this pass, the sibling's
`bootstrap`/`begin_setup`/`open_app`/`run_action` 4-command IPC shape and its `parity.rs` state-machine
*and* its actual `web/setup.js`/`setup.css` UI are now a much closer port than earlier drafts of this doc
assumed, because this repo adopted the same isolated-onboarding-webview architecture.

Do NOT copy files verbatim across repos — port the *pattern*, adapted to this repo's `cli_bridge.rs` /
`commands.rs` split and its multi-instance data model. Every task below names the source pattern (in the
sibling repo) and the concrete target in this one.

Work top to bottom; each phase is independently shippable. Update this doc's checkboxes as you go —
don't let it go stale (same rule as `AGENT.md`).

## Decisions from review (2026-08-08)

A first critical pass over this doc against both repos' real code turned up a stale assumption and two
open architectural questions. Resolved as follows — later phases below have been updated to match, but
if anything downstream still contradicts these, this section wins:

- **CLI version pin: floor, not exact match.** The sibling pins `EXPECTED_CLI_VERSION`/`EXPECTED_CLI_COMMIT`
  to an exact string (`v0.10.0-alpha.2` at the time of writing) and checks equality. This repo instead
  checks `jsonContract` equality (already implemented, unchanged — that's the CLI's own documented
  breaking-change signal) plus a **minimum-version floor of `v0.10.0`** on the version string from
  `--version --json`. Rationale: the sidecar binary is always exactly what `vendor-manifest.json` pinned
  at build time (checksummed — that pin stays exact, see Phase 1), so the runtime self-check in
  `cli_bridge.rs` isn't guarding against a substituted CLI, just against a build mistake or corruption.
  A floor avoids hand-updating a hardcoded exact-match constant on every CLI patch release while still
  catching "older/untested CLI than what this build was verified against." `v0.10.0` specifically because
  it's the first CLI release with the finalized `JSON_MODE_SPEC.md` contract, `contracts/` JSON schemas,
  and `runtime`/`environment` commands — confirmed by checking the CLI repo directly: `v0.10.0-alpha.2`
  (which the sibling pins) predates `cmd/runtime.go`/`cmd/environment.go` existing at all, and `v0.10.0`
  stable was tagged only ~3 hours after the sibling's last commit. **Do not copy the sibling's `runtime
  ensure`/`environment ensure` call args verbatim** — re-verify them against the real `v0.10.0` tag in the
  CLI repo, since that command surface was still in flux when the sibling's code was written.
- **Onboarding is an isolated webview, not a React route.** Mirrors the sibling's actual security model
  (separate window, narrow capability grant, distinct from the dashboard's broad mutating command
  surface) rather than folding onboarding into the main SPA. Chosen deliberately over unifying: the
  dashboard here already needs a wide command surface (`add_instance`/`remove_instance`/etc., unlike the
  sibling's near-zero-capability steady-state window), so isolating onboarding specifically still buys a
  real, separately-auditable capability boundary, and keeps onboarding free to evolve (copy, phases,
  recovery actions) without touching dashboard code or its capability manifest.
- **Onboarding webview is vanilla JS/HTML/CSS, not React.** Same reasoning the sibling used for its own
  setup screen, and it maximizes reuse of the actual work being ported: `web/setup.js` (render/state
  logic), `web/setup.css` (already SIGNAL-token-based, matches this repo's `src/styles/tokens.css`), and
  `src-tauri/src/parity.rs` (state shape) port far closer to verbatim than a React re-authoring would,
  and `tests/policy.test.mjs`'s structural assertions carry over with adapted content instead of a
  rewrite. This means the app ships two frontend stacks (React for the dashboard/instance chrome, vanilla
  JS for onboarding) — an accepted, deliberate tradeoff for this surface, not a precedent for others.
- **Window labels do not map 1:1 across repos — verified against both `tauri.conf.json`s.** The sibling's
  `"main"` window *is* its setup/onboarding screen; this repo's `"main"` is already the dashboard
  (declared statically in `tauri.conf.json`, shown immediately per `App.tsx`'s existing AGENT.md-compliant
  "checking" guard — unchanged by any of this). The new isolated onboarding window here must be labeled
  something else (`"onboarding"`) and get its own capability scoped to that label, or the security bridge
  either binds to the wrong window or doesn't bind at all. Full detail and the corrected wiring is in
  Phase 5's "Window/capability label mapping" section — Phase 2's capability bullet and Phase 5's bullets
  have been written to match it, but if anything elsewhere in this doc still says `"main"` for the
  onboarding surface, that's stale and Phase 5's version wins.

## Reversal (2026-08-09): onboarding is a React screen, not a second window

The isolated-webview decision above shipped, built clean, and passed its own policy tests — but real
hardware testing found it caused a genuine, reproducible bug: creating two GTK/WebKit windows at startup
(the visible `"main"` dashboard plus the hidden `"onboarding"` window) failed EGL/GPU-driver
initialization (`Could not create default EGL display: EGL_BAD_PARAMETER`, blank white dashboard) on a
real Intel Iris Xe / Mesa 26.1.4 combination. Root-caused by process of elimination over roughly a dozen
hypotheses (NVIDIA-specific, Wayland-vs-X11, WebKit compositing mode, the DMA-BUF renderer,
software-only Mesa rendering, individual bundled shared libraries) — all ruled out with real evidence,
including the strong signal that `LIBGL_ALWAYS_SOFTWARE=1` still failed identically, which should have
bypassed any hardware-driver-specific cause. What actually fixed it: disabling creation of the second
window entirely. That fix was independently reproduced against the *sibling* app's own build too (same
two-window-at-startup pattern, same failure) — meaning this is a real bug in the pattern itself on
certain hardware, not something specific to this repo's port.

Given that, the user asked to keep onboarding as a visually distinct *screen* but drop the second
*window* — this doc's "Onboarding is an isolated webview" and "vanilla JS/HTML/CSS, not React" bullets
above are superseded:

- **One window, one React app.** `src-tauri/src/bootstrap.rs`'s `create_onboarding_window`/
  `show_onboarding`/`show_dashboard`/`open_dashboard` are deleted outright, not feature-flagged — there's
  no window left to show or hide. `bootstrap`/`begin_setup`/`run_action` (3 commands, `open_dashboard`
  dropped — nothing left for it to hand off to) are folded into the dashboard's existing
  `dashboard-bridge` capability and called from the single `"main"` window.
- **Onboarding is now `src/components/OnboardingView.tsx` + `src/hooks/useBootstrap.ts`**, a straight
  port of `public/onboarding/setup.js`'s `render(state)` logic and `setup.css`'s visual design (now
  reusing `src/styles/tokens.css` instead of duplicating its tokens) into React. `App.tsx` calls
  `bootstrap` on mount and renders `OnboardingView` in place of the dashboard until the runtime is ready
  *and* the user has clicked through ("Continue" is purely a local `App.tsx` state change now, not an
  IPC call — there's nothing left for a command to show/hide). The vanilla-JS `public/onboarding/` and
  `withGlobalTauri` (only needed for that unbundled JS to reach `window.__TAURI__`) are both deleted.
- **Real, knowingly-accepted security tradeoff.** The whole point of the original isolated-window design
  was a capability boundary the OS/Tauri enforced independently of application code — a compromised
  dashboard couldn't invoke bootstrap commands, and vice versa, because they lived behind different
  window-scoped capability grants. That boundary is gone: `bootstrap`/`begin_setup`/`run_action` are now
  reachable from the same capability grant as the rest of the dashboard's command surface. What's kept:
  `bootstrap.rs`'s own `window.label() == "main"` check (now the *only* enforcement, not
  defense-in-depth alongside a capability boundary) and the server-side `offered_actions` allowlist for
  `run_action`. This is a deliberate choice, not an oversight — a real, hardware-triggered startup crash
  was judged worse than losing this specific isolation boundary. If the isolation matters enough to
  re-add later, revisit with either a fix for the underlying two-window EGL bug (not attempted — root
  cause is in Mesa/WebKit's window-creation path, well outside this app's control) or a *lazily created*
  second window (created only once onboarding is actually needed, not unconditionally at startup — this
  wasn't tried, so it's unknown whether it would still trigger the same bug).
- **Tests**: `tests/policy.test.mjs`'s onboarding-window-isolation assertions were replaced with
  equivalents for the single-window model (dashboard-bridge's command list includes the 3 bootstrap
  commands, `authorize_main` checks `"main"`, and a new assertion that `bootstrap.rs` contains no window-
  management symbols at all — asserted by absence, so this exact pattern can't quietly creep back in).

---

## Phase 1 — Sidecar integrity (do this before shipping any real build)

Today `src-tauri/binaries/omnideck-x86_64-unknown-linux-gnu` is a local, gitignored, unpinned build
(see `AGENT.md`'s "CLI sidecar" bullet). The sibling repo has a real pinned-and-checksummed pattern
worth copying once a JSON-capable CLI release actually exists.

- [x] **Add a `vendor-manifest.json`** at `src-tauri/binaries/vendor-manifest.json` recording, per target
  triple: the CLI release tag, commit, download URL, archive SHA-256, and extracted-binary SHA-256.
  Model: sibling repo's `desktop/src-tauri/binaries/vendor-manifest.json` (schema: `schemaVersion`,
  `repository`, `tag`, `downloadBaseUrl`, `version`, `commit`, `targets[]` with
  `targetTriple`/`archive`/`archiveSha256`/`binarySha256`).
- [x] **Add a `scripts/fetch-sidecars.mjs`** that downloads + verifies one or all target triples against
  the manifest, writing `src-tauri/binaries/omnideck-<target-triple>[.exe]` and `chmod +x` on non-Windows.
  Model: sibling repo's `desktop/scripts/fetch-sidecars.mjs`. Support an `OMNIDECK_CLI_ARCHIVE_DIR`
  env var pointing at pre-downloaded archives, for offline/sandboxed builds (same pattern).
- [x] **Add a `scripts/verify-sidecars.mjs`** that re-checksums already-fetched binaries without
  re-downloading — cheap CI re-verification step.
- [x] **Add a minimum-CLI-version floor at runtime**, alongside the existing JSON contract check.
  `cli_bridge.rs::version()` currently only checks `json_contract == EXPECTED_JSON_CONTRACT` — keep that.
  Add a `MINIMUM_CLI_VERSION = "v0.10.0"` semver-floor comparison against the version string from
  `--version --json`, parsed the same way the sibling repo's `lib.rs::parse_cli_version()` does but
  compared with `>=` instead of `==` (see "Decisions from review" above for why floor-not-exact). Vendor
  the real `v0.10.0` tag (now released) in `vendor-manifest.json`, not a placeholder.
- [x] Wire `pnpm`/`npm` scripts `fetch:sidecars` / `verify:sidecars` into `package.json`, and a top-level
  `verify` script that runs fetch → test → lint → typecheck in one command (model: sibling repo's
  `verify` script composition), so CI and local dev share one entrypoint.

## Phase 2 — Capability & CSP tightening

This repo's `src-tauri/capabilities/default.json` currently grants `core:default` +
`opener:default` to the `main` window — broad Tauri defaults, not an enumerated allowlist. The CSP in
`tauri.conf.json` also allows `https://www.omnideck.dev`, `https://*.slack.com`, and wildcard
`localhost`/`127.0.0.1` in `frame-src`/`connect-src` for every window, all the time.

Per "Decisions from review," this is now a **three-surface** split, not two: the dashboard (React, broad
mutating capability), the onboarding window (vanilla JS, its own narrow capability, created per Phase 5),
and the per-instance webview (zero capability). Each gets its own capability file — don't let any of
them inherit from a shared default.

- [ ] **Split the CSP by concern across all three surfaces**: the dashboard (no reason to load remote
  frames or arbitrary localhost ports), the onboarding window (same — it only ever talks to the bundled
  CLI via typed commands, never loads network content), and the per-instance webview/iframe embedding a
  Deck's own UI (needs `http://127.0.0.1:*` / `ws://127.0.0.1:*` for arbitrary Deck ports, per
  `AGENT.md`'s CSP rule). Right now everything gets the wide policy; only the instance-webview surface
  should.
- [x] **Write a custom capability for the dashboard window** (model: sibling repo's
  `src-tauri/permissions/read-only-cli.toml` shape — a `[[permission]]` block whose `commands.allow` is
  an exact list — but not its name or its "read-only" framing, which doesn't fit: this repo's dashboard
  needs real mutating commands like `add_instance`/`remove_instance`/`start_instance`, so call it
  something accurate, e.g. `dashboard-bridge`). Enumerate exactly the `#[tauri::command]`s the dashboard
  window needs, replacing `core:default`. Do this once `commands.rs`'s command surface stabilizes past
  the current `list_instances` / `cli_version_contract` / `instance_status` / `instance_logs` / lifecycle
  actions — don't let this drift from `commands.rs`'s actual `invoke_handler!` list in `lib.rs`.
- [x] **Write a second, narrower capability for the onboarding window**, named e.g. `onboarding-bridge`
  (model: sibling's `read-only-cli.toml`/`setup-local.json` shape fits this one much more literally,
  since onboarding's job really is narrow and typed — `commands.allow` limited to `bootstrap`/
  `begin_setup`/`open_dashboard`/`run_action`). **Declare `"windows": ["onboarding"]`, not `"main"`** —
  in this repo `"main"` is the dashboard, not the setup window (the sibling's `"main"` is its setup
  window, which is why its capability says `"main"`; copying that literally here would grant the bridge
  to the wrong window and leave the real onboarding window with no capability at all). See Phase 5's
  "Window/capability label mapping" for the full reasoning. Enforce, the same way the sibling's
  `authorize_local_setup()` does but checking `window.label() == "onboarding"`, that these commands are
  only callable from the onboarding window — not reachable from the dashboard or instance webviews even
  though they're compiled into the same binary.
- [ ] **Give the per-instance webview (`InstanceWebviewTab.tsx`'s embedded Deck UI) zero Tauri
  capabilities**, whenever it gets its own webview/window (vs. today's presumed iframe) — model: sibling
  repo's `hosted-app` window, which has no `#[tauri::command]` surface reachable from it at all. This is
  the security-critical one: a Deck's own web UI is less trusted than either of this app's own surfaces,
  since it renders content from a container image that updates independently of this app's release
  cadence.

## Phase 3 — Process & lifecycle robustness

`cli_bridge.rs::run_json()` / `run_ndjson_stream()` today have no output size bound and no per-call
timeout — a hung or runaway `omnideck` sidecar process blocks the calling command indefinitely, and
unbounded stdout capture can exhaust memory.

- [x] **Bound stdout/stderr capture** in `cli_bridge.rs`. Model: sibling repo's `append_bounded()` +
  `STDOUT_LIMIT`/`STDERR_LIMIT` constants (1MB stdout / 256KB stderr) in `lib.rs` — return a distinct
  `CliError` variant (e.g. `OutputLimitExceeded`) on overflow rather than silently truncating.
- [x] **Add per-operation timeouts.** Model: sibling repo's `FixedOperation::timeout()` — short
  (~15s) for inspection calls (`list`, `status`, `doctor`, `config show`), long (~20min) for anything
  that can pull an image or do first-run setup (`add`, `update`). Kill the child process on timeout
  (`child.kill()`) rather than leaving it orphaned.
- [x] **Add `tauri-plugin-single-instance`.** This repo has no equivalent yet — a second launch opens a
  second process/window today. Model: sibling repo's `.plugin(tauri_plugin_single_instance::init(...))`
  in `lib.rs::run()`, focusing/showing the existing window on relaunch instead.
- [x] **Reassemble NDJSON lines across process-output chunks.** Check whether
  `run_ndjson_stream()`'s `text.lines()` in `cli_bridge.rs` can split a single JSON line across two
  `CommandEvent::Stdout` chunks (it iterates per-chunk, not per-logical-line) — the sibling repo hit this
  exact bug and fixed it with a small `LineBuffer` that holds a partial-line tail across `push()` calls
  (see `lib.rs::LineBuffer`, tested by `json_lines_are_reassembled_across_process_chunks`). If
  `tauri-plugin-shell`'s event stream already guarantees line-buffered chunks this may be a non-issue —
  verify against real large-output commands (`logs --tail 1000`) before deciding it's needed.

## Phase 4 — Linux AppImage runtime fixes (carry these forward, don't rediscover them)

This repo's `src-tauri/src/main.rs` and `cli_bridge.rs::sidecar_command()` already have two real,
root-caused fixes the sibling repo's shipped app does NOT have (confirmed absent in its `platform.rs`).
Since that repo also ships an AppImage bundle target, it's plausibly exposed to the same bugs — but
that's their problem to port back, not this repo's problem to solve. For *this* repo:

- [x] **Verify both fixes still apply** after any Tauri/linuxdeploy version bump:
  - `fix_appimage_webkit_exec_path()` in `main.rs` (sets `WEBKIT_EXEC_PATH` before any GTK/WebKit init,
    to stop `WebKitWebProcess`/`WebKitNetworkProcess` resolving a mismatched host-installed webkit2gtk
    and crashing with `SIGBUS`).
  - The `LD_LIBRARY_PATH` strip in `cli_bridge.rs::sidecar_command()` (stops the AppImage runtime's
    injected `LD_LIBRARY_PATH` from leaking into the `omnideck` sidecar's child `podman`, which
    otherwise silently reports every instance's status as `"unknown"` with no error).
- [x] Add a regression test/manual check that specifically exercises `list`/`status` from inside a built
  AppImage (not just `tauri dev`) after any change to `sidecar_command()` — this bug class produces no
  error, just silently wrong data, so it won't be caught by fixture tests against canned JSON.
- [x] `NO_STRIP=1` for `linuxdeploy` (toolbox binutils can't strip newer bundled libs) and
  `--appimage-extract-and-run` in the run script are both already handled in
  `scripts/build-appimage.sh`/`run-appimage.sh` — no action needed, just don't regress them when editing
  those scripts.

## Phase 5 — Bootstrap/onboarding state machine and its isolated webview

**Superseded by "Reversal (2026-08-09)" above**: this phase's window-creation steps (`create_onboarding_window`,
the `"onboarding"` capability/permission files, the vanilla-JS `public/onboarding/` bundle) were built,
shipped, then removed after a real hardware bug. Left unedited below as the historical record of the
original design and why it was chosen — the checkboxes are still `[x]` because the work described *was*
done, just later reverted. Do not use this phase as a guide for the current architecture; use the Reversal
section and `bootstrap.rs`'s own doc comment instead.

`AGENT.md` already names `bootstrap.rs` (podman/docker detection+install, WSL2 setup, `podman machine`
lifecycle) as the one legitimate non-CLI-delegated logic in this repo's target architecture, and it's
still unwritten. Per "Decisions from review," onboarding is its own isolated, vanilla-JS webview — the
same architecture the sibling repo uses for its setup screen (not Electron-specific; the sibling already
migrated off Electron to Tauri). That means this phase is a much closer port than earlier drafts of this
doc assumed: port the actual `parity.rs` state shape, `web/setup.js` render logic, `web/setup.css`
(already SIGNAL-token-based) and IPC shape, adapting *content* (phase list, copy, error codes, CLI args)
to this repo's real bootstrap phases and its `dashboard-bridge`/onboarding capability split from Phase 2
— not rewriting the mechanism from scratch.

### Window/capability label mapping — get this exact, it's security-critical

The sibling's window roles do **not** map onto this repo's window labels by name, only by function, and
porting `authorize_local_setup()`'s check verbatim (`window.label() != "main"`) would silently authorize
the *wrong* window here. Checked directly against both repos' actual `tauri.conf.json`/`lib.rs`:

- In the sibling, `"main"` **is** the setup/onboarding window (created visible by default,
  `create_desktop_windows()`'s first `WebviewWindowBuilder`), and `"hosted-app"` is the single hosted
  instance's webview (created hidden, `.visible(false)`, shown only once setup hands off).
- In **this** repo, `"main"` is already the dashboard — declared statically in `tauri.conf.json`'s
  `app.windows` array (not built in Rust code like the sibling's windows are), already shown immediately
  per `App.tsx`'s existing `cliVersion.status === "checking"` guard (AGENT.md's rule, already correctly
  implemented — don't touch this). There is no "hosted-app" concept here at all; the closest functional
  analog to it is the new onboarding window itself, not the dashboard.

So the correct mapping is: **keep `"main"` = dashboard, unchanged, zero regression. Add a new window
labeled `"onboarding"` that plays the sibling's `"hosted-app"` *lifecycle* role (created hidden by
default) but the sibling's `"main"` *capability/bridge* role (only this window may call
`bootstrap`/`begin_setup`/`open_dashboard`/`run_action`).** Concretely:

- [x] **Add the `"onboarding"` window via `WebviewWindowBuilder` inside Rust's `setup()` hook**, the same
  place `create_desktop_windows()` runs in the sibling — do **not** add it to `tauri.conf.json`'s
  declarative `app.windows` array, since `"main"` already lives there and mixing the two creation
  mechanisms for the same window is unnecessary risk; Tauri creates config-declared windows before
  `setup()` runs, so adding `"onboarding"` there afterward is purely additive. Create it with
  `.visible(false)`, mirroring the sibling's `hosted-app` default — never `"main"`'s default-visible
  behavior.
- [x] **Serve the ported vanilla-JS bundle from `public/onboarding/`** (this repo's `public/` dir exists
  and is currently empty), pointing the window at `WebviewUrl::App("onboarding/index.html".into())`.
  Vite copies `public/` into `dist/` verbatim without bundling it — this coexists with the existing
  React build for `"main"` with zero changes to `vite.config.ts` or the React app.
- [x] **Add `"withGlobalTauri": true`** to `tauri.conf.json`'s `app` block (currently absent — this repo
  relies on `@tauri-apps/api`'s npm/ESM imports for the React window, which the unbundled onboarding JS
  can't use). This is additive: it doesn't change how `"main"`'s React code accesses the Tauri API, it
  only makes `window.__TAURI__` additionally available for code that isn't going through a bundler.
- [x] **The new capability file must declare `"windows": ["onboarding"]`, not `"main"`** — rename Phase
  2's onboarding capability away from the sibling's `read-only-cli`/`setup-local` naming (which encodes
  the sibling's own window-label assumption) to something label-accurate, e.g. `onboarding-bridge` /
  `onboarding-local`.
- [x] **The Rust-side authorization check must test `window.label() == "onboarding"`**, the direct fix for
  the mismatch above — every one of `bootstrap`/`begin_setup`/`open_dashboard`/`run_action`'s handlers
  needs this guard, not `"main"`.
- [x] **`bootstrap` is invoked by the onboarding window's own JS on its own script load** (mirrors the
  sibling's `setup.js` calling `bootstrap` immediately — webviews run their JS whether visible or not, so
  this works fine from a window created with `.visible(false)`), completely decoupled from `"main"`'s
  existing mount/render code — `App.tsx`/`useCliVersion.ts` need zero changes for any of this. If
  `bootstrap` decides setup is needed, it shows+focuses `"onboarding"` (mirrors the sibling's
  `show_setup()`); if not needed, it does nothing further — `"onboarding"` stays hidden and `"main"`
  proceeds exactly as it does today. This replaces the sibling's `show_hosted()` swap, which doesn't apply
  here (no single hosted app to reveal).
- [x] **`open_dashboard` (renamed from the sibling's `open_app`) is the reverse handoff**: called by
  onboarding once setup completes, it hides `"onboarding"` and shows+focuses `"main"` — the functional
  analog of the sibling's `show_hosted()`, just handing back to the dashboard instead of a hosted app.
- [x] **Phase 3's `tauri-plugin-single-instance` relaunch handler** must pick whichever of `"onboarding"` /
  `"main"` is currently visible to focus on a second launch — port the sibling's "hosted-app if visible,
  else main" selection logic with these labels swapped in.

With that settled, the rest of this phase:

- [x] **Scaffold the isolated onboarding webview's UI**, modeled directly on the sibling's actual
  `web/setup.js`/`web/setup.css`/`web/index.html` (start from those files, strip Electron-parity fixture
  concerns, keep the SIGNAL tokens), gated by the `onboarding-bridge` capability from Phase 2.
- [x] **`begin_setup` and `run_action` keep the sibling's guard shapes**: `begin_setup` still uses
  `HostState.setup_running`'s swap-to-prevent-reentry, and `open_dashboard` still gates on
  `HostState.app_ready` the same way the sibling's `open_app` gates on it — only the window it hands off
  to changed (see the wiring subsection above), not the guard logic.
- [x] **A single state struct driving the UI declaratively**, ported from `parity.rs`'s `SetupState`
  (`stage`, `title`, `detail`, `progress`, `indeterminate`, `canStart`/`canRetry`/`canOpen`, `activity`,
  `primaryAction`/`secondaryAction`, `diagnostics`, `technical`) pushed over a Tauri `Channel<T>`, and
  `web/setup.js`'s `render(state)` function that fully re-derives the DOM from each pushed state — port
  the function, replace the phase list with this repo's actual bootstrap phases (podman/docker detection,
  WSL2 enablement, `podman machine` start) instead of the sibling's software/environment/download/startup
  phases.
- [x] **Weighted multi-phase progress**, some phases platform-conditional. Port
  `parity.rs::overall_progress()` / `phases_for(platform)` directly — each phase carries a relative
  `weight` and an `appliesTo` filter (the sibling skips "Secure space" on Linux, since only darwin/win32
  need a managed VM; this repo's `podman machine` provisioning is the same shape, different reason). One
  0–1 overall bar derived from current-phase-index + that phase's own fractional progress.
- [x] **A failure-kind taxonomy, not raw error passthrough.** Port `failure_kind()`'s mapping shape from
  `lib.rs` (bridge-error code → one of a small fixed set of user-facing kinds, each with its own
  title/detail/can-retry/recovery-action pair), but write this repo's own kind set and copy for its
  actual failure modes (podman install failed, WSL2 needs a reboot, podman machine won't start, CLI
  sidecar missing or below the `v0.10.0` floor, image pull failed) — the sibling's kinds
  (`restart`/`permission`/`downloads`/`support`/`environment`/`startup`/`release`/`components`/`unknown`)
  are a reasonable starting vocabulary to prune/extend from, not a fixed list to keep verbatim.
- [x] **Diagnostics as a checklist, shown only on failure.** Port `parity::error()`'s per-phase
  pass/issue/waiting classification and `setup.js::renderDiagnostics()` directly — the same phase list
  doubles as a live progress indicator mid-flight and a failure report on error.
- [~] **Resume semantics — determined not applicable, not skipped.** Implementing `bootstrap.rs` surfaced
  that this repo's actual scope (shared runtime readiness only — see the module's own doc comment) has no
  "did we finish pulling image X into container Y" step to remember, unlike the sibling. `runtime status`
  is cheap and authoritative on every launch, so querying it fresh *is* the resume mechanism — a crash mid
  `runtime ensure` just means the next launch's status check still reports not-ready and setup runs again,
  which is the CLI's own idempotency contract to uphold (JSON_MODE_SPEC's "Desktop runtime contract"), not
  this app's. No resume-record file was written; `bootstrap.rs`'s module doc comment explains this in full.
- [x] **Server-side allowlist of "currently offered" recovery actions.** Port `HostState.offered_actions`
  plus `run_action()`'s check that the requested action was actually included in the *last state pushed
  to that window* before executing it, verbatim. This is what stops a compromised or buggy webview from
  invoking something like "restart the computer" just because the command exists in the IPC surface —
  only what the current state actually offered is runnable.
- [x] **Port the test suite, not just its methodology** — the near-literal UI/mechanism port means most of
  `tests/policy.test.mjs`'s *structure* now applies with adapted content, not a rewrite:
  - A **policy test** (model: `tests/policy.test.mjs`'s security-assertion half) asserting the onboarding
    window's capability grants nothing beyond its Phase 2 allowlist, its command set matches exactly what
    Rust's `invoke_handler!` exposes for that window, and CSP is exactly what's expected — port the
    assertion shapes, repoint every path/selector at this repo's files. Drop only the byte-for-byte
    Electron-parity comparisons (`fixtures/electron-setup/*` and the assertions that diff against them) —
    there's no Electron app here to match, and this repo isn't even diffing against the sibling, which is
    itself post-Electron.
  - **Manual test scripts** (model: `tests/manual/recovery-lifecycle.md`, `clean-first-run.md`) — port
    the scenario list directly (close-mid-setup-and-relaunch, network-drop-during-a-pull-and-recover,
    port-collision-retry, stop-the-engine-while-the-app-is-open, reboot-after-partial-setup,
    relaunch-after-a-failed-action all apply verbatim to any long-running host-environment bootstrap),
    adapting only the concrete pass/fail steps to this repo's actual phases and error copy.
  - A **packaged-build smoke check** (model: `tests/hardware/validate-proof.mjs`) that a real installed
    build actually exercised a real (not fixture) dependency check on at least one representative OS
    before each release, writing a small proof file CI can assert on — port directly.

## Phase 6 — CI build matrix (when this repo gets CI)

This repo has no `.github/workflows/*.yml` yet. The sibling repo's matrix builds 6 targets (linux ×
{x64,arm64}, windows × {x64,arm64}, macos × {x64,arm64}) — decided, for this repo, to scope down to
**one target per OS** to keep CI cost/time down until there's an actual need for the others:

| OS | Target triple | Bundles | Rationale |
|---|---|---|---|
| Linux | `x86_64-unknown-linux-gnu` | `appimage`, `deb`, `rpm` | Keep AppImage specifically — it's the only packaging that works on immutable-filesystem distros (Fedora Silverblue/Bluefin) without root package installs. |
| Windows | `x86_64-pc-windows-msvc` | `nsis` | Most common Windows arch. |
| macOS | `aarch64-apple-darwin` | `dmg` | Apple Silicon is the current default; add `x86_64-apple-darwin` back if Intel-Mac users show up. |

- [x] When wiring CI, use exactly this 3-entry matrix (not the sibling repo's 6-entry one) —
  `linux-arm64`, `windows-arm64`, and `macos-x64` are deliberately dropped, not forgotten. Add them back
  as separate matrix entries later if real demand shows up; don't restore all six preemptively.
- [x] Model the workflow shape (not the target list) on the sibling repo's
  `.github/workflows/desktop.yml`: a fast `test` job (frontend build/typecheck, `cargo test`,
  `cargo clippy -- -D warnings`, `cargo fmt --check`) gating a `build` job matrix, with per-target
  installer artifacts uploaded and checksummed.
- [x] Package.json build scripts should be named/scoped to match — one `build:linux`
  (`--bundles appimage,deb,rpm --target x86_64-unknown-linux-gnu`), one `build:windows`
  (`--bundles nsis --target x86_64-pc-windows-msvc`), one `build:macos`
  (`--bundles dmg --target aarch64-apple-darwin`). Don't add the arm64/x64-alternate script variants
  until the matrix above actually grows.

## Phase 7 — Release engineering (later, once Phase 1–6 are done and a real release is imminent)

Lower priority — do this when actually cutting a first real release, not preemptively:

- [ ] `TESTING.md` defining test layers/gates (model: sibling repo's `TESTING.md`).
- [x] `RELEASING.md` defining tag/publish flow (model: sibling repo's `RELEASING.md`).
- [ ] A release-artifact contract check (model: sibling repo's `tests/releasecontract/verify-release.mjs`)
  asserting the expected installer files/checksums exist for a given version tag before publishing.
- [x] SHA-256 checksums generated and attached alongside installers (model: sibling repo's
  `scripts/checksums.mjs` + the `attest-build-provenance` step in its release workflow).

---

## Explicitly NOT being ported

Since the "Decisions from review" adopted the sibling's isolated-onboarding-webview architecture, this
list is shorter than earlier drafts of this doc — Phase 5 now ports both the *mechanism* and, in
adapted form, the actual UI/IPC/test code behind the sibling's setup path. What follows is genuinely
single-instance/Electron-parity-specific content that still doesn't apply to this repo's multi-instance
model:

- `HostState.hosted_port` and `show_hosted()`/`show_setup()`'s window-swap-to-one-instance's-webview
  logic — the sibling's `open_app` hands off to *one* hardcoded hosted instance webview it owns; this
  repo's `open_dashboard` (Phase 5) hands off to the multi-instance dashboard instead, which lists and
  drives N Decks itself rather than being handed a single port to display. Port the 4-command *shape* and
  the guard pattern (`app_ready`/`offered_actions`), not this specific field or the single-webview-swap.
- `setup-parity.json`'s literal copy strings and phase IDs/weights (`"software"`, `"download"`, "Bringing
  omnideck up to date", etc.) — write this repo's own copy for its own bootstrap phases, per `DESIGN.md`.
  Reuse the *shape* of the contract (Phase 5), not its values.
- The byte-for-byte-Electron-parity half of `tests/policy.test.mjs` (`setup DOM, CSS, behavior, and
  visible text are byte-for-byte Electron parity`) — there's no Electron app here to diff against. Keep
  only the security-assertion half (Phase 5's policy-test bullet).
- Hardcoded `CONTAINER_NAME`/`HOME_VOLUME`/`STATE_VOLUME` and the single-instance port-reservation logic
  (`reserve_and_persist_port`) — this repo's whole point is the CLI's existing multi-instance name/port
  model, not a single reserved port.
- The image-manifest pinning (`resources/image-manifest.json`, `image_manifest()`) — that's for
  validating a bundled *container image* digest, which is a single-instance-app concept; this repo's
  Decks each reference whatever image the CLI's `add`/`update` commands resolve.
- Windows restart-to-resume via `RunOnce` registry key — this one's content, not just copy, genuinely is
  reusable (any Windows WSL2-install-requiring-reboot flow needs the same OS mechanism), but it belongs
  inside Phase 5's `bootstrap.rs` failure-kind handling once that's written, not as a standalone item —
  folded in there rather than listed separately.
