#!/usr/bin/env bash
# Builds the Linux AppImage bundle. Needs the same Rust/webkit2gtk toolchain
# as scripts/dev.sh's dev build, so it re-execs into the toolbox the same
# way — see that script and AGENT.md for why.
#
# NO_STRIP=1 works around a binutils/linuxdeploy incompatibility: this
# toolbox's `strip` (binutils 2.44) can't parse the `.relr.dyn` relocation
# section present in its own newer system libraries that get bundled in, and
# fails the whole build without it. Confirmed by hand — see AGENT.md.
#
# No longer `exec`s the build command directly (it used to) — needs control
# back afterward to run strip-unsafe-appimage-libs.sh, which doesn't need
# the toolbox at all (it only extracts/repackages an already-built
# AppImage), so it always runs on the bare host regardless of which branch
# built it.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
toolbox_name="${OMNIDECK_TOOLBOX:-omnideck-dev}"
cd "$repo_root"

if [[ -f /run/.toolboxenv ]]; then
  env NO_STRIP=1 npm run tauri build -- --bundles appimage
elif [[ "$(uname -s)" == "Linux" ]] && command -v toolbox >/dev/null 2>&1 \
  && { ! command -v cargo >/dev/null 2>&1 || ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; }; then
  toolbox run -c "$toolbox_name" bash -c "cd '$repo_root' && NO_STRIP=1 npm run tauri build -- --bundles appimage"
else
  env NO_STRIP=1 npm run tauri build -- --bundles appimage
fi

bash "$repo_root/scripts/strip-unsafe-appimage-libs.sh" "$repo_root/src-tauri/target/release/bundle"
