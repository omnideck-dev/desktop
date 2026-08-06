# Desktop app rewrite: Tauri + CLI-delegated, multi-instance dashboard

## Reference docs

This plan lives in a standalone repo (`/var/home/ron/Code/omnideck/desktop/`) separate from the `omnideck` monorepo that contains the current Electron app, the CLI, and the prior-art specs this plan builds on. Local, durable copies of those specs are kept in [`reference/`](./reference/) so links here resolve without depending on another repo's state:

- [`reference/desktop-cli-unification-ISSUE.md`](./reference/desktop-cli-unification-ISSUE.md) / [`reference/desktop-cli-unification-SPEC.md`](./reference/desktop-cli-unification-SPEC.md) — copied from `omnideck/plans/desktop-cli-unification/` in the monorepo.
- [`reference/tauritest-SPEC.md`](./reference/tauritest-SPEC.md) — the Tauri v2 POC spec once at `omnideck/../tauritest/SPEC.md`. That directory was deleted at some point after July 2026; this copy was recovered verbatim from an old session transcript and is now the only surviving copy anywhere — treat it as authoritative going forward.
- The current Electron app referenced throughout this doc (`runtime.cjs`, `setup/index.html`, `agent-dash.js`, etc.) lives at `/var/home/ron/Code/omnideck/omnideck/desktop/` in the monorepo, **not** at any path relative to this repo.

## Why

The current desktop app (Electron, at `omnideck/desktop/` in the monorepo — see above) has three compounding problems:

1. **It's a second implementation of the CLI.** `desktop/src/runtime.cjs` calls `podman` directly with an isolated storage root, a hardcoded single container name, and its own duplicated lifecycle logic. A container created by the desktop app is invisible to `podman ps`, to Podman Desktop, and to the CLI — already fully diagnosed in [`reference/desktop-cli-unification-ISSUE.md`](./reference/desktop-cli-unification-ISSUE.md) and [`SPEC.md`](./reference/desktop-cli-unification-SPEC.md).
2. **It's single-instance**, with no way to run/manage more than one "Deck" from the GUI, while the CLI already supports this natively.
3. **It's built on Electron**, bundling a full Chromium + Node runtime per install. A prior investigation into this app suspected its startup sequence hides the window until every bootstrap/start/health-check step finishes — **that claim doesn't hold up against the code as it stands today**: `main.cjs` already calls `mainWindow.show()` immediately after the static setup page paints, before any readiness check runs (see Feature Spec §1 for what's actually still missing).

This plan folds together two existing pieces of prior art rather than re-deriving them:

- **[`reference/desktop-cli-unification-ISSUE.md`](./reference/desktop-cli-unification-ISSUE.md)** + **[`SPEC.md`](./reference/desktop-cli-unification-SPEC.md)** — already-detailed spec for making the CLI the single source of truth for instance lifecycle, with a JSON/NDJSON contract, multi-instance ("Decks") support, and a dashboard. **This plan does not redefine that work — it adopts it as the data layer.** The JSON/NDJSON contract itself (§1 of that spec) is no longer just planned — it's shipped, see Sequencing step 1 below.
- **[`reference/tauritest-SPEC.md`](./reference/tauritest-SPEC.md)** — a Tauri v2 POC spec with worked Rust/React code for spawning the CLI, streaming output, and embedding the web app in a webview. **This plan adopts its mechanics, but replaces its plain-text `--plain` stdout scraping with the JSON/NDJSON contract from the unification spec**, and replaces its single-instance two-tab (Setup/Dashboard) UI with a multi-instance, dashboard-first shell. Note: this doc was spec-only — no working Tauri app code was ever built from it, so there's no POC codebase to inherit, only the design and code samples in the doc itself.

Where this plan adds anything genuinely new, it's: the app-chrome/navigation model, the instant-open/progress-transparency requirements, the tips/video-while-waiting panel, and the advanced-logging toggle — none of which existed in either prior doc.

## Goals

1. **Podman-visible, CLI-consistent management.** Every instance created by the desktop app or the CLI is visible and manageable from both. One implementation of "create/start/stop/update/inspect/remove," owned by the CLI.
2. **Multi-instance dashboard.** A default view listing installed Decks (instances) with status (running/stopped), resource usage, per-instance update, and log viewing (with copy-to-clipboard for reporting).
3. **App chrome.** A persistent shell around the content with quick links to: the dashboard, each open/pinned instance's UI, Help (docs website), and Community (Slack).
4. **Instant, transparent startup.** The window appears immediately and always shows real state — checking prerequisites, installing dependencies, pulling images, booting a container — never a blank/frozen window.
5. **Approachable by default, powerful on demand.** Tips or a short intro video fill genuinely long waits for casual users; a toggleable "advanced logging" view exposes the raw CLI/container output for power users and bug reports.
6. **Still handles dependency setup.** Podman/Docker detection and install, WSL2 setup on Windows, and Podman machine lifecycle on macOS/Windows remain the app's job — the CLI intentionally has no equivalent (confirmed in `desktop-cli-unification/SPEC.md`).
7. **Tauri instead of Electron.** Native system webview, no bundled Chromium, meaningfully smaller installer and faster cold start.

## Non-goals (v1)

Inherits all non-goals from `desktop-cli-unification/ISSUE.md` (no cross-device sync, no bespoke high-polish dashboard redesign for v1, no CLI distribution changes) plus, specific to the Tauri rewrite:

- No bundled Podman/Docker (Phase 3 idea in `tauritest/SPEC.md`, not this plan).
- No code signing/notarization/auto-update/system tray for the initial cutover — match current Electron app's ship state first, add these back before general release if the current app already has them (it does not, per the Electron `package.json` review — no native modules or signing beyond `@electron/notarize` at build time).
- No full interactive PTY/terminal emulation. Advanced logging is a live read-only stream (stdout/stderr passthrough), not an embedded shell.

## Target architecture

```
Tauri app
 ├─ Rust backend (src-tauri/src/)
 │   ├─ bootstrap.rs   — env dependency detection/install: podman/docker,
 │   │                   WSL2, podman machine lifecycle. Ported from the
 │   │                   bootstrap half of desktop/src/runtime.cjs. The CLI
 │   │                   has no equivalent and isn't expected to grow one.
 │   ├─ cli_bridge.rs  — spawns the bundled `omnideck` sidecar binary,
 │   │                   parses its JSON/NDJSON output, validates the
 │   │                   jsonContract version, streams progress/log events
 │   │                   to the frontend via app.emit(). Mirrors
 │   │                   cli-bridge.cjs from desktop-cli-unification/SPEC.md
 │   │                   — same responsibilities, different host language.
 │   └─ commands.rs    — #[tauri::command] surface the frontend calls:
 │                        check_dependencies, install_dependencies,
 │                        list_instances, instance_status, instance_logs,
 │                        create/start/stop/restart/update/remove_instance,
 │                        cli_version_contract.
 └─ Frontend (React + TS + Vite, per tauritest/SPEC.md's stack choice)
     ├─ App shell        — persistent nav: Dashboard | open instance tabs
     │                     | Help | Community | Settings (advanced logging)
     ├─ Onboarding view  — first run / no instances yet: dependency checks,
     │                     install progress, tips/video panel, optional
     │                     advanced-log drawer
     ├─ Dashboard view   — Decks list: name, status dot, port, CPU%,
     │                     mem used/total, image, uptime, actions
     ├─ Instance detail  — logs (tail + follow), health, resource history
     └─ Instance webview — iframe (or Tauri multi-webview) to that
                            instance's own `127.0.0.1:<port>`
        │
        ▼
   bundled `omnideck` CLI binary (per-platform sidecar via Tauri's
   `externalBin` mechanism, per tauritest/SPEC.md §5.3/§9 Phase 2 —
   that doc only names the mechanism, with no pinning/checksum detail.
   Version pinning + checksum verification is specified in full by
   desktop-cli-unification/SPEC.md §2, and is what this plan actually
   follows for that part)
        │  shells out to
        ▼
   podman / docker (ambient default storage — same one `podman ps` sees)
```

The frontend never talks to podman or the CLI binary directly — only through Tauri commands into Rust, same separation of concerns the Electron app already has via IPC.

## Feature specs

### 1. Instant open + transparent progress

**Re-verified against current code**: the Electron app's `main.cjs` already calls `mainWindow.show()` right after the static setup page (`src/setup/index.html`) paints — before `runtime.setup()` or any readiness check runs — with a comment explicitly warning against gating visibility on backend checks. So the "hidden window" failure mode this section was originally written to fix isn't present in the app as it stands today; don't take that framing at face value without a fresh repro. What's still a genuine, worth-preserving requirement for the Tauri rewrite:

- The Tauri window is created and shown immediately on launch — no `show: false` gate on a backend readiness check, matching (not fixing) the current Electron app's already-correct behavior. Tauri's native webview has no Chromium cold-start cost, so this is close to free, but the *principle* still has to be enforced deliberately: never make window visibility conditional on an async check completing.
- The frontend renders a real default state (not a blank page) before the first backend event arrives — "Checking your setup…" — exactly like the existing `setup/index.html`'s static welcome markup, ported forward as prior art worth keeping, not a gap to close.
- The Rust backend pushes state events as it progresses (dependency check → CLI version/contract check → instance list load → per-instance status), the same `emitCopy`/`emitWorking` pattern `runtime.cjs` already uses, generalized to Tauri's `app.emit()`.

### 2. Tips / intro video while waiting

- A `<TipsPanel>` (or similar) shown during genuinely long waits (dependency install, image pull, first container boot) — not on every launch once an instance is already running.
- Content: a rotating tip carousel and/or a short embedded intro video. Keep this decoupled from the install/progress logic so it can be redesigned or A/B'd without touching the state machine.
- The existing "Agent Dash" mini-game (`desktop/src/setup/agent-dash.js`) can be kept as one of the panel's options, folded into this component, or dropped — a product/design call, not an architecture one.

### 3. Advanced logging toggle

- A persistent, off-by-default toggle in the app chrome (Settings or footer).
- When on, a collapsible drawer shows the live raw stdout/stderr stream from the CLI sidecar as it runs (reusing the `CommandEvent::Stdout`/`Stderr` → `app.emit()` mechanism from `tauritest/SPEC.md` §7 Step 4), plus "copy to clipboard" and "open log file" actions.
- This is the same underlying data as the dashboard's per-instance historical log view (`logs --json`, non-follow) — one live/ephemeral surface (advanced drawer, current process only) and one historical surface (per-instance logs panel, persisted container logs), not two separate logging systems.

### 4. Dashboard (multi-instance)

Adopt the data model and actions from `desktop-cli-unification/SPEC.md` §5 as-is:

- List of Decks: name, status dot, port, CPU%, mem used/total, image, uptime — sourced from one `list --json` call, polled every 2–3s while the dashboard is open or refreshed on window focus.
- Resource usage comes straight from the CLI's already-implemented `ContainerStats` (`cpuPct`, `ramBytes`/`ramTotalBytes`) — no new data plumbing needed on the CLI side, confirmed during research (`engine/engine.go`, `engine/podman.go`, `engine/docker.go`).
- Per-instance actions: Start / Stop / Restart / Update / Remove (with the volume-keep-or-discard choice surfaced, since `uninstall` already has backup behavior) / Open UI / Logs (with copy).
- "+ New Deck": name + port (pre-filled via the CLI's own suggestion logic) + optional image/memory override, calling `install`.

### 5. App chrome / navigation

- Persistent sidebar or top bar, always visible regardless of which view is active:
  - **Dashboard** (home) — the Decks list.
  - **Open instances** — one entry per instance the user has opened, each a tab/webview pointed at that instance's own port. Clicking "Open UI" on a dashboard row adds/focuses this tab rather than replacing the dashboard.
  - **Help** — opens the docs website via Tauri's shell-open (external browser, not embedded).
  - **Community** — opens the Slack invite, same mechanism.
  - **Settings** — advanced-logging toggle, CLI version/contract info, migration status/actions.

### 6. Dependency install (unchanged responsibility, new implementation)

- Port the bootstrap half of `runtime.cjs` — `findExecutable('podman')`, per-OS install (`pkexec` + apt/dnf/pacman/zypper/apk on Linux), `ensureWindowsPrerequisites()` (WSL2), `ensureRuntimeReady()` (Podman machine init/start on macOS/Windows) — to Rust in `bootstrap.rs`. This is a mechanical port of existing, working logic, not a redesign; the CLI deliberately has no equivalent (`desktop-cli-unification/SPEC.md`: "No podman/docker install logic, no `podman machine` management").

### 7. CLI unification (adopt, don't redefine)

Everything about the JSON/NDJSON contract, sidecar embedding, multi-instance registry, and version-compatibility guard is inherited wholesale from `desktop-cli-unification/{ISSUE,SPEC}.md`. `cli_bridge.rs` has the exact same responsibilities as that spec's `cli-bridge.cjs`: own the subprocess spawn, parse/validate JSON against `jsonContract`, throw/return a distinguishable error for a contract mismatch, and carry zero podman/docker knowledge of its own.

## Sequencing

1. **CLI JSON mode** (`omnideck-dev/cli` repo) — `--json` for `status`, `list`, `doctor`, `logs`, `--version`, plus the non-interactive-ambiguity fix. Spec'd in `desktop-cli-unification/SPEC.md` §1. **Done, not just planned**: `cli/cmd/jsonout.go`, `--json` wired in `root.go`, JSON support in `status.go`/`list.go`/`doctor.go`, matching tests, and `cli/docs/JSON_MODE_SPEC.md` all exist in the CLI repo now. **This means step 2 below is the actual starting point for implementation, not step 1.**
2. **Tauri mechanics validation** — a small spike, scaffolded fresh in this repo (the original `tauritest/` scratch directory no longer exists — see `reference/tauritest-SPEC.md` for its design and code samples to build from) proving sidecar spawn + event streaming + webview/iframe embedding of the app's own React UI (including its WebSocket browser-control channel) on all three target OSes. Point it at the CLI's new JSON output from the start rather than plain-text scraping, to avoid throwaway work.
3. **Read-only dashboard** — real app shell (nav chrome, Dashboard view, instance detail) against the CLI JSON contract: list/status/logs only, no lifecycle actions yet. Validates the end-to-end data path before touching anything destructive.
4. **Lifecycle actions** — wire Start/Stop/Restart/Update/Remove/Create through `cli_bridge.rs`.
5. **Environment bootstrap port** — `bootstrap.rs`, replacing the assumption ("assume podman/docker already installed") that both `tauritest/SPEC.md` and a bare CLI make.
6. **Onboarding polish** — instant-open shell, tips/video panel, advanced-logging drawer.
7. **Migration** — see below; gate behind a feature flag until steps 1–5 are solid.
8. **Packaging + cutover** — Tauri bundler output (`.dmg`/`.exe`/`.deb`) reaches parity with today's `desktop.yml` release workflow; retire Electron (remove `electron`/`electron-builder`/`@electron/notarize` deps, delete `desktop/src/*.cjs`, update `.github/workflows/desktop.yml`) only after real-world validation on all platforms.

## Migration

`desktop-cli-unification/SPEC.md` §6 already specifies migrating the Electron app's isolated single-instance storage into a named CLI-managed instance (verify-before-delete, volume export/import). **This plan's migration is a superset of that**, because it also involves replacing the Electron *application* itself with a different (Tauri) one:

- The storage-model migration (legacy `omnideck-desktop` container/volumes → a named Deck under default podman storage) happens exactly as already specced, regardless of which app performs it.
- Additionally: users with the old Electron app installed need a path to the new Tauri app (new installer, possibly uninstalling the old one) without ending up with two apps or losing "already set up" state. Sequence: Electron app's last release detects it's outdated and points users to the new Tauri installer (or auto-updates if that infrastructure exists by then) → Tauri app performs the storage migration on first launch per the existing spec.
- Treat this the same way `desktop-cli-unification/SPEC.md` already treats its migration step: the highest-risk part of the project, verify-before-delete non-negotiable, no silent data loss.

## Risks / open questions

- **Two upstream prerequisites** (CLI JSON contract, Tauri mechanics spike) must both land before the real dashboard work starts — a slip in either delays everything downstream.
- **Dynamic per-instance ports** mean the CSP `frame-src`/`connect-src` must allow `http://localhost:*`/`ws://localhost:*` generally (`tauritest/SPEC.md` §8 already does this), not one hardcoded port — verify this holds once multiple instances are opened simultaneously.
- **iframe vs. multi-webview** for embedding instance UIs — start with iframe per `tauritest/SPEC.md` §5.1 (simplest), fall back to Tauri `WebviewWindow`/multi-webview if CSP or WebSocket issues appear (the app's browser-control feature depends on WebSockets — test this specifically, it's flagged as a real risk in `tauritest/SPEC.md`'s risk table).
- **Bootstrap port to Rust** is the single biggest Tauri-specific engineering risk beyond what `desktop-cli-unification` already covers — multiple Linux package managers, `pkexec` elevation, WSL2, and Podman machine lifecycle all need re-verification in Rust, not just direct translation.
- **Tips/video panel and advanced-logging drawer are new UI surface** with no reference implementation in either prior doc — keep v1 simple (static tip list, plain-text log drawer) per the "functional over polished" bar `desktop-cli-unification/ISSUE.md` already set for the dashboard.
- **App-switch migration** (Electron → Tauri as two different installed applications) is a distribution/UX problem on top of the already-planned data migration — needs its own decision on installer/update mechanics before Phase 8.
- **No update-mechanism equivalent exists yet.** The current Electron app has no `electron-updater` dependency — its update check is a hand-rolled GHCR-tag-polling implementation (`desktop/src/updates.cjs`, `update-state.cjs`), not something Tauri's built-in updater plugin can drop in for as-is. Non-goals already excludes auto-update from v1, but this needs an explicit decision (port the GHCR polling logic, adopt Tauri's updater plugin, or defer entirely) before it becomes a silent gap at cutover time.
- **`tauritest/SPEC.md` was found deleted from disk** during this plan's verification pass, with no working POC code ever built from it — recovered from an old session transcript and now preserved at `reference/tauritest-SPEC.md` in this repo (see "Reference docs" above) specifically so this doesn't happen again.

## Testing

Inherits `desktop-cli-unification/ISSUE.md`'s testing section (CLI golden-file/snapshot tests, bridge-module tests against canned JSON fixtures, real-binary+real-podman integration tests for concurrent instances, migration simulation, manual side-by-side verification against `podman ps`/CLI TUI). Adds, Tauri-specific:

- Cross-platform sidecar spawn test (Windows/macOS/Linux).
- CSP/iframe/WebSocket smoke test per platform for the embedded instance webview.
- A startup-latency regression guard: window must be visible within a small, explicit time budget of process start on every platform, showing real state — this directly guards against the exact failure mode that started this investigation.

## Acceptance criteria

Everything in `desktop-cli-unification/ISSUE.md`'s acceptance criteria, plus:

- [ ] App window is visible within the agreed time budget on all platforms, always showing real progress state — never a blank/frozen window.
- [ ] Long-running installs show tips/video by default; enabling "advanced logging" reveals the full raw CLI/container output live.
- [ ] App chrome provides one-click access to the Dashboard, every open instance, Help, and Community from any view.
- [ ] Packaged app size and cold-start time are measured and reported against the current Electron build.
- [ ] Existing Electron installs migrate (data + app) with no silent loss, per the migration section above.
