# AGENT.md

Instructions for any agent (human or AI) building and maintaining this repo.

## What this repo is

The Tauri v2 + React/TypeScript rewrite of the Omnideck desktop app, replacing the Electron app at `omnideck/desktop/` in the `omnideck` monorepo. It's a thin GUI shell over the `omnideck` CLI — almost no container/lifecycle logic belongs in this repo; that all lives in the CLI and is reached through its `--json` contract.

**Current status: sequencing steps 2–4 done** (Tauri mechanics, read-only dashboard, and lifecycle actions — see `desktop_tauri_rewrite.md`'s Sequencing section). `src-tauri/` and `src/` build clean (`cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `npm run build` all pass). `cli_bridge.rs` + `commands.rs` cover `list`/`status`/`logs`/`start`/`stop`/`restart`/`add`/`remove` against the real CLI JSON/NDJSON contract, with `cargo test` fixtures pinning the JSON shapes. The frontend has an app shell (Dashboard/Settings nav) on the ported SIGNAL tokens: Dashboard polls `list --json` with per-row Start/Stop/Restart/Logs/Remove, a New Deck form streaming `add --json` progress, a Remove confirmation dialog with the CLI's required explicit keep/delete + backup choices, and a blocking screen for CLI-missing/contract-mismatch. Every backend command was verified against the real CLI directly (not just typechecked) before being trusted. `update_instance`, instance detail drill-in (DESIGN.md #6), and Open UI instance webview tabs (DESIGN.md #7) are done — the "still to do" list below was stale about these. **Hardening-migration Phases 1–4 are done** (see `reference/desktop-hardening-migration-PLAN.md`): sidecar pinned+checksummed against real CLI `v0.10.0` (`vendor-manifest.json`, `fetch:sidecars`/`verify:sidecars`), `EXPECTED_JSON_CONTRACT` corrected to `2` with a `MINIMUM_CLI_VERSION` floor check, the dashboard capability replaced with an enumerated `dashboard-bridge` allowlist, `cli_bridge.rs` now bounds stdout/stderr and enforces per-operation timeouts via a unified `run_cli` helper with correct NDJSON line reassembly across chunk boundaries, `tauri-plugin-single-instance` is wired up, and the AppImage runtime fixes were confirmed still intact and still build clean. **Hardening-migration Phase 5 is also done**: `bootstrap.rs` drives the shared Podman runtime's
readiness via `cli_bridge::runtime_status`/`runtime_ensure` (correcting this doc's earlier claim that the
CLI had no equivalent — it does, as of `v0.10.0`) and owns the 4-command IPC surface
(`bootstrap`/`begin_setup`/`open_dashboard`/`run_action`) for the isolated `"onboarding"` window, created
hidden via `WebviewWindowBuilder` in `lib.rs`'s `setup()` hook and scoped to its own `onboarding-bridge`
capability (never `"main"`, which is the dashboard here — see the module's own doc comment and
`reference/desktop-hardening-migration-PLAN.md`'s "Decisions from review" for why the sibling's window
labels don't map 1:1). The onboarding UI itself (`public/onboarding/{index.html,setup.css,setup.js,
host-adapter.js}`) is vanilla JS/CSS, ported from the sibling's `web/` and adapted to this repo's two real
bootstrap phases (`software`/`environment`, matching the CLI's own `runtime ensure` stages exactly — no
`download`/`startup` phase here, since pulling an image and creating a Deck is the dashboard's separate,
already-built `add_instance` flow). No resume-record file — `bootstrap.rs`'s doc comment explains why one
isn't needed here. Test coverage ported alongside: `tests/policy.test.mjs` (security-posture assertions,
`node --test`, wired into `npm run test:policy` and the `verify` composite), `tests/manual/*.md`
(clean-first-run and recovery-lifecycle procedures), and `tests/hardware/validate-proof.mjs` (packaged-build
smoke check, `OMNIDECK_DESKTOP_SMOKE_FILE`-gated in `lib.rs`). Verified end-to-end this session: `npm run
verify` clean (fetch/verify sidecars, policy tests, typecheck, fmt, Rust tests, clippy), a real `npm run
dev:app` launch with both windows live and no errors, a real `npm run build:appimage` + `run:appimage`
launch with no crashes/coredumps, and a caught-and-fixed regression (`bootstrap()` was unconditionally
showing the onboarding window on every launch before the fix — `tests/policy.test.mjs` now guards this
shape). Still to do: onboarding visual polish/copy review, migration (legacy Electron data → CLI-managed
instance — untouched by this session, still the highest-risk remaining path per this doc's rules above),
and Phase 6/7 of the hardening plan (CI, release engineering — explicitly deferred in that doc until this
repo actually gets CI / cuts a first release, not preemptive work). Update this file as decisions firm up;
don't let it go stale the way the docs it was built from briefly did (see `reference/` for why local
copies of prior art exist now).

## Read first, in order

1. **[`desktop_tauri_rewrite.md`](./desktop_tauri_rewrite.md)** — the architecture, the "why," the full feature specs, sequencing, risks, migration plan. Single source of truth for what to build and in what order.
2. **[`reference/desktop-cli-unification-SPEC.md`](./reference/desktop-cli-unification-SPEC.md)** / **`-ISSUE.md`** — the data-layer contract this app is built on top of (multi-instance model, dashboard data model, migration mechanics).
3. **The CLI's JSON contract** — `/var/home/ron/Code/omnideck/cli/docs/JSON_MODE_SPEC.md` (external repo, already shipped code, not just a spec). Every shape `cli_bridge.rs` parses comes from here. On any disagreement between this doc and that file, the CLI repo wins — refresh `reference/` if it drifts materially.
4. **[`DESIGN.md`](./DESIGN.md)** — every screen/state that needs a wireframe before it's implemented. If a screen isn't listed there and isn't trivial, get it added rather than improvising the UI ad hoc.
5. **[`reference/tauritest-SPEC.md`](./reference/tauritest-SPEC.md)** — worked Rust/React code samples for sidecar spawn, event streaming, and iframe embedding. A starting point for mechanics, not a source of product decisions — `desktop_tauri_rewrite.md` explicitly overrides parts of it (JSON contract instead of `--plain` stdout scraping, multi-instance instead of single-instance).

## Non-negotiable architectural rules

These come straight out of the plan; violating them defeats the point of the rewrite.

- **The frontend never talks to podman/docker or spawns the CLI directly** — only through `#[tauri::command]`s into Rust (`commands.rs` → `cli_bridge.rs`). Same separation of concerns the Electron app already has via IPC; don't regress it.
- **`cli_bridge.rs` owns 100% of subprocess spawn + JSON/NDJSON parsing.** It validates the reported `jsonContract` against what this app build expects and returns a distinguishable error on mismatch. It carries zero podman/docker-specific knowledge of its own — that's the CLI's job, not this repo's.
- **Never reimplement anything the CLI already does** — create/start/stop/update/inspect/remove, the multi-instance registry, doctor diagnostics. If a feature seems to need bypassing the CLI, that's a signal to extend the CLI's JSON contract, not to shell out around it from Rust.
- **The window is created and shown immediately on launch.** Never make window visibility conditional on an async readiness check completing. Render a real default state ("Checking your setup…") before the first backend event arrives — never a blank pane.
- **`bootstrap.rs`** drives the *shared* (not per-instance) Podman runtime lifecycle — podman/docker detection+install, WSL2, podman machine — but, correcting this doc's earlier claim, **the CLI does have an equivalent as of `v0.10.0`**: `runtime status`/`runtime ensure` (`cmd/runtime.go`, `engine/runtime_setup.go` in the CLI repo), added specifically for this cross-app need after this doc was first written. `bootstrap.rs` is a thin driver over that NDJSON stream (same shape as `add`/`update`/`remove`'s), translating its `stage`/`activity`/`RuntimeSetupFailure` vocabulary into this app's onboarding `SetupState` — it does **not** reimplement platform-specific installer logic (the CLI's `engine` package already owns pinned+checksummed podman installers per platform). Per-instance provisioning (pulling the omnideck image, creating a Deck) is unrelated and already built (`NewDeckForm.tsx` → `add_instance`). Everything else goes through `cli_bridge.rs`.
- **CSP `frame-src`/`connect-src` must allow `http://localhost:*` and `ws://localhost:*` generally**, not one hardcoded port — instances run on dynamic ports, and the app's browser-control feature depends on WebSockets working through whatever embeds the instance UI (iframe by default, `WebviewWindow`/multi-webview as the documented fallback if iframe hits CSP/WebSocket limits).
- **The sidecar CLI binary is pinned by version + checksum** (`desktop-cli-unification/SPEC.md §2`), never silently tracks `latest`. A version bump is a deliberate, reviewed change to a manifest, not an incidental side effect of a build.
- **Migration (legacy Electron data → CLI-managed instance) is verify-before-delete, always.** Never remove or overwrite legacy data before the new instance is confirmed working. This is the highest-risk code path in the app — extra review, extra tests, no shortcuts, no silent data loss.

## Target repo layout

```
Tauri app
 ├─ Rust backend (src-tauri/src/)
 │   ├─ bootstrap.rs   — env dependency detection/install: podman/docker,
 │   │                   WSL2, podman machine lifecycle. Ported from the
 │   │                   bootstrap half of omnideck/desktop/src/runtime.cjs
 │   │                   in the monorepo. The CLI has no equivalent.
 │   ├─ cli_bridge.rs  — spawns the bundled `omnideck` sidecar binary,
 │   │                   parses its JSON/NDJSON output, validates the
 │   │                   jsonContract version, streams progress/log events
 │   │                   to the frontend via app.emit().
 │   └─ commands.rs    — #[tauri::command] surface the frontend calls:
 │                        check_dependencies, install_dependencies,
 │                        list_instances, instance_status, instance_logs,
 │                        create/start/stop/restart/update/remove_instance,
 │                        cli_version_contract.
 └─ Frontend (React + TS + Vite)
     ├─ App shell        — persistent nav: Dashboard | open instance tabs
     │                     | Help | Community | Settings (advanced logging)
     ├─ Onboarding view  — first run / no instances yet
     ├─ Dashboard view   — Decks list
     ├─ Instance detail  — logs, health, resource snapshot
     └─ Instance webview — iframe (or Tauri multi-webview) to that
                            instance's own `127.0.0.1:<port>`
```

See `desktop_tauri_rewrite.md`'s own "Target architecture" section for the full annotated version with citations — this is a trimmed copy for quick reference while scaffolding.

## Getting started / build & test

Scaffolded and verified working (sequencing step 2 + a first read-only slice of step 3 — see `desktop_tauri_rewrite.md`). Commands below are confirmed against real code in this repo, not aspirational.

- **Linux dev environment**: this host is an immutable/atomic distro (Bluefin/Silverblue) with no `cargo`/`rustc` and no `webkit2gtk-4.1`/`gtk3` dev headers on the base system — correctly so, don't try to `rpm-ostree install` them onto the host. Rust needs a toolbox to *build*:
  ```
  toolbox create omnideck-dev   # once
  toolbox run -c omnideck-dev sudo dnf install -y rust cargo webkit2gtk4.1-devel \
    gtk3-devel librsvg2-devel openssl-devel libappindicator-gtk3-devel patchelf \
    file curl wget go nodejs npm clippy rustfmt
  ```
  But the compiled binary must *run* on the host, not inside the toolbox — confirmed the hard way: a toolbox has no `podman` of its own, and even the host's `podman` binary reached via the toolbox's `/run/host` bind mount crashes there (needs namespace/capability access a toolbox doesn't grant), while the compiled Tauri binary's runtime deps (webkit2gtk, gtk3) already resolve cleanly on the bare host (`ldd` reports nothing missing — this image ships those runtime libs even though the `-devel`/pkgconfig files were never installed). So: **run `npm run dev:app`**, which handles this automatically (`scripts/dev.sh` — builds inside the toolbox, then runs the result directly on the host, with Vite also on the host). Don't hand-roll `toolbox run ... tauri dev`; it'll compile fine and then fail every podman-backed action. macOS/Windows/non-atomic-Linux contributors don't need any of this — the script just runs `npm run tauri dev` directly there.
- **Scaffold**: already done — `npm create tauri-app@latest -- --template react-ts` plus `npm run tauri add shell` (adds `tauri-plugin-shell` for CLI subprocess spawning). **`create-tauri-app`'s `--force`/`-f` flag does not mean "tolerate a non-empty directory," it means "overwrite/delete what's there."** Running it against this repo's root once wiped every untracked planning doc (`AGENT.md`, `DESIGN.md`, `desktop_tauri_rewrite.md`, `reference/`, `wireframes/`) with no prompt and no way to recover via git (they'd never been committed). Recovered that time only because sources for each file happened to still exist elsewhere (session transcript, the `omnideck` monorepo, the user's Downloads folder) — don't count on that luck twice. **Never re-run the scaffold command against a non-empty target directory.** If you ever need to re-scaffold, do it in an empty temp dir and merge by hand.
- **CLI sidecar**: bundled via Tauri's `externalBin` mechanism (`bundle.externalBin: ["binaries/omnideck"]` in `tauri.conf.json`; `cli_bridge.rs` spawns it with `app.shell().sidecar("omnideck")`, never PATH). **Pinned by version + checksum as of `omnideck-dev/cli` `v0.10.0`** (its first release with the finalized `JSON_MODE_SPEC.md` contract) — `src-tauri/binaries/vendor-manifest.json` records the tag, commit, download URLs, and archive/binary/SBOM SHA-256s for all six target triples; the local-unpinned-build exception this note used to describe is resolved. Run `npm run fetch:sidecars` to download+verify all six (or `node scripts/fetch-sidecars.mjs <target-triple>` for just one), writing `src-tauri/binaries/omnideck-<target-triple>[.exe]` (gitignored — only `vendor-manifest.json` is committed). `npm run verify:sidecars` re-checksums already-fetched binaries without re-downloading. `OMNIDECK_CLI_ARCHIVE_DIR` may point at a directory of pre-downloaded release archives for an offline/sandboxed fetch; the pinned hashes are still enforced either way. `cli_bridge.rs::version()` checks both `EXPECTED_JSON_CONTRACT` (`2` as of `v0.10.0`, exact match) and `MINIMUM_CLI_VERSION` (`v0.10.0`, floor — not exact, since the sidecar is always exactly what the manifest pinned at build time; see `reference/desktop-hardening-migration-PLAN.md`'s "Decisions from review" for why floor-not-exact). To bump the pinned version: re-run the manifest-generation steps in that plan doc's Phase 1 against the new tag's real GitHub release assets (don't hand-edit checksums), then update `MINIMUM_CLI_VERSION`/`EXPECTED_JSON_CONTRACT` in `cli_bridge.rs` in the same change.
- **Frontend**: `npm install`, `npm run dev` (Vite only) or `npm run dev:app` (full app — see above), `npm run build` (typecheck + production bundle — works on the bare host). No test runner wired in yet — Vitest is still the natural fit; decide and record here when it lands.
- **Backend**: from `src-tauri/` inside the toolbox — `cargo build`, `cargo test` (fixture tests against canned JSON, no real CLI/podman needed), `cargo clippy -- -D warnings`, `cargo fmt`.
- **Full packaged app (Linux AppImage; other bundle targets untested here)**: `npm run build:appimage`, then `npm run run:appimage` to launch it (`scripts/build-appimage.sh` / `scripts/run-appimage.sh`). Since this is a release build, sidecar resolution works the same way as dev (no debug/PATH fallback involved) — the bundled `omnideck` binary ships inside the AppImage. Four real issues had to be worked through to get a *correctly functioning* AppImage out of this toolbox, all now handled automatically by the scripts/code below — worth knowing about if a rebuild ever breaks again:
  - **`xdg-open` / FUSE2 missing in the toolbox**: `dnf install -y xdg-utils fuse fuse-libs` (needed once per toolbox — not scripted, since it's a one-time environment setup step, not a per-build one).
  - **`strip` can't parse `.relr.dyn`**: this toolbox's binutils (2.44) can't strip its own newer system libraries that get bundled in. `build-appimage.sh` sets `NO_STRIP=1` (linuxdeploy's own escape hatch — skips stripping, AppImage is a bit larger).
  - **WebKitGTK helper-process crash (`SIGBUS`)**: without `WEBKIT_EXEC_PATH` set, `WebKitWebProcess`/`WebKitNetworkProcess` resolve via their compiled-in absolute path instead of the bundled copies — and if that finds a *different* webkit2gtk build already on the host, the mismatched shared-memory IPC crashes almost immediately. Confirmed via `coredumpctl`, not a guess. Fixed permanently in `src-tauri/src/main.rs` (`fix_appimage_webkit_exec_path`) — in Rust, not a build-time AppRun hook patch, because `linuxdeploy` regenerates its own hook on every build regardless of what's injected via `bundle.linux.appimage.files`, silently clobbering any hook-level fix.
  - **`LD_LIBRARY_PATH` leaking into podman itself, breaking container inspection silently (`status: "unknown"` for every instance, even running ones — no crash, no error, just wrong answers)**: the AppImage runtime sets `LD_LIBRARY_PATH` (among other vars) so *our own* GTK/WebKit process finds its bundled libraries — but every child process inherits it by default, including the `omnideck` sidecar and, in turn, *its* child, podman. Podman dynamically linking against the AppImage's bundled versions of libraries it also happens to depend on (instead of the host's) is enough to break container inspection without erroring outright. Root-caused by comparing the real running app's full `/proc/<pid>/environ` against a manual reproduction that initially didn't reproduce the bug (because it used a stripped-down `env -i` environment that accidentally avoided the problem) — not a guess, verified end-to-end with the fix applied. Fixed in `cli_bridge.rs`'s `sidecar_command()` helper, which clears `LD_LIBRARY_PATH` specifically (the only var of the AppImage-injected set that actually affects `ld.so`'s dynamic linking) before every sidecar spawn.
  - **Separately** (not yet root-caused as environment-specific vs. universal): the packaged AppImage's default FUSE-mount execution exits silently within a few seconds in this setup — the loose `AppDir/AppRun` and `--appimage-extract-and-run` both stay running reliably in every test, the plain double-executed `.AppImage` never did. `run-appimage.sh` always passes `--appimage-extract-and-run`. If you ever build for real distribution, re-test the plain (no-flag) launch on a target machine before assuming this flag is required everywhere — it may be specific to FUSE behavior inside this toolbox.

## Testing expectations

Pulled from `desktop_tauri_rewrite.md`'s Testing section — build toward this coverage as each sequencing step lands, not all at once on day one:

- `cli_bridge.rs` unit tests run against canned JSON fixtures — no real podman/docker needed for these.
- Integration tests run the real bundled/PATH binary against real podman, covering at least two concurrent instances.
- A migration-simulation test exercises the legacy-data path without touching real user data.
- A cross-platform sidecar-spawn test (Windows/macOS/Linux) once sidecar bundling lands.
- A CSP/iframe/WebSocket smoke test per platform for the embedded instance webview — the regression guard for the browser-control feature specifically.
- A startup-latency regression guard: window visible within an explicit, agreed time budget on every platform, always showing real state — never a blank/frozen window.

## Conventions

- **Rust**: `cargo fmt` + `cargo clippy -- -D warnings` clean before committing. Keep `cli_bridge.rs` / `bootstrap.rs` / `commands.rs` split by responsibility as named above — don't let podman-specific logic creep into `commands.rs`, and don't let CLI-spawn logic creep into `bootstrap.rs`.
- **TypeScript**: strict mode on. Component/hook layout follows the structure sketched in `reference/tauritest-SPEC.md` §6 (`components/`, `hooks/`, `styles/`) as a starting point — extend it rather than fight it, unless the multi-instance app shell genuinely needs a different shape, in which case update this file to match.
- **Visual design**: don't invent new colors, spacing, or radii. Reuse the existing "SIGNAL" token set already in production in the Electron app (`omnideck/desktop/src/setup/setup.css` in the monorepo; summarized in `DESIGN.md`'s "Visual language" section). A screen that seems to need a token that doesn't exist is a design conversation, not a CSS-file improvisation.
- **Keep the docs honest.** If a sequencing step in `desktop_tauri_rewrite.md` completes, or a decision in it changes, update that doc (and this one, if it affects how the repo is built) in the same change — not as separate follow-up cleanup that may never happen.
