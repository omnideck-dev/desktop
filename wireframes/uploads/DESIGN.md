# Omnideck Desktop (Tauri) — wireframe brief

## Purpose

This doc enumerates every screen/state the Tauri rewrite needs wireframed, so design can proceed in parallel with the Rust/React build-out. It's a companion to [`desktop_tauri_rewrite.md`](./desktop_tauri_rewrite.md) (the engineering plan — read that first for full context on why this app exists and what it talks to) and to the data contract in `reference/` and the CLI's `docs/JSON_MODE_SPEC.md`. Every field named below in a "Data" line is a real field from that JSON contract, not a placeholder — if a wireframe needs data not listed there, flag it, don't invent it.

## Visual language to build from

This isn't a blank slate. The current app already has a design system ("SIGNAL") and a working design-review process — extend both rather than starting over.

- **Tokens**: `omnideck/desktop/src/setup/setup.css` (in the monorepo) defines the full token set already in production use: surface colors (`--canvas`/`--surface`/`--elevated`), text hierarchy (`--text-primary`/`-secondary`/`-tertiary`), semantic colors (`--accent`, `--success`, `--warning`, `--danger`, each with a `-muted` wash variant), a spacing scale (`--sp-1` through `--sp-12`, 4px base), a radius scale (`--radius-sm` through `--radius-full`), and two font stacks (`--font-brand` for the wordmark, `--font-body`/`--font-code` for everything else). Both a light theme ("Blueprint", default) and dark theme ("Terminal", `[data-theme="dark"]` or OS preference) are fully specified. New wireframes should reuse these tokens; if a new screen needs a color or spacing value that doesn't exist yet, call it out explicitly rather than picking a nearby hex value.
- **Review process**: `omnideck/desktop/mockups/setup-flow.html` (+ `.css`/`.js`) is a self-contained, no-build review harness the team already uses — it steps through every setup screen as data-driven states, with an in-page approve/feedback flow. Screens are declared as plain objects: `{ id, group, name, stage, eyebrow, title, detail, primary/secondary action labels, note }`. **Recommend extending this same pattern** (either this file directly, or a sibling `dashboard-flow.html` alongside it) for the new screens below, so engineering can read the approved states straight out of the same format it already parses, instead of translating from a different tool. Static mockups or Figma are fine for early exploration, but land the final, approved states in this format before implementation starts.
- **Existing screens being ported, not redesigned**: the 9 states already defined in `setup-flow.js` — `welcome`, `preparing`, `permission`, `finishing`, `ready`, `update`, `update-finishing`, `update-ready`, and 8 `doctor-*` error variants (`support`, `components`, `permission`, `download`, `environment`, `release`, `startup`, `restart`, `unknown`) — cover first-run dependency install and error diagnosis. These carry forward into the Tauri app close to as-is (see "Onboarding" below for the deltas multi-instance actually requires). Don't re-wireframe them from scratch; start from the existing ones and mark up what changes.

## Window constraints

Not a responsive web page — a fixed desktop window. Target `1200×800` default, `900×600` minimum (matching the existing Tauri POC spec's config). Design for that minimum width holding up, not just the default. Flag if any screen (especially the Dashboard table) needs a distinct narrow-width layout below some breakpoint. Also flag a decision needed on window chrome: native OS title bar per-platform (simplest, matches current Electron app) vs. a custom title bar (needed only if we want the app-chrome nav bar to sit flush with the top of the window) — this changes the top ~40px of every screen below.

## Navigation map

```
Launch
  │
  ├─ No instances yet ──────────────► Onboarding (first-time setup)
  │                                         │
  │                                    (ready) ──► Dashboard
  │
  └─ Instance(s) exist ─────────────► Dashboard  ◄──────────────────────┐
                                        │  │  │  │                       │
                          "+ New Deck" │  │  │  └─ row action: Logs ─► Logs panel
                                       │  │  └─ row action: Remove ─► Remove confirm
                                       │  └─ row action: Open UI ──► Instance webview tab
                                       ▼
                                 New Deck form ──(progress)──► back to Dashboard

App chrome (persistent, wraps all of the above except Onboarding):
  Dashboard | [open instance tabs] | Help (external) | Community (external) | Settings
                                                                                 │
                                                                    Advanced-logging drawer (overlay, any screen)
                                                                    Legacy-migration prompt (one-time, overlay)
                                                                    CLI version/contract mismatch (blocking, replaces chrome)
```

## Wireframes needed

Each entry: purpose, states, the data it's driven by (field names from the JSON contract), primary actions, and open questions for design/product to resolve. States marked **(reused)** already exist in `setup-flow.js` and just need a copy pass, not a new wireframe.

### 1. App chrome / persistent shell

- **Purpose**: always-visible nav so any view is one click from any other.
- **States**: Dashboard active (no instance tabs open) · Dashboard active with N instance tabs open · an instance tab active · Settings active · advanced-logging indicator on (subtle, doesn't need its own full state).
- **Elements**: wordmark/mark, nav list (Dashboard, one entry per open instance tab — closable — Help, Community, Settings).
- **Actions**: click switches the content pane; instance tabs close independently of stopping the underlying Deck; Help/Community open the user's default browser, never load in-window.
- **Open question**: how do many open tabs behave — wrap, scroll, or overflow menu? No hard cap exists on how many Decks a user can open at once.

### 2. Onboarding (first-time setup) — deltas on top of the reused screens

- **Reused (with copy pass)**: `welcome` (reused), `preparing` (reused), `permission` (reused), `finishing` (reused), all 8 `doctor-*` variants (reused). Copy needs review wherever it currently implies a single app instance (e.g. `ready`'s "omnideck is ready" should route into the Dashboard now, not directly open the app).
- **New**: 
  - `tips-panel` variant of `preparing`/`permission` — a second panel (rotating tips and/or a short embedded video) shown during genuinely long waits. Net new; nothing like it exists today (the current game panel — Agent Dash — can be kept, folded in as one tip-panel option, or dropped; that's a product call, not a design constraint).
  - `advanced-log-open` variant — a collapsible drawer showing raw stdout/stderr, reachable from any onboarding screen once the toggle exists in chrome/settings. Off by default.
- **Data**: same state shape `runtime.cjs`/the Rust equivalent already emits — `stage`, `title`, `detail`, `progress`, `indeterminate`, `activity`, `primaryAction`/`Label`, `secondaryAction`/`Label`, `diagnostics`, `diagnosticResult`, `technical`.

### 3. Dashboard (Decks list) — default/home view

- **Purpose**: the multi-instance list this whole rewrite exists to add.
- **States**: empty (zero Decks — reachable if a user removes their last one, not just pre-onboarding) · loading (first paint / post-action refresh) · populated (1..N rows) · background-refreshing (polled every 2–3s — must not flicker/reset scroll position or button focus while refreshing) · engine-unreachable error (podman/docker stopped after the app was already past onboarding).
- **Data** (from `list --json`, one row per entry): `name`, `status` (`running`/`exited`/`paused`/`unknown` → status-dot color/label), `webUiPort`, `cpu`/`cpuPct`, `ram`/`ramTotal`/`ramPct`, `uptime`, `restarts`, `health` (`""`/`healthy`/etc.), `created`, `image`. **The five live-stat fields plus `uptime`/`restarts` are explicitly `null` for any non-active instance** — design a real dash/placeholder treatment for that, not a "0%"/"0" that reads as a live zero reading.
- **Row actions**: Open UI, Logs, Start/Stop/Restart, Update, Remove — likely needs an overflow/kebab menu given the count; primary 1–2 actions (Open UI, Start/Stop) can stay inline.
- **Entry point**: prominent "+ New Deck" (button or card in the empty state).
- **Open question**: does a stopped/unhealthy Deck need any different row treatment (e.g. muted row, inline "repair" CTA) beyond the status dot?

### 4. New Deck form

- **Purpose**: create an additional Deck (`omnideck add`/`install`).
- **States**: prefilled default (name/port pre-filled from `add --suggest-defaults --json`) · client-side validation error (mirror the CLI's own rules before submit — see below) · submitting/progress (NDJSON stages: `check_availability` → `create_home_volume` → `create_state_volume` → `pull_image` [has a raw progress `detail` string, forwarded verbatim from podman/docker — render like the existing `preparing` screen's activity line, don't try to parse it as structured] → `run_container` → `save_config`) · success (return to Dashboard, new row highlighted) · error (any stage can end in a structured error — same envelope/copy pattern as the `doctor-*` screens).
- **Fields**: name, port, optional image override, optional memory override.
- **Client-side validation to mirror** (`JSON_MODE_SPEC.md` §4 `config show`/`config set` rules): port is 1–65535; memory/shm-size are a positive number + unit (`2g`, `512m`); name/volume-name fields start with a letter or number, then letters/numbers/dots/underscores/hyphens only.

### 5. Remove Deck confirmation

- **Purpose**: `remove` is destructive; the CLI requires explicit, non-defaulted choices here (`--yes` plus exactly one of keep/delete-volumes, plus backup/no-backup if deleting) — the UI needs to force the same explicit choices, no pre-selected default.
- **States**: choice step (keep vs. delete volumes; if delete, backup vs. no backup) · in-progress (stages: `prepare` → `stop_container` → [`backup`] → `remove_container` → [`delete_volumes`], only the applicable ones shown) · success (surface `backupPath` if a backup was made, with a "reveal in file manager" action — there's no flag to choose the backup location, it always lands in the user's home directory) · error.

### 6. Instance detail (expanded row / drill-in)

- **Purpose**: everything the row doesn't have room for.
- **Elements**: full stat set, last N log lines, health/doctor status, the same lifecycle actions as the row plus a repair CTA when `health` or `doctor` reports a failure.
- **Data**: same `list --json` fields as the row, plus `doctor --json`'s per-check shape (`label`, `status` [`pass`/`fail`/`warn`/`info`], `detail`, `hint`, optional `action`/`actionLabel`/`actionValue`) when a check fails.
- **Open question**: the plan doc mentions "resource history" here — the CLI has no historical stats endpoint, only a live snapshot per poll. If a sparkline/trend is wanted, it'd have to be built from client-side accumulation of poll results (lossy, resets on app restart). Worth confirming this is acceptable, or scoping history out of v1 (the engineering plan's non-goals already exclude "resizable graphs/sparkline history" — recommend design treat this as out of scope for now too, current-value display only).

### 7. Instance webview tab

- **Purpose**: chrome around the iframe/webview loading `http://127.0.0.1:<webUiPort>` for that Deck.
- **States**: loading (iframe hasn't painted yet) · loaded · connection-refused (Deck was stopped/removed while its tab was open) · closable via an `×` on the tab itself.
- **Note**: this is mostly tab-chrome, not page content — the page content is the Deck's own web app, out of this doc's scope.

### 8. Logs panel (historical, per-Deck)

- **Purpose**: `logs --json`/`logs --follow --json` viewer, reachable from a Dashboard row or Instance detail.
- **Elements**: tail-length control, monospace log lines, copy-to-clipboard, a live/follow toggle.
- **Data**: non-follow is `{"lines": [...]}`; follow is NDJSON `{"line", "ts"}` per line, ends when the process is killed (no in-stream "done" event to design around).

### 9. Advanced-logging drawer

- **Purpose**: raw, live stdout/stderr of whatever CLI subprocess is currently running — distinct from #8 (that's historical container logs; this is the current CLI process's own output). Off by default, toggled in chrome/Settings.
- **States**: closed (default) · open, idle (no operation currently running — "Nothing running") · open, streaming (raw text, append-only) · copy-to-clipboard / "open log file" actions.

### 10. Settings

- **Elements**: advanced-logging toggle, CLI version/contract info (`version`, `commit`, `date`, `jsonContract` from `omnideck --version --json`) with a clear warning state if `jsonContract` doesn't match what this app build expects, migration status/actions (see #11).

### 11. Legacy migration prompt (Electron → Tauri, one-time)

- **Purpose**: users with the old Electron app's isolated single-instance data need a non-silent, verify-before-delete migration path.
- **States**: detected (legacy instance found, prompt to migrate now or later) · in-progress (export → create → import → verify-before-delete stages — mirror the New Deck form's progress treatment) · success · failure/rollback (must reassure the user their original data is untouched — this is the highest-risk flow in the whole rewrite per the engineering plan, the copy needs to earn trust, not just report a status) · declined (remembered — don't re-prompt every launch) · already-migrated (a quiet info line in Settings, no action).
- **Open question for product**: if declined, is the user asked again later (e.g. once more after N launches) or never again until they open Settings manually?

### 12. Blocking states (replace the whole shell, not overlays)

- **CLI missing / version-contract mismatch**: the app can't safely talk to the CLI at all. Needs its own blocking screen, not just an error toast — this is a harder failure than any per-Deck error.
- **Engine (podman/docker) unreachable after onboarding**: softer than the above — Dashboard should degrade gracefully (see #3's engine-unreachable state) rather than block the whole app, since the user may just need to restart their container runtime.

## Explicitly out of scope for this wireframe round

Mirrors the engineering plan's non-goals — don't spend design time here yet: resizable graphs/sparkline resource history, drag-to-reorder Decks, per-Deck theming, a full interactive terminal/PTY (the advanced-logging drawer is read-only, append-only text), system tray, in-app auto-update UI, code-signing/notarization UX.

## Deliverable format

For each screen above, at minimum: the populated/default state and every explicitly-listed error/empty state — don't skip error states to save time, they're at least half the list above. Where a screen is list-like (Dashboard), show an empty, a typical (3–5 rows), and a many-items (10+, scrolled) variant. Use the same `{ id, group, name, ... }` shape the existing `setup-flow.js` already uses so engineering can wire approved states directly.
