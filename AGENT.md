# AGENT.md

Instructions for any agent (human or AI) building and maintaining this repo.

## What this repo is

The Tauri v2 + React/TypeScript rewrite of the Omnideck desktop app, replacing the Electron app at `omnideck/desktop/` in the `omnideck` monorepo. It's a thin GUI shell over the `omnideck` CLI — almost no container/lifecycle logic belongs in this repo; that all lives in the CLI and is reached through its `--json` contract.

**Current status: sequencing step 2 done, step 3 started.** The Tauri v2 + React/TS scaffold exists (`src-tauri/`, `src/`) and builds clean (`cargo build`, `cargo clippy -- -D warnings`, `npm run build` all pass). `cli_bridge.rs` + `commands.rs` implement `list_instances`/`cli_version_contract` against the real CLI JSON contract (read-only — no lifecycle actions wired yet). The frontend has an app shell (Dashboard/Settings nav) built on the ported SIGNAL tokens, with a Dashboard view that polls `list --json` and a blocking screen for CLI-missing/contract-mismatch. Still to do, in order: instance detail/logs, lifecycle actions (start/stop/restart/update/remove/create), `bootstrap.rs`, onboarding polish, migration. See `desktop_tauri_rewrite.md`'s Sequencing section for the full list. Update this file as decisions firm up; don't let it go stale the way the docs it was built from briefly did (see `reference/` for why local copies of prior art exist now).

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
- **`bootstrap.rs`** (podman/docker detection+install, WSL2, podman machine lifecycle) is the one legitimate place with logic that doesn't route through the CLI — the CLI intentionally has no equivalent. Everything else goes through `cli_bridge.rs`.
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

- **Linux dev environment**: this host is an immutable/atomic distro (Bluefin/Silverblue) with no `cargo`/`rustc` and no `webkit2gtk-4.1`/`gtk3` dev headers on the base system — correctly so, don't try to `rpm-ostree install` them onto the host. Development happens inside a toolbox:
  ```
  toolbox create omnideck-dev   # once
  toolbox run -c omnideck-dev sudo dnf install -y rust cargo webkit2gtk4.1-devel \
    gtk3-devel librsvg2-devel openssl-devel libappindicator-gtk3-devel patchelf \
    file curl wget go nodejs npm clippy rustfmt
  toolbox run -c omnideck-dev bash -c 'cd /path/to/desktop && npm run tauri dev'
  ```
  `npm install`/`vite build` (frontend-only) work fine on the bare host since Node is available there too (via Homebrew in this environment) — only the Rust/webview half needs the toolbox. macOS/Windows contributors don't need any of this; it's Linux-specific.
- **Scaffold**: already done — `npm create tauri-app@latest -- --template react-ts` plus `npm run tauri add shell` (adds `tauri-plugin-shell` for CLI subprocess spawning). **`create-tauri-app`'s `--force`/`-f` flag does not mean "tolerate a non-empty directory," it means "overwrite/delete what's there."** Running it against this repo's root once wiped every untracked planning doc (`AGENT.md`, `DESIGN.md`, `desktop_tauri_rewrite.md`, `reference/`, `wireframes/`) with no prompt and no way to recover via git (they'd never been committed). Recovered that time only because sources for each file happened to still exist elsewhere (session transcript, the `omnideck` monorepo, the user's Downloads folder) — don't count on that luck twice. **Never re-run the scaffold command against a non-empty target directory.** If you ever need to re-scaffold, do it in an empty temp dir and merge by hand.
- **CLI binary for local dev**: the sidecar isn't bundled yet (deferred per sequencing). Build it from a pinned tag/commit of `/var/home/ron/Code/omnideck/cli` — e.g. `git worktree add /tmp/cli-build v0.10.0-alpha.2 && cd /tmp/cli-build && go build -ldflags="-s -w -X main.version=v0.10.0-alpha.2" -o omnideck .` — then copy the binary onto `PATH` (`~/.local/bin/omnideck` works for both the bare host and the toolbox, since toolbox shares `$HOME`). Verify with `omnideck --version --json` (expect `"jsonContract":1`) and `omnideck list --json`.
- **Frontend**: `npm install`, `npm run dev` (Vite only) or `npm run tauri dev` (full app, needs the toolbox on Linux), `npm run build` (typecheck + production bundle — works on the bare host). No test runner wired in yet — Vitest is still the natural fit; decide and record here when it lands.
- **Backend**: from `src-tauri/` inside the toolbox — `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt`. No `cargo test` suite yet (see Testing expectations below — `cli_bridge.rs` fixture tests are the next thing to add).
- **Full packaged app**: `npm run tauri build` (not yet exercised in this repo).

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
