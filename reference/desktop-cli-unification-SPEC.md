# SPEC: Desktop delegates to CLI

Companion detail doc for [`ISSUE.md`](./ISSUE.md). This is implementation guidance, not a contract set in stone — deviate where the code disagrees with what's described here, and update this file when it does.

## Current state (as of this writing)

### `omnideck-dev/cli` (Go, sibling repo at `../cli` relative to this repo)

- Commands: `install` (`--plain` for non-interactive), `start`, `stop`, `restart`, `update`, `status`, `logs`, `doctor`, `uninstall`, `config {show,set,path}`, `tui`.
- `--name`/`-n` global flag selects an instance; with no instances or exactly one, it's inferred. With multiple and no `--name`, commands needing a config call `requireConfigMulti()` which opens an interactive picker — **not safe for headless/scripted use as-is** (see JSON mode section).
- Instance config: `config.Config` struct, YAML at `~/.config/omnideck-cli/instances/<name>.yaml` (`container_name`, `memory`, `shm_size`, `web_ui_port`, `engine`, `image`, `installed_at`, optional `home_volume`/`state_volume` overrides). `config.ListInstances()` enumerates all of them.
- `engine.Engine` interface (`engine/engine.go`) already has everything a dashboard needs: `ContainerStatus`, `ContainerStats` (CPU %, mem used/total), `FetchLogs`, `ContainerInspect` (started/created/restarts/health). This is real, already-plumbed data — it just isn't exposed as JSON yet.
- No `--root`/`--runroot`/XDG overrides anywhere — always operates against the ambient/default podman or docker the host already has configured.
- No podman/docker install logic, no `podman machine` management. `doctor` diagnoses and reports; it doesn't fix.
- Release pipeline (`.github/workflows/release.yml`, `Makefile`): on `v*` tag push, builds `linux/{amd64,arm64}`, `darwin/{amd64,arm64}`, `windows/amd64`, archives as `omnideck-<os>-<arch>.{tar.gz,zip}`, attaches to the GitHub release. This is directly reusable for embedding.

> **Note found during investigation:** the CLI binary installed locally (v0.9.0) exposed a top-level `list` command and an `instance remove` subcommand in `--help`, but the checked-out repo `HEAD` at investigation time did not have literal `list`/`instance` commands in `cmd/*.go` — `status`/`uninstall` appear to be the current equivalents. Verify actual command names against `cli` repo `HEAD` at implementation time rather than trusting either source here; this doc may describe either the released or in-development shape depending on when it's read.

### `omnideck` desktop app (`desktop/src/runtime.cjs`, this repo)

- `OmniDeckRuntime` class does two genuinely different jobs in one file:
  1. **Environment bootstrap** (keep as-is): `findExecutable('podman')`, `installRuntime()`/`installRuntimeOnLinux()` (pkexec + apt/dnf/pacman/zypper/apk), `ensureWindowsPrerequisites()` (WSL2), `ensureRuntimeReady()` (`podman machine init/start` on macOS/Windows).
  2. **Instance lifecycle** (replace with CLI calls): `ensureImage`, `ensureContainer`, `startContainer`, `containerInfo`, `waitForApp`, plus the hardcoded `CONTAINER_NAME = 'omnideck-desktop'`, `HOME_VOLUME`, `STATE_VOLUME` constants.
- Runs podman with `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_CACHE_HOME` overridden to `userData/runtime/{data,config,cache}` (`runtimeEnv()`) — this is what makes its storage invisible to everything else. This override goes away for instance-lifecycle calls; it may still be useful to keep isolated `REGISTRY_AUTH_FILE`/pull-cache behavior scoped to the app, worth a second look once the CLI bridge is in place (not blocking).
- Single hardcoded container name/port means today's desktop app is architecturally a single-instance tool wearing a multi-instance-shaped download page (`INSTALLERS`, `MACHINE_NAME`, etc. all already keyed generically — the constraint is purely `CONTAINER_NAME`/`HOME_VOLUME`/`STATE_VOLUME` being module-level constants instead of per-instance values).
- Local userData layout observed: `~/.config/omnideck/` (Electron `app.setName('omnideck')`, `main.cjs:157`) with a `runtime/` subdir holding the isolated podman storage plus `app-port`, `setup-state.json`.

## Target architecture

```
Electron main process
 ├─ bootstrap.cjs  (renamed/trimmed runtime.cjs: podman/docker install, WSL2, podman machine)
 ├─ cli-bridge.cjs (new: spawns bundled `omnideck` binary, parses JSON/NDJSON)
 └─ instances.cjs  (new: desktop-side view model — list, create, select active Deck)
        │
        ▼
   bundled `omnideck` binary  (per-platform, pinned by version+sha256)
        │  shells out to
        ▼
   podman / docker  (ambient, default storage — same one `podman ps` sees)
```

The renderer (dashboard UI) never talks to podman or the CLI binary directly — only through IPC to the main process, same pattern the app already uses for setup state.

## 1. CLI: headless/JSON mode

Add a package-level `--output json` (or reuse `-o`/an env var `OMNIDECK_OUTPUT=json`) that, when set, suppresses TUI/lipgloss rendering and prints exactly one JSON value (or one NDJSON line per event for streaming commands) to stdout. Keep stderr for actual errors/logs so stdout stays parseable.

Minimum required additions:

- **`status --name <x> --json`** → single JSON object:
  ```json
  {
    "name": "main",
    "container": "main",
    "status": "running",
    "image": "ghcr.io/omnideck-dev/omnideck:main",
    "engine": "podman",
    "webUiPort": "2337",
    "homeVolume": { "name": "main-home", "exists": true },
    "stateVolume": { "name": "main-state", "exists": true },
    "ollama": { "reachable": true, "host": "http://host.containers.internal:11434" }
  }
  ```
- **`list --json`** (new or renamed from whatever `HEAD` currently calls the all-instances view) → array of the above, each merged with live `ContainerStats`/`ContainerInspect` data already available in `engine.Engine`:
  ```json
  [
    { "name": "demo", "status": "running", "cpuPct": 0.0011, "cpu": "0.11%",
      "ramBytes": 204893, "ram": "195.4MB", "ramTotalBytes": 1073741824, "ramTotal": "1.074GB",
      "uptimeSeconds": 12660, "restarts": 0, "health": "", "createdAt": "2026-07-27T00:00:00Z",
      "webUiPort": "2338", "image": "ghcr.io/omnideck-dev/omnideck:latest" },
    { "name": "main", "status": "exited", "webUiPort": "2337", "image": "ghcr.io/omnideck-dev/omnideck:main" }
  ]
  ```
  This one call is almost the entire data source for the dashboard's list view — the screenshot's per-row CPU/MEM/uptime/image line maps directly onto it.
- **`logs --name <x> --tail N --json`** (non-follow) → `{"lines": ["...", "..."]}`, and `--follow --json` → NDJSON, one `{"line": "...", "ts": "..."}` per line, so the dashboard's log panel can stream without scraping ANSI-styled text.
- **`doctor --json`** → structured pass/fail per check, mirroring `tui.RunDoctorChecks` results (support/components/downloads/environment/release/startup-shaped, or whatever the current check IDs are — reuse `checks` package output, don't invent new categories).
- **`install`/`update` progress** → NDJSON progress events (`{"stage": "pulling", "detail": "...", "progress": 0.42}` etc.) on stdout when `--json`/`--plain` is combined with a streaming flag, so the dashboard can show a real progress UI instead of polling `status` in a loop. If this is too large a lift for v1, polling `status` after an async `install --plain &` is an acceptable fallback — call this out explicitly as a possible v1 cut.
- **Non-interactive safety**: any JSON-mode invocation with an ambiguous multi-instance target (`--name` omitted, >1 instance, no TUI picker possible) must **exit non-zero with a structured error**, never fall back to an interactive picker. `requireConfigMulti()`'s picker path needs a JSON-mode bypass.
- **Version contract**: add `omnideck --version --json` → `{"version": "...", "commit": "...", "jsonContract": 1}`. Bump `jsonContract` (a plain integer) whenever any JSON shape above changes incompatibly. This is what the desktop app's compatibility guard checks — not semver, which the CLI may bump for unrelated reasons.

Implementation note: these all read from the same `engine.Engine`/`config.Config` data already used by `tui`/`cmd`; this is a serialization layer, not new data plumbing. Keep JSON struct definitions in one new file (e.g. `cli/cmd/jsonout.go` or a small `cli/output` package) rather than scattering `encoding/json` calls through each command file.

## 2. Embedding the CLI in the desktop build

Mirror the existing pinned-image pattern (`desktop/scripts/prepare-runtime-image.cjs`, `INSTALLERS` map in `runtime.cjs`) rather than inventing a new one:

- New `desktop/scripts/prepare-cli-binary.cjs`, invoked at desktop build time with a CLI version tag (e.g. `node scripts/prepare-cli-binary.cjs v0.10.0`):
  - Downloads `omnideck-<os>-<arch>.{tar.gz,zip}` from the `omnideck-dev/cli` GitHub release for each of the platforms the Electron build targets.
  - Verifies against the release's published checksums file.
  - Extracts into `desktop/build/cli/<platform>/omnideck[.exe]`.
- `package.json` `build.extraResources` gains an entry mapping `build/cli` → `cli` (same shape as the existing `build/runtime` → `runtime` entry).
- `cli-bridge.cjs` resolves the binary path from `process.resourcesPath` (packaged) or `build/cli` (dev), analogous to how `releaseImage()` resolves `image-manifest.json` today.
- CI: desktop release workflow needs a pinned CLI version (a constant or a manifest file, e.g. `desktop/cli-version.json` — same idea as `image-manifest.json`) bumped deliberately per desktop release, not auto-tracking CLI `latest`. This keeps the two repos' release cadences decoupled but explicit.

## 3. `cli-bridge.cjs` (new)

Thin wrapper, similar shape to `OmniDeckRuntime.run()` today:

```js
async function cliJson(binary, args, env) {
  const { code, stdout, stderr } = await run(binary, [...args, '--json'], env);
  if (code !== 0) throw new CliError(parseErrorJson(stdout) ?? stderr, code);
  return JSON.parse(stdout);
}

// list(), status(name), start(name), stop(name), restart(name), update(name),
// logs(name, {tail, follow, onLine}), install(opts), uninstall(name, {keepVolumes})
```

Responsibilities:
- Own the subprocess spawn (reuse the existing `spawn`/log-capture plumbing in `runtime.cjs` rather than rewriting it).
- Parse and validate JSON against the expected `jsonContract` version; throw a distinguishable error type if the contract doesn't match, so the caller can show the "CLI needs updating" state instead of a generic crash.
- No podman knowledge at all — if this module ever constructs a `podman`/`docker` argv itself, that's a sign logic leaked back across the boundary; it shouldn't happen.

## 4. Multi-instance support in the desktop app

- Replace `CONTAINER_NAME`/`HOME_VOLUME`/`STATE_VOLUME` module constants with per-instance values sourced from `cli-bridge.list()`.
- "Create a Deck" flow: reuse the CLI's own name/port suggestion (`suggestNewInstanceDefaults` equivalent) — either expose it via a small `install --suggest-defaults --json` CLI addition, or just call `list --json` from the desktop side and replicate the "next unused name, port = max+1" logic client-side (simpler, avoids another CLI surface, and it's a few lines either way — the CLI repo's version is the source of truth for the exact algorithm, so link to `suggestNewInstanceDefaults` in the cli repo rather than re-deriving it independently if replicating).
- No new desktop-side persistence needed — `list --json` from the CLI's own instance registry is the single source of truth; the desktop app should not keep a second copy of instance metadata anywhere.

## 5. Decks dashboard (renderer)

v1 scope, matching the CLI TUI screenshot's information density rather than redesigning it:

- List of Decks, each row: name, status dot, port, CPU%, mem used/total, image (truncated), uptime — sourced entirely from one `list --json` call, polled (e.g. every 2–3s while the dashboard is open) or refreshed on window focus.
- Expand-per-row (or a detail pane): image, uptime, restarts, health, last N log lines (`logs --tail --json`, non-follow poll or a follow stream if IPC supports streaming events to the renderer).
- Actions per row, each a direct IPC call into `cli-bridge`: **Open UI** (opens `http://127.0.0.1:<port>` in the default browser or an in-app webview, matching current desktop behavior), **Logs** (opens full log view/streams), **Start/Stop/Restart**, **Update**, **Remove** (confirmation dialog — this maps to `uninstall`, which already has volume-backup behavior in the CLI; surface that choice in the confirmation dialog rather than silently keeping or discarding volumes).
- **"+ New Deck"**: name + port (pre-filled with suggested defaults) + optional image/memory override, calling `install`. Reuse the existing first-run setup wizard UI for the "collect these fields" step rather than building a second form.
- Explicitly out of scope for v1: resizable graphs/sparkline history, drag-reorder, per-Deck theming. The screenshot's flat info-dense layout is an acceptable v1 target, restyled to the desktop app's existing visual language rather than terminal styling.

## 6. Migration (existing alpha desktop installs)

On first launch of the delegating desktop app version, before anything else:

1. Check for the legacy isolated container: run `podman` (not the CLI) with the old `XDG_*` overrides pointed at `userData/runtime/{data,config,cache}`, `container inspect omnideck-desktop`. If absent, skip — nothing to migrate.
2. If present, **do not auto-migrate silently**. Show a screen: "We found an existing OmniDeck install. Migrate it into the new format?" with the container's current state (running/stopped) and volume sizes shown.
3. On confirm:
   a. Stop the legacy container if running.
   b. `podman volume export` both legacy volumes (already-proven code path — same primitive the CLI's `uninstall` backup uses) to a temp location, still using the old `XDG_*` env.
   c. Create new CLI-managed volumes (`<chosen-name>-home`/`-state`) under default storage via the CLI (`install --plain`), then `podman volume import` the exported tarballs into them, using default env this time.
   d. Verify the new instance starts and serves `/` before declaring success.
   e. Only then remove the legacy container/volumes (isolated storage). Keep the exported tarballs around (e.g. in `userData/migration-backup/`) for one more run as a safety net, and surface their location if the user asks or if step (d) fails.
4. On decline: leave the legacy install fully alone, keep the old code path (or a minimal read-only shim of it) available so it keeps working, and don't nag on every launch — ask once, remember the answer (`setup-state.json`-style flag), offer a manual "Migrate now" action from settings later.

This is the highest-risk part of this project because it's the only step that touches existing user data destructively. Treat steps 3d/3e ordering (verify-before-delete) as non-negotiable.

## 7. Version compatibility guard

At desktop startup, after resolving the CLI binary path and before any instance-lifecycle call: `omnideck --version --json`, check `jsonContract` against the value this desktop build was built against (baked in at build time from the same manifest used in step 2). Mismatch → dedicated error screen (reuse the existing `FAILURE_COPY`/diagnostic screen pattern in `runtime.cjs`), not a generic crash, with `canRetry: false` since retrying won't fix a version mismatch — the fix is a desktop update.

## Suggested implementation order

1. CLI: `--json` for `status`, `list`, `doctor`, `--version` (steps that don't touch install/update streaming) + the non-interactive-ambiguity fix. Ship as a CLI release.
2. Desktop: `prepare-cli-binary.cjs` + extraResources wiring + `cli-bridge.cjs` against the new CLI release, read-only (status/list/logs) — enough to build a read-only dashboard prototype and validate the JSON contract end-to-end before touching lifecycle actions.
3. Desktop: wire Start/Stop/Restart/Update/Remove/Create through the bridge; remove the podman-calling half of `runtime.cjs`.
4. Migration flow, gated behind a feature flag until steps 1–3 are solid.
5. Version compatibility guard (can land any time after step 1 defines `jsonContract`, but should land before general release).
6. CLI: streaming install/update progress, if not already done — otherwise ship v1 with the polling fallback noted in section 1.
