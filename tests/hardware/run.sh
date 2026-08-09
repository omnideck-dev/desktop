#!/usr/bin/env bash
# Launches a packaged desktop executable with OMNIDECK_DESKTOP_SMOKE_FILE
# set, waits for the read-only smoke proof lib.rs's run_packaged_smoke()
# writes, validates it, and terminates the host. Ported from the sibling
# repo's tests/hardware/run.sh, adapted to this repo's binary name
# (omnideck-desktop, not omnideck) and without OMNIDECK_DESKTOP_USER_DATA
# (this app has no isolated-user-data-directory feature — it uses the
# CLI's/Podman's normal shared state, per the multi-instance model).
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
desktop_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
application=""
output_directory="${OMNIDECK_DESKTOP_SMOKE_OUTPUT_DIR:-}"
timeout_seconds=45
require_ready=false

usage() {
  echo "Usage: $0 --application PATH [--output DIR] [--timeout SECONDS] [--require-ready]"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --application) application="${2:?Missing application path}"; shift 2 ;;
    --output) output_directory="${2:?Missing output directory}"; shift 2 ;;
    --timeout) timeout_seconds="${2:?Missing timeout}"; shift 2 ;;
    --require-ready) require_ready=true; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

[[ -n "$application" ]] || { usage >&2; exit 1; }
[[ -x "$application" ]] || { echo "Application is not executable: $application" >&2; exit 1; }
[[ "$timeout_seconds" =~ ^[0-9]+$ ]] || { echo "Timeout must be an integer." >&2; exit 1; }
(( timeout_seconds >= 5 && timeout_seconds <= 300 )) || { echo "Timeout must be between 5 and 300 seconds." >&2; exit 1; }
command -v node >/dev/null 2>&1 || { echo "Node.js is required to validate the packaged smoke proof." >&2; exit 1; }

# -f, not -x: the kernel truncates comm to 15 bytes, and "omnideck-desktop"
# is 16 — pgrep -x can never match it (confirmed directly: it prints "pattern
# ... longer than 15 characters will result in zero matches" and always
# exits 1, silently disabling this safety check entirely). -f matches
# against the full command line instead, which isn't length-limited.
# "[o]mnideck-desktop" (not "omnideck-desktop") is the standard pgrep
# self-match guard: pgrep's own argv literally contains the search string,
# so an unguarded pattern matches the pgrep/pkill invocation itself —
# confirmed directly, this pattern always "found" a match with none
# actually running. The bracket makes the *string* "[o]mnideck-desktop"
# (this command's own argv) not satisfy the *regex* `[o]mnideck-desktop`,
# while still matching a real process's plain "omnideck-desktop".
if pgrep -f '[o]mnideck-desktop' >/dev/null 2>&1; then
  echo "Close every existing Omnideck process before running packaged smoke." >&2
  exit 1
fi
if [[ "$(uname -s)" == "Linux" && -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
  echo "A real X11 or Wayland session is required for native desktop smoke." >&2
  exit 1
fi

run_id="${GITHUB_RUN_ID:-local-$$}"
if [[ -z "$output_directory" ]]; then
  output_directory="$desktop_root/../artifacts/desktop-hardware/$(uname -s | tr '[:upper:]' '[:lower:]')-$run_id"
fi
mkdir -p "$output_directory"
proof_path="$output_directory/smoke-proof.json"
report_path="$output_directory/report.json"
stdout_path="$output_directory/host.stdout.log"
stderr_path="$output_directory/host.stderr.log"
rm -f -- "$proof_path"

application="$(CDPATH= cd -- "$(dirname -- "$application")" && pwd)/$(basename -- "$application")"

# AppImage-specific: the plain FUSE-mount launch has been observed to exit
# silently within a few seconds on at least one real environment (see
# AGENT.md's AppImage runtime notes and scripts/run-appimage.sh, which
# always passes this too) — --appimage-extract-and-run sidesteps FUSE
# entirely. Harmless to always pass for an AppImage; irrelevant for the
# other platforms' installers, which don't take this flag at all.
application_args=()
if [[ "$application" == *.AppImage ]]; then
  application_args+=(--appimage-extract-and-run)
fi

OMNIDECK_DESKTOP_SMOKE_FILE="$proof_path" \
  "$application" "${application_args[@]}" >"$stdout_path" 2>"$stderr_path" &
application_pid=$!

# Confirmed directly: an AppImage launched via --appimage-extract-and-run
# reparents away from $application_pid during extraction (its real PPID ends
# up some unrelated adopting process, not this script) — killing only the
# tracked PID here left a fully running, un-cleaned-up omnideck-desktop +
# WebKit process tree behind in testing. pkill -f as a fallback catches it
# regardless of the process-tree relationship.
cleanup() {
  if kill -0 "$application_pid" >/dev/null 2>&1; then
    kill "$application_pid" >/dev/null 2>&1 || true
    wait "$application_pid" >/dev/null 2>&1 || true
  fi
  pkill -f '[o]mnideck-desktop' >/dev/null 2>&1 || true
}
trap cleanup EXIT

deadline=$((SECONDS + timeout_seconds))
while [[ ! -f "$proof_path" ]]; do
  if ! kill -0 "$application_pid" >/dev/null 2>&1; then
    echo "The desktop host exited before writing a packaged smoke proof." >&2
    exit 1
  fi
  if (( SECONDS >= deadline )); then
    echo "The desktop host did not write a packaged smoke proof within $timeout_seconds seconds." >&2
    exit 1
  fi
  sleep 0.25
done

validation=(
  node "$script_dir/validate-proof.mjs"
  --proof "$proof_path"
  --application "$application"
  --report "$report_path"
)
if [[ "$require_ready" == "true" ]]; then
  validation+=(--require-ready)
fi
"${validation[@]}"
echo "Evidence: $report_path"
