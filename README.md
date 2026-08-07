# Omnideck Desktop

The Omnideck desktop app — a Tauri v2 + React/TypeScript rewrite that replaces the old Electron app with a thin GUI shell over the [`omnideck` CLI](https://github.com/omnideck-dev/cli). See [`AGENT.md`](./AGENT.md) for the full architecture, non-negotiable rules, and current build status, and [`desktop_tauri_rewrite.md`](./desktop_tauri_rewrite.md) for the "why" and the sequencing plan.

## Roadmap

See the public [Omnideck Roadmap](https://github.com/orgs/omnideck-dev/projects/1) board for what's planned, in progress, and recently shipped.

## Prerequisites

- **Node.js** 18+ and npm
- **Rust** (stable) — `cargo`, `rustfmt`, `clippy`
- **Tauri v2 Linux build deps** (Linux only — macOS/Windows just need Xcode CLT / the Visual Studio C++ build tools per [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/)): `webkit2gtk-4.1`, `gtk3`, `librsvg2`, `openssl`, `libappindicator-gtk3`, `patchelf`
- **The `omnideck` CLI**, built and on `PATH` — the Tauri sidecar isn't bundled yet (see `AGENT.md`), so local dev shells out to whatever `omnideck` resolves to. Build it from [`omnideck-dev/cli`](https://github.com/omnideck-dev/cli):
  ```bash
  git clone https://github.com/omnideck-dev/cli
  cd cli && git checkout <tag>   # e.g. v0.10.0-alpha.2, or main
  go build -ldflags="-s -w" -o omnideck .
  cp omnideck ~/.local/bin/      # or anywhere else on PATH
  omnideck --version --json      # sanity check — expect a jsonContract field
  ```
- **Podman or Docker**, running, if you want real instance data in the Dashboard rather than an empty list.

### Linux on an immutable/atomic distro (Fedora Silverblue, Bluefin, etc.)

The base system deliberately has no compiler toolchain or GTK/WebKit dev headers, and shouldn't get them via `rpm-ostree install`. Do it in a toolbox instead:

```bash
toolbox create omnideck-dev   # once
toolbox run -c omnideck-dev sudo dnf install -y \
  rust cargo clippy rustfmt webkit2gtk4.1-devel gtk3-devel librsvg2-devel \
  openssl-devel libappindicator-gtk3-devel patchelf file curl wget nodejs npm go
```

Then run every `cargo`/`npm run tauri *` command via `toolbox run -c omnideck-dev ...` (or `toolbox enter omnideck-dev` for an interactive shell). Plain frontend commands (`npm install`, `npm run build`, `npm run dev`) work fine on the bare host too, since they don't touch Rust or the system webview.

## Development

```bash
npm install
npm run tauri dev     # full app: Vite dev server + Rust backend, hot reload — needs the toolbox on Linux
npm run dev            # frontend only, no Rust/webview (fast iteration on React/CSS)
```

The window opens immediately showing a real "Checking your setup…" state, then either the Dashboard or a blocking screen if the CLI is missing/unreachable — see `AGENT.md`'s "instant open" rule if you're touching startup code.

## Testing

No end-to-end test runner is wired in yet (Vitest is the intended choice — not installed yet). Until then:

- **Frontend**: `npm run build` — runs `tsc` (typecheck) then `vite build`. Fails loudly on type errors.
- **Backend**: from `src-tauri/`:
  ```bash
  cargo build
  cargo test              # cli_bridge.rs fixture tests — canned JSON_MODE_SPEC.md payloads, no real CLI/podman needed
  cargo clippy -- -D warnings
  cargo fmt
  ```
- **Manual/integration verification against the real CLI** — the most reliable way to confirm a `cli_bridge.rs` change actually matches what the CLI emits, since the fixture tests only catch drift from *known* shapes:
  1. Run the CLI command by hand with the exact same args your Rust code constructs, e.g. `omnideck add --name test --port 46177 --json`, and diff the output against what your Rust structs expect.
  2. For anything destructive (`remove`, or actions against instances you care about), test against a disposable instance you create and remove yourself, or a low-stakes existing one — never a production Deck. `omnideck list --json` shows what's currently installed before you touch anything.
  3. Run `npm run tauri dev` and exercise the flow in the actual window (Start/Stop/Restart, New Deck, Remove, Logs) — typechecking and fixture tests verify shapes, not that the UI actually calls the right command with the right args at the right time.

See `AGENT.md`'s "Testing expectations" section for where this is headed (integration tests against real podman, migration-simulation tests, cross-platform sidecar tests) as more of the app lands.
