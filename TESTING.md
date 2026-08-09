# Testing

The authoritative source for what test layers exist, what each one proves,
and what's deliberately not built yet. Modeled on the sibling repo
(`omnideck/desktop`)'s `TESTING.md`, scaled down to this repo's actual
scope and maturity — see "Deliberately not done yet" below for what was
cut and why.

A passing build proves only its stated layer. Compilation is not
installation, a cross-build is not native execution, and an unexecuted
manual procedure is `blocked` coverage, not a pass.

## Test layers

| Layer | Implementation | Where it runs | Proves |
|---|---|---|---|
| Source | `test.yml` (reusable): sidecar fetch/verify, policy tests, typecheck, `cargo fmt`/`test`/`clippy` | Every PR, push to `main`, and tag push | Code compiles, security-posture invariants hold, sidecar checksums match the pin |
| Release contract | `tests/releasecontract/verify-release.mjs` | `release.yml`'s `publish` job, before attestation/publish | The 5-artifact matrix is exactly right: correct filenames, per-format magic bytes, checksums match |
| Native packaged smoke | `tests/hardware/run.sh` + `validate-proof.mjs` | Manual, on a real machine with a display session (see `tests/hardware/README.md`) | A real installed/launched build actually reaches and correctly parses real CLI output on that OS — not a fixture |
| Manual journey | `tests/manual/*.md` | Manual, on a disposable VM or dedicated machine | First-run setup, recovery/interruption handling, published-artifact trust experience |

Hosted CI (source + release contract) runs on every relevant push/PR/tag.
Native packaged smoke and manual journeys are run by hand before a real
release goes out to actual users — see `RELEASING.md`.

## Automated security boundary

Source tests (`tests/policy.test.mjs`) keep the following invariants
release-blocking:

- the dashboard window's capability is an enumerated allowlist
  (`dashboard-bridge`), never `core:default`;
- the onboarding window's capability (`onboarding-bridge`) is scoped to
  `"windows": ["onboarding"]` only — never `"main"`, which is the dashboard
  here (this exact mistake was caught once already; see the policy test's
  own comment);
- every onboarding command re-checks `window.label() == "onboarding"`
  server-side, not just the capability grant;
- `bootstrap()` only reveals the onboarding window when setup is actually
  needed — never unconditionally (this was a real, caught-and-fixed
  regression);
- the CLI sidecar is pinned by checksum for all 6 target triples, and the
  runtime version check is a floor (`>= v0.10.0`), not an exact match;
- sidecar process output is bounded and every operation has a timeout.

## Release gating

`release.yml`'s `test` → `build` → `publish` chain (see `RELEASING.md`) is
the actual gate: a tag that fails any version-field check, any source
check, or the release-artifact contract never reaches `publish`. Stable
tags additionally require review in the `release` GitHub Environment;
alpha/beta/rc tags publish unattended via `release-preview`.

There is no formal alpha/beta/RC/stable promotion ladder with named gates
per channel (contrast the sibling's, which has one) — this repo doesn't
have the release volume or team size to justify that process yet. The tag
naming convention (`-alpha.N`/`-beta.N`/`-rc.N`/bare) already exists and
GitHub correctly treats non-bare tags as prereleases; formalize gates per
channel if/when that distinction needs to mean something more than "not
GA."

## Deliberately not done yet

- **Self-hosted hardware runners.** The sibling's opt-in native-smoke CI
  workflow runs on dedicated, always-on self-hosted machines. Standing
  that up is a real infrastructure/ops commitment (cost, machine
  maintenance, security surface of a self-hosted runner), not something to
  build speculatively — `tests/hardware/run.sh` exists and is verified
  working; running it is manual for now.
- **`run.ps1`** (Windows equivalent of `tests/hardware/run.sh`). Cheap to
  write, but not written — and critically, not *tested*, since there's no
  Windows machine available while writing this. `run.sh` needed two real
  fixes (a broken `pgrep -x` length limit, an AppImage process-reparenting
  cleanup bug) despite being ported from already-working code; don't trust
  a straight port of `run.ps1` without running it for real first.
- **`visual-platform.md`** (theme/DPI/multi-monitor/accessibility/platform
  fit). Worth adding once this app has real usage on more than one
  platform — right now only Linux has been built, run, and verified.
- **Destructive host-reset tooling** (the sibling's
  `scripts/release-test/reset-host.*`, for resetting a test machine's
  WSL/Podman state between manual runs). Real, useful, and genuinely
  dangerous if done wrong (the sibling's version has real safety rails —
  typed confirmation, dry-run, administrator checks) — not built
  speculatively alongside everything else here.
- **Post-publication re-verification workflow** (downloading the *public*
  release assets fresh and re-running the release contract + attestation
  check, rather than trusting the build-time artifacts). `tests/manual/
  published-artifact.md` covers this by hand; automating it is a
  reasonable, cheap follow-up once it's worth not doing by hand every time.
