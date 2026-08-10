#!/usr/bin/env bash
# Post-processes a built Linux AppImage, removing shared libraries
# `linuxdeploy` bundled but shouldn't have — libraries whose ABI is tightly
# version-locked to another library `linuxdeploy`'s own exclude list
# *does* correctly leave to the host.
#
# The real bug this fixes: `linuxdeploy` has a ~50-entry built-in exclude
# list of universal system libraries it always leaves to the host —
# libgpg-error.so.0 is on it. libgcrypt.so.20 (pulled in transitively by
# webkit2gtk) is not, even though upstream GnuPG always releases and
# version-locks the two as a matched pair. Bundling one without the other
# means the bundled libgcrypt (built against the build machine's
# libgpg-error) gets loaded at runtime alongside whatever libgpg-error the
# *host* provides instead — on a sufficiently different distro, a symbol
# version the bundled libgcrypt expects isn't there. Confirmed directly: a
# Fedora-built AppImage crashed on Ubuntu with "undefined symbol:
# gpgrt_add_post_log_func, version GPG_ERROR_1.0"; removing the bundled
# libgcrypt.so.20 (falling back to the host's own matched pair, exactly
# like libgpg-error already does) fixed it — verified by relaunching the
# repackaged AppImage.
#
# Usage: strip-unsafe-appimage-libs.sh <bundle-dir>
# <bundle-dir> is a tauri bundle output dir containing an appimage/
# subdirectory with exactly one *.AppImage (matches scripts/checksums.mjs's
# own directory-argument convention).
set -euo pipefail

[[ $# -eq 1 ]] || { echo "Usage: $0 <bundle-dir>" >&2; exit 1; }
bundle_dir="$1"
appimage_dir="$bundle_dir/appimage"
[[ -d "$appimage_dir" ]] || { echo "No appimage/ directory under $bundle_dir" >&2; exit 1; }

appimage=""
for f in "$appimage_dir"/*.AppImage; do
  [[ -e "$f" ]] || continue
  if [[ -n "$appimage" ]]; then
    echo "Expected exactly one .AppImage in $appimage_dir, found more than one" >&2
    exit 1
  fi
  appimage="$f"
done
[[ -n "$appimage" ]] || { echo "No .AppImage found in $appimage_dir" >&2; exit 1; }
appimage="$(cd "$(dirname "$appimage")" && pwd)/$(basename "$appimage")"

# Libraries linuxdeploy bundles but shouldn't — each is tightly
# version-locked to a library linuxdeploy's own exclude list already
# leaves to the host, so bundling only this one breaks that pairing.
# Glob patterns, matched against usr/lib/.
unsafe_libs=(
  "libgcrypt.so*" # paired with libgpg-error.so.0, which linuxdeploy excludes
)
# GTK plugin, not a hard startup dependency — only breaks printing, not
# launch — but has the identical half-bundled-dependency-chain problem
# (needs avahi/colord/cups, none of which are bundled or excluded either).
unsafe_plugins=(
  "libprintbackend-cups.so"
)

work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT
cd "$work_dir"

echo "==> Extracting $(basename "$appimage")"
"$appimage" --appimage-extract >/dev/null

removed=0
for pattern in "${unsafe_libs[@]}" "${unsafe_plugins[@]}"; do
  for f in squashfs-root/usr/lib/$pattern; do
    [[ -e "$f" ]] || continue
    echo "    removing $(basename "$f")"
    rm -f "$f"
    removed=$((removed + 1))
  done
done
if (( removed == 0 )); then
  echo "Nothing matched the unsafe-library list — is it stale, or did linuxdeploy stop bundling these?" >&2
  exit 1
fi

# Same tool tauri's own AppImage bundling step already downloads+caches —
# reuse that cache if present (fast path for repeat local builds), else
# fetch fresh (CI containers start empty every run).
plugin="$HOME/.cache/tauri/linuxdeploy-plugin-appimage.AppImage"
if [[ ! -x "$plugin" ]]; then
  echo "==> Fetching linuxdeploy-plugin-appimage"
  mkdir -p "$(dirname "$plugin")"
  curl -fsSL -o "$plugin" \
    https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage
  chmod +x "$plugin"
fi

echo "==> Repackaging"
# NO_STRIP=1: same binutils/.relr.dyn issue documented in AGENT.md /
# build-appimage.sh — repackaging re-invokes strip otherwise.
NO_STRIP=1 "$plugin" --appimage-extract-and-run --appdir squashfs-root >/dev/null

repacked=""
for f in *.AppImage; do
  [[ -e "$f" ]] || continue
  repacked="$f"
  break
done
[[ -n "$repacked" ]] || { echo "Repackaging produced no AppImage" >&2; exit 1; }

mv -f -- "$repacked" "$appimage"
echo "==> Done: $appimage"
