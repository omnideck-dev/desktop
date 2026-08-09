# Omnideck Desktop

The Omnideck desktop app — a Tauri v2 + React/TypeScript rewrite that replaces the old Electron app with a thin GUI shell over the [`omnideck` CLI](https://github.com/omnideck-dev/cli). See [`AGENT.md`](./AGENT.md) for the full architecture, non-negotiable rules, and current build status, [`desktop_tauri_rewrite.md`](./desktop_tauri_rewrite.md) for the "why" and the sequencing plan, and [`reference/desktop-hardening-migration-PLAN.md`](./reference/desktop-hardening-migration-PLAN.md) for the sidecar-pinning, capability-hardening, and onboarding-window work.

## Roadmap

See the public [Omnideck Roadmap](https://github.com/orgs/omnideck-dev/projects/1) board for what's planned, in progress, and recently shipped.

## Architecture at a glance

Two windows, two frontend stacks, deliberately:

- **`"main"`** — the React/TypeScript dashboard. Lists Decks, drives Start/Stop/Restart/Update/Remove/Logs, streams `add`/`update`/`remove` progress. Always created and shown immediately on launch (never blank — see `AGENT.md`'s "instant open" rule).
- **`"onboarding"`** — an isolated, vanilla-JS/HTML/CSS window (`public/onboarding/`) that drives first-run/repair setup of the *shared* Podman runtime via a narrow 4-command bridge (`bootstrap`/`begin_setup`/`open_dashboard`/`run_action`, scoped to its own `onboarding-bridge` Tauri capability — never reachable from `"main"`). Created hidden; only shown when the runtime actually isn't ready. See `src-tauri/src/bootstrap.rs`'s module doc comment for the full design rationale.

Both talk to the bundled `omnideck` CLI sidecar (never PATH) through `src-tauri/src/cli_bridge.rs`, which owns 100% of subprocess spawn + JSON/NDJSON parsing.

## Prerequisites

- **Node.js** 18+ and npm
- **Rust** (stable) — `cargo`, `rustfmt`, `clippy`
- **Tauri v2 Linux build deps** (Linux only — macOS/Windows just need Xcode CLT / the Visual Studio C++ build tools per [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/)): `webkit2gtk-4.1`, `gtk3`, `librsvg2`, `openssl`, `libappindicator-gtk3`, `patchelf`
- **The bundled CLI sidecar**, fetched once per checkout — pinned by version + checksum against a real [`omnideck-dev/cli`](https://github.com/omnideck-dev/cli) release, not resolved from PATH:
  ```bash
  npm run fetch:sidecars      # downloads + verifies all 6 target triples into src-tauri/binaries/
  npm run verify:sidecars     # re-checksums without re-downloading (cheap, e.g. in CI)
  ```
  `src-tauri/binaries/vendor-manifest.json` records the pinned tag/commit/checksums (committed); the fetched binaries themselves are gitignored. `OMNIDECK_CLI_ARCHIVE_DIR=/path/to/archives` points `fetch:sidecars` at pre-downloaded release archives for an offline/sandboxed build — the pinned hashes are still enforced either way.
- **Podman**, installed — needed for the dashboard to show real Deck data and for the onboarding flow's "already ready" path to actually be ready. If Podman genuinely isn't ready, the app's own onboarding window is what sets it up; you don't need to pre-provision it by hand to develop against this repo, just to see the dashboard's populated state instead of an empty list.

### Linux on an immutable/atomic distro (Fedora Silverblue, Bluefin, etc.)

The base system deliberately has no compiler toolchain or GTK/WebKit dev headers, and shouldn't get them via `rpm-ostree install`. Do it in a toolbox instead:

```bash
toolbox create omnideck-dev   # once
toolbox run -c omnideck-dev sudo dnf install -y \
  rust cargo clippy rustfmt webkit2gtk4.1-devel gtk3-devel librsvg2-devel \
  openssl-devel libappindicator-gtk3-devel patchelf file curl wget nodejs npm go
```

**Don't hand-roll `toolbox run ... tauri dev`.** The compiled binary must run directly on the host, not inside the toolbox — a toolbox has no working `podman` of its own, and even the host's `podman` reached via the toolbox's bind mount crashes there (missing namespace/capability access). `npm run dev:app` (see below) handles this automatically: it builds inside the toolbox, then runs the resulting binary directly on the host, with Vite also on the host. This was learned the hard way — see `AGENT.md`'s "Linux dev environment" note.

## Development

```bash
npm install
npm run fetch:sidecars   # once per checkout, or whenever vendor-manifest.json changes
npm run dev:app          # full app: Vite dev server + Rust backend, hot reload
npm run dev               # frontend only, no Rust/webview (fast iteration on React/CSS)
```

`npm run dev:app` is the correct entrypoint on every platform — it's a thin wrapper (`scripts/dev.sh`) that only does the toolbox dance on Linux; macOS/Windows/non-atomic-Linux just get `npm run tauri dev` directly. Don't run `npm run tauri dev` by hand on an atomic-Linux dev box.

The dashboard window opens immediately showing a real "Checking your setup…" state, then either the populated Dashboard or a blocking screen if the CLI sidecar is missing/version-mismatched — see `AGENT.md`'s "instant open" rule if you're touching startup code. If the shared Podman runtime isn't ready yet, the onboarding window appears instead; see below for how to preview its screens without needing an actually-unprovisioned machine.

### Testing the onboarding flow

Onboarding only shows up for real when Podman genuinely isn't ready — which, on a dev machine, is normally never (you already have Podman set up), and reaching that state for real means partially uninstalling your own working environment. Don't do that. Instead:

```bash
OMNIDECK_DEBUG_ONBOARDING_STAGE=welcome npm run dev:app
OMNIDECK_DEBUG_ONBOARDING_STAGE=preparing npm run dev:app
OMNIDECK_DEBUG_ONBOARDING_STAGE=ready npm run dev:app
OMNIDECK_DEBUG_ONBOARDING_STAGE=error npm run dev:app
```

Each value forces the onboarding window to open showing that exact screen — real render, real buttons, no real Podman calls made for the check itself. `welcome`'s "Set up Omnideck" button still calls the real `begin_setup`, though: since your Podman is presumably actually ready, that resolves to the real `ready` state almost instantly, which is a nice free integration check of the real `runtime ensure` idempotent-no-op path.

This is `debug_forced_state()` in `src-tauri/src/bootstrap.rs`, `#[cfg(debug_assertions)]`-gated — the function and the env var read don't exist at all in a release build (`cargo build --release`/`tauri build`), so there's no flag to accidentally ship enabled.

If you need to test the *real* first-run install path (not just the screens), the safe way is to make the CLI sidecar unable to find Podman without touching your actual install — e.g. temporarily prepend an empty directory to `PATH` before launching, so `podman` isn't found on this run only, rather than uninstalling anything. Expect this to actually attempt a real install if you click through past "Welcome," so only do it on a machine/VM you don't mind that happening on.

## Testing

- **Composite gate**: `npm run verify` — fetches/verifies sidecars, runs the policy tests, typechecks, and runs `cargo fmt --check`/`cargo test`/`cargo clippy -- -D warnings`. This is what CI will run once this repo has CI (see the hardening plan's Phase 6).
- **Frontend**: `npm run build` (`tsc` + `vite build`) or `npm run typecheck` (`tsc --noEmit` only, faster).
- **Backend**: from `src-tauri/`:
  ```bash
  cargo build
  cargo test              # cli_bridge.rs + bootstrap.rs fixture/unit tests — no real CLI/podman needed
  cargo clippy -- -D warnings
  cargo fmt
  ```
- **Policy tests** (`npm run test:policy`, `tests/policy.test.mjs`): security-posture assertions on the capability files, the onboarding window's authorization checks, the CLI sidecar pin, and the process hardening (output bounds, timeouts) — so a future PR can't silently widen the attack surface without a test failing. Uses Node's built-in test runner, no extra dependency.
- **Manual/integration verification against the real CLI** — the most reliable way to confirm a `cli_bridge.rs` change actually matches what the CLI emits, since fixture tests only catch drift from *known* shapes:
  1. Run the CLI command by hand with the exact same args your Rust code constructs, e.g. `src-tauri/binaries/omnideck-<your-triple> add --name test --port 46177 --json`, and diff the output against what your Rust structs expect.
  2. For anything destructive (`remove`, or actions against instances you care about), test against a disposable instance you create and remove yourself, or a low-stakes existing one — never a production Deck. `omnideck list --json` shows what's currently installed before you touch anything.
  3. Run `npm run dev:app` and exercise the flow in the actual window (Start/Stop/Restart, New Deck, Remove, Logs) — typechecking and fixture tests verify shapes, not that the UI actually calls the right command with the right args at the right time.
- **Manual release procedures** (`tests/manual/*.md`): clean-first-run and recovery-lifecycle walkthroughs against a real packaged build (AppImage/deb/rpm/dmg/nsis), for behavior hosted CI can't safely or reliably prove — runtime installation, restart/resume, destructive recovery.
- **Packaged-build smoke check** (`tests/hardware/validate-proof.mjs`): proves a real installed build actually exercised a real (not fixture) dependency check on a representative OS before release. See `tests/hardware/README.md`.
