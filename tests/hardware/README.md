# Native packaged desktop smoke

Launches a real packaged build (not `tauri dev`) with
`OMNIDECK_DESKTOP_SMOKE_FILE` set. The app performs only `--version` and
`--json runtime status` through its bundled sidecar, writes a proof with
`"mutation": false`, and keeps running until terminated — see
`run_packaged_smoke`/`record_packaged_smoke` in `src-tauri/src/lib.rs`.
`validate-proof.mjs` checks the pinned CLI version/commit against
`vendor-manifest.json`, runtime schema 4, the exact read-only operations,
and the application hash. `validate-proof.test.mjs` unit-tests the
validator itself against synthetic fixtures.

Ported from the sibling repo's `tests/hardware`, now including `run.sh`
(build a real installer, run it with the smoke env var, wait for the proof,
validate, clean up):

```sh
npm run build:linux   # or build:windows / build:macos
bash tests/hardware/run.sh \
  --application src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/Omnideck_*.AppImage \
  --require-ready   # only if the test machine's Podman runtime is already ready
```

Requires an actual display session (X11 or Wayland) — this launches the
real GUI process, not a headless check. Verified directly against a real
build (not just written and assumed correct) — two real bugs were caught
and fixed doing that:

- `pgrep -x omnideck-desktop`'s "refuse to run if already running" check
  silently never matched anything at all — the kernel truncates `comm` to
  15 bytes, and `omnideck-desktop` is 16. Switched to `pgrep -f` matching
  the full command line instead, which isn't length-limited.
- An AppImage launched via `--appimage-extract-and-run` reparents away from
  the PID this script tracks during extraction, so cleanup only killing
  that one PID left a fully running app + WebKit process tree behind.
  `pkill -f` as a cleanup fallback catches it regardless of the actual
  process-tree relationship.

Both fixes use the `[o]mnideck-desktop` bracket form, not
`omnideck-desktop` — the standard guard against `pgrep -f`/`pkill -f`
matching their own invocation (their own argv literally contains the
search string).

Not yet ported: `run.ps1` (Windows), `reset-host.sh`/`reset-host.ps1`, and
the opt-in self-hosted-runner CI workflow that drives all of this
automatically. `run.ps1` is a reasonable, cheap follow-up (same shape as
`run.sh`, just PowerShell) but hasn't been written *or tested* — don't
trust a straight port of it without running it on real Windows first, the
same way `run.sh` needed two real fixes despite being ported from
already-working code. The self-hosted-runner workflow is a real
infrastructure/ops decision (dedicated always-on test hardware), not
something to stand up speculatively — see `TESTING.md`.

A missing target or unexecuted step is blocked coverage, never a pass.
Generated evidence belongs under `artifacts/desktop-hardware/` (not
committed).
