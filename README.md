<p align="center">
  <strong>Omnideck Desktop</strong> — a native app for managing your <a href="https://github.com/omnideck-dev/omnideck">Omnideck</a> agent workspaces.
</p>

<p align="center">
  <a href="https://github.com/omnideck-dev/desktop/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/omnideck-dev/desktop/ci.yml?branch=main&style=flat-square&label=ci" alt="CI Status">
  </a>
  <a href="https://github.com/omnideck-dev/desktop/releases">
    <img src="https://img.shields.io/github/v/release/omnideck-dev/desktop?include_prereleases&style=flat-square&label=release" alt="Latest Release">
  </a>
</p>

---

A Tauri v2 + React/TypeScript desktop app that manages one or more local Omnideck instances ("Decks") — start/stop/update/remove, live status, logs — and drives first-run setup of the Podman runtime they need. It's a thin GUI shell: almost no container logic lives here, it's a front end over the [`omnideck` CLI](https://github.com/omnideck-dev/cli).

## Architecture

```
                    single "main" window
┌───────────────────────────────────────────────────────────┐
│                     React + TypeScript                     │
│  OnboardingView          Dashboard view                    │
│  first-run/repair   ⇄    Deck list, start/                 │
│  Podman runtime setup    stop/update/remove                │
│  (shown until ready)     (shown once ready)                │
└──────────────────────────────┬──────────────────────────────┘
                                ▼
                 src-tauri/src/cli_bridge.rs
           (owns all `omnideck` CLI subprocess I/O —
            spawn, JSON/NDJSON parsing, bounds, timeouts)
                                │
                                ▼
                 bundled `omnideck` CLI sidecar
              (pinned by version + checksum, never PATH)
```

One window, one React app — onboarding and the dashboard are two screens `App.tsx` swaps between client-side based on whether the shared Podman runtime is ready, not two OS-level windows. That wasn't the original design (onboarding first shipped as a second, hidden, minimally-privileged window with its own Tauri capability); it was reverted after real hardware testing found that creating two GTK/WebKit windows at startup broke EGL/GPU-driver init on at least one real Intel/Mesa combination. See [`src-tauri/src/bootstrap.rs`](./src-tauri/src/bootstrap.rs)'s module doc comment for the full rationale — including the security-isolation tradeoff that reversal knowingly accepts — and [`reference/desktop-hardening-migration-PLAN.md`](./reference/desktop-hardening-migration-PLAN.md) for how this repo's security posture got here.

## Commands

| Command | What it does |
|---|---|
| `npm install` | Install frontend dependencies |
| `npm run fetch:sidecars` | Download + checksum-verify the pinned `omnideck` CLI binaries (once per checkout) |
| `npm run dev:app` | Run the full app — Vite + Rust backend, hot reload |
| `npm run dev` | Frontend only, no Rust/webview (fast React/CSS iteration) |
| `npm run build` | Typecheck + production frontend bundle |
| `npm run verify` | The full local gate: sidecars, policy tests, typecheck, `cargo fmt`/`test`/`clippy` — what CI runs |
| `npm run test:policy` | Security-posture assertions (capabilities, sidecar pin, process hardening) |

See [`DEVELOPMENT.md`](./DEVELOPMENT.md) for the rest (platform-specific builds, backend-only commands, onboarding-flow testing) and [`RELEASING.md`](./RELEASING.md) for cutting a release.

## Getting started

Requires Node.js 18+, Rust (stable), and the Tauri v2 platform build tools ([prerequisites](https://v2.tauri.app/start/prerequisites/)). Linux on an immutable/atomic distro (Fedora Silverblue, Bluefin, etc.) needs one extra setup step — see [`DEVELOPMENT.md`](./DEVELOPMENT.md#linux-on-an-immutableatomic-distro-fedora-silverblue-bluefin-etc).

```bash
git clone https://github.com/omnideck-dev/desktop
cd desktop
npm install
npm run fetch:sidecars
npm run dev:app
```

The app window opens immediately. If you don't have a ready Podman runtime yet, an onboarding screen guides you through setting one up before handing off to the dashboard.

## Documentation

- [`DEVELOPMENT.md`](./DEVELOPMENT.md) — full dev environment setup, testing (including previewing onboarding screens without touching real Podman state), and the command reference
- [`TESTING.md`](./TESTING.md) — the full test-layer breakdown (source, release contract, native smoke, manual) and what's deliberately not built yet
- [`RELEASING.md`](./RELEASING.md) — tagging, the CI release pipeline, and the review-gate policy
- [`AGENT.md`](./AGENT.md) — architecture, non-negotiable rules, and current build status (the primary reference for contributing code)
- [`DESIGN.md`](./DESIGN.md) — screen-by-screen UI/UX reference
- [`reference/`](./reference) — the specs and migration plans this app was built from

## Roadmap

See the public [Omnideck Roadmap](https://github.com/orgs/omnideck-dev/projects/1) board for what's planned, in progress, and recently shipped.

## Contributing

Issues and PRs welcome. Start with [`AGENT.md`](./AGENT.md) for the architecture and the rules that keep this app's security posture intact (capability scoping, CLI sidecar pinning, the CLI-delegated multi-instance model), then [`DEVELOPMENT.md`](./DEVELOPMENT.md) to get a dev environment running. `npm run verify` is the same gate CI runs — get it green locally before opening a PR.
