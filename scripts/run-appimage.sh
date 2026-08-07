#!/usr/bin/env bash
# Runs the built AppImage with --appimage-extract-and-run.
#
# Confirmed by direct testing (see AGENT.md): in this toolbox/host setup, the
# AppImage's default FUSE-mount execution mode gets torn down before this
# app — large, webkit2gtk-heavy, slower than average to initialize — finishes
# starting, and the process exits silently within a few seconds with no
# crash or error output. Both the loose (unpacked) AppDir and
# --appimage-extract-and-run stayed running reliably in every test; the
# default FUSE-mount path never did. Not yet verified whether this is
# specific to this dev environment or would also affect a real end user's
# machine — if you're building a release for actual distribution, re-test
# the plain (no-flag) launch on a target system before assuming this flag is
# required everywhere.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
appimage="$repo_root/src-tauri/target/release/bundle/appimage/Omnideck_0.1.0_amd64.AppImage"

if [[ ! -f "$appimage" ]]; then
  echo "error: $appimage not found — build it first:" >&2
  echo "  toolbox run -c omnideck-dev bash -c 'NO_STRIP=1 npm run tauri build -- --bundles appimage'" >&2
  exit 1
fi

chmod +x "$appimage"
exec "$appimage" --appimage-extract-and-run "$@"
