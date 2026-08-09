#!/usr/bin/env bash
# Single entry point for `npm run dev:app` (see AGENT.md's "Linux dev
# environment" note).
#
# On an immutable/atomic Linux host (Fedora Silverblue, Bluefin, ...) the
# Rust/webview toolchain lives in a toolbox, not on the base system. That
# alone would be simple (just run everything inside the toolbox) — but the
# app also needs to reach the *host's* podman/docker to do anything useful,
# and a toolbox is itself a container: podman doesn't exist inside it by
# default, and even a host podman binary reached via /run/host's bind mount
# reliably crashes there (needs namespace/capability access a toolbox
# doesn't grant). Confirmed by hand before writing this: the compiled
# Tauri binary's dynamic deps (webkit2gtk, gtk3) already resolve fine on the
# bare host (`ldd` reports nothing missing) because this desktop image ships
# their runtime libraries already — only the `-devel`/pkgconfig headers and
# Rust toolchain were ever actually missing from the host.
#
# So: build inside the toolbox (needs cargo + webkit2gtk-devel), then run
# the resulting binary directly on the host (needs podman + the webview
# runtime libs, both already present there). The Vite dev server also runs
# on the host — Node is available there too — so only the Rust/cargo step
# needs the toolbox at all.
#
# On any other platform (macOS/Windows, or a normal non-atomic Linux distro
# with the toolchain already on PATH) none of this applies — just run
# `npm run tauri dev` directly, same as this script always did.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
toolbox_name="${OMNIDECK_TOOLBOX:-omnideck-dev}"
cd "$repo_root"

needs_toolbox_build() {
  [[ "$(uname -s)" == "Linux" ]] || return 1
  command -v toolbox >/dev/null 2>&1 || return 1
  if command -v cargo >/dev/null 2>&1 && pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    return 1
  fi
  return 0
}

if [[ -f /run/.toolboxenv ]]; then
  # Invoked from inside a toolbox directly — build and run here works, but
  # anything that shells out to podman/docker will fail (see header comment
  # above). Prefer running this script from your normal host shell instead.
  echo "warning: running inside a toolbox — podman-backed features (start/stop/status/...) will not work here. Run this from your host shell instead." >&2
  exec npm run tauri dev
fi

if needs_toolbox_build; then
  echo "==> Building Rust backend inside toolbox '$toolbox_name'..."
  toolbox run -c "$toolbox_name" bash -c "cd '$repo_root/src-tauri' && cargo build"

  vite_pid=""
  cleanup() {
    [[ -n "$vite_pid" ]] && kill "$vite_pid" 2>/dev/null || true
  }
  trap cleanup EXIT INT TERM

  echo "==> Starting Vite dev server on the host..."
  npm run dev &
  vite_pid=$!

  # Give Vite a moment to bind before the webview tries to load it.
  for _ in $(seq 1 30); do
    curl -sf http://localhost:1420 -o /dev/null 2>/dev/null && break
    sleep 0.5
  done

  echo "==> Running Omnideck directly on the host..."
  ./src-tauri/target/debug/omnideck-desktop
  exit $?
fi

# macOS/Windows, or a Linux host that already has the toolchain on PATH.
exec npm run tauri dev
