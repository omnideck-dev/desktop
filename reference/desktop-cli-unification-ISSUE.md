# Desktop app should delegate to the CLI instead of reimplementing it

**Repos affected:** `omnideck` (this repo, `desktop/`) and `omnideck-dev/cli` (sibling repo). This is a two-repo change — see "Repo split" below for how to sequence it. Full technical detail lives in [`SPEC.md`](./SPEC.md) alongside this issue; keep this issue as the high-level tracker.

## Problem

The desktop (Electron) app and the `omnideck` CLI are two independent implementations of "run the Omnideck container." They were built separately and have already drifted:

| | CLI (`omnideck-dev/cli`) | Desktop (`desktop/src/runtime.cjs`) |
|---|---|---|
| Podman storage root | user's default rootless storage (`~/.local/share/containers`) | isolated, via `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_CACHE_HOME` pointed at Electron's userData dir |
| Container naming | user-chosen (`main`, `demo`, `work`, ...) | hardcoded single name, `omnideck-desktop` |
| Volumes | `<name>-home` / `<name>-state` | hardcoded `omnideck-desktop-home` / `omnideck-desktop-state` |
| Instance registry | `~/.config/omnideck-cli/instances/<name>.yaml` | none |
| Multiple instances | supported (auto name/port suggestion) | not supported — one Electron install = one instance |
| Run flags | `--replace`, no version labels | `--rm`-then-create, version labels, fixed `--memory 2g` |

Practical result: a container/instance created by one tool is invisible to the other. `podman ps` and Podman Desktop don't show the desktop app's container. The CLI's `main` instance and the desktop app both default to port `2337`, which is a live collision. And every feature built into the CLI (multi-instance, status detail, doctor diagnostics) has to be rebuilt from scratch in the Electron app to reach parity — which is exactly the situation we're in now, needing a "Decks" dashboard.

## Goals

- One implementation of "create/start/stop/update/inspect/remove an Omnideck instance" — the CLI. The desktop app becomes a GUI front end over it, not a second implementation.
- A container/instance created from either the CLI or the desktop app is visible and manageable from both.
- The desktop app supports multiple named instances ("Decks") — home, work, a client, a side project — the same way the CLI already does.
- The desktop app ships a dashboard listing all installed Decks with live status/stats and Start/Stop/Restart/Update/Logs/Open/Remove actions per instance.

## Non-goals (this issue)

- Cross-device backup/sync/migration of instance data — tracked separately.
- Changing how the desktop app installs/bootstraps podman or docker itself (see "What does NOT move" below) — that stays as-is.
- A fully custom-designed dashboard UI. For v1, a plain/functional dashboard that mirrors the CLI's own TUI dashboard (data and actions, not necessarily visual polish) is the acceptance bar. Visual redesign is a fast-follow, not a blocker.
- Package-manager distribution changes for the CLI (Homebrew tap, etc.) — unaffected by this work.

## Proposed solution

Split responsibility along a clean seam:

- **Electron keeps owning environment bootstrap** — detecting/installing podman or docker, WSL2 setup on Windows, `podman machine` lifecycle on macOS/Windows, and the first-run "preparing your environment" UX. None of that logic moves; the CLI doesn't have (and doesn't need) an equivalent — it assumes a working container engine, same as it does for terminal users today.
- **The CLI becomes the only thing that runs `podman`/`docker` commands for instance lifecycle.** Once the engine is confirmed working, the Electron main process shells out to a bundled `omnideck` binary for every instance operation (install/start/stop/restart/update/logs/status/list/uninstall) instead of calling `podman` directly. This requires the CLI to gain a proper **headless/machine-readable output mode** (JSON), since today its output is styled for a terminal, not for a program to parse.
- The desktop app is built and shipped with the CLI binary embedded (per-platform, pinned by version + checksum), the same way it already embeds a pinned container image reference. No separate install step for end users.
- Desktop storage/config settles on the CLI's existing model: default podman storage root, `<name>-home`/`<name>-state` volumes, instances registered in `~/.config/omnideck-cli/instances/`. The desktop app's current isolated-storage single instance goes away, with a one-time migration for existing alpha installs (see Migration).

## High-level plan

1. **CLI: add headless/JSON mode.** `--json` output for `status`, a real `list` (all instances + live status/stats in one call), `doctor`, and structured/NDJSON progress events for the long-running `install`/`update`/`logs -f` commands. Define and version a stable JSON contract the desktop app can rely on. *(cli repo)*
2. **CLI: embed in desktop build.** New build step (mirrors the existing pinned-image-manifest pattern) that downloads, checksums, and bundles the right platform binary into the Electron app at build time. *(omnideck repo, `desktop/`)*
3. **Desktop: replace the podman-calling half of `runtime.cjs`** with a thin bridge that shells out to the bundled CLI's JSON commands. Keep the podman/docker/WSL2 bootstrap half unchanged.
4. **Desktop: multi-instance support.** Generalize the single hardcoded instance into "create/select/switch between named Decks," using the CLI's existing name/port auto-suggestion.
5. **Desktop: Decks dashboard.** List view of all instances with status, port, CPU/mem, image, and Start/Stop/Restart/Update/Logs/Open/Remove actions, backed by the CLI's `list`/`status`/`logs` JSON output. v1 can closely mirror the CLI TUI's own dashboard (screenshot attached to the originating conversation) rather than a bespoke design.
6. **Migration.** One-time, on first run of the new desktop version: detect the old isolated `omnideck-desktop` container/volumes, offer to migrate them into a named instance under the shared storage model (or leave in place with a clear "legacy install" notice if the user declines).
7. **Version compatibility guard.** Desktop checks the bundled/detected CLI's JSON-contract version at startup and shows a clear "needs update" state rather than failing silently if it's older than expected.

## Repo split

This spans two repos with independent release cadences:
- `omnideck-dev/cli`: steps 1 (JSON mode) — should ship and tag a release before desktop work depends on it.
- `omnideck` (`desktop/`): steps 2–7, consuming the CLI's tagged release.

File this as a tracking issue here, with a linked companion issue in `omnideck-dev/cli` scoped to step 1 only.

## Testing

- **CLI**: golden-file/snapshot tests for every new JSON output shape; a test asserting the JSON contract version is bumped whenever a shape changes.
- **Desktop unit**: the CLI-bridge module tested against canned JSON fixtures (no real podman/CLI needed) — covers parsing, error handling, and the version-compatibility guard.
- **Desktop integration**: a real bundled CLI binary + real podman, driving create → start → stop → update → remove for at least two concurrent named instances, confirming both `podman ps` and `omnideck list` (external CLI on PATH) agree with what the dashboard shows.
- **Migration test**: simulate a pre-refactor `omnideck-desktop` install (old isolated storage layout) and verify the migration path produces a working named instance with data intact, or a working legacy-mode fallback if the user declines.
- **Manual**: install two Decks side by side (e.g. "home" and "work"), confirm both are visible and independently controllable from the desktop dashboard, the CLI TUI, and raw `podman ps`/`podman volume ls`.

## Acceptance criteria

- [ ] A Deck created from the desktop app appears in `omnideck list` and `podman ps` without any special env vars.
- [ ] A Deck created from the CLI appears in the desktop dashboard.
- [ ] The desktop app can create, start, stop, restart, update, view logs for, and remove more than one Deck concurrently.
- [ ] No hardcoded single-container assumption remains in `desktop/src/runtime.cjs` or its replacement.
- [ ] Existing alpha desktop installs are migrated (or clearly flagged as legacy) on first launch of the new version, with no silent data loss.
- [ ] The desktop app's environment bootstrap flow (podman/docker install, WSL2, `podman machine`) is unchanged in behavior.
- [ ] Desktop startup fails gracefully with an actionable message if the bundled/detected CLI's JSON contract is incompatible.

## Risks / open questions

- Bundling a ~10 MB CLI binary per platform increases installer size — acceptable, but worth confirming against current installer size budget.
- Two-repo coordination means it's possible to ship a desktop build against a CLI JSON contract that later changes — the version guard in step 7 is what makes that safe, not optional polish.
- Migration touches real user data (volumes) on existing alpha installs — needs a dry-run/backup-first path, not a silent rewrite. This is the one step in this plan that's genuinely destructive if done wrong, and should get proportionate review/testing attention.
- Should the desktop app ever fall back to a system-installed `omnideck` CLI (e.g., a Homebrew install newer than the bundled one) instead of the bundled binary? Recommend: no for v1 — always use the bundled, version-pinned binary for predictability; revisit if it causes friction for CLI power users who also use the desktop app.
