# Native packaged desktop smoke

Launches a real packaged build (not `tauri dev`) with
`OMNIDECK_DESKTOP_SMOKE_FILE` set. The app performs only `--version` and
`--json runtime status` through its bundled sidecar, writes a proof with
`"mutation": false`, and keeps running until terminated — see
`run_packaged_smoke`/`record_packaged_smoke` in `src-tauri/src/lib.rs`.
`validate-proof.mjs` checks the pinned CLI version/commit against
`vendor-manifest.json`, runtime schema 4, the exact read-only operations,
and the application hash.

Ported from the sibling repo's `tests/hardware`. Not yet ported: its
`run.sh`/`run.ps1`/`reset-host` harness scripts and the opt-in self-hosted
CI workflow — those assume real per-platform installers and a release
pipeline this repo doesn't have yet (Phase 6/7 of
`reference/desktop-hardening-migration-PLAN.md`). For now, run this by hand
against a real build:

```sh
npm run build:appimage
export OMNIDECK_DESKTOP_SMOKE_FILE=/tmp/omnideck-smoke-proof.json
./src-tauri/target/release/bundle/appimage/Omnideck_*.AppImage --appimage-extract-and-run &
sleep 5
kill %1
node tests/hardware/validate-proof.mjs \
  --proof /tmp/omnideck-smoke-proof.json \
  --application ./src-tauri/target/release/bundle/appimage/Omnideck_*.AppImage \
  --report /tmp/omnideck-smoke-report.json \
  --require-ready   # only if the test machine's Podman runtime is already ready
```

Requires an actual display session (X11 or Wayland) — this launches the
real GUI process, not a headless check. A missing target or unexecuted step
is blocked coverage, never a pass. Generated evidence belongs under
`artifacts/desktop-hardware/` (not committed).
