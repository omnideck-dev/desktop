# Recovery lifecycle test

Ported from the sibling repo's `recovery-lifecycle.md`. The scenario list
(interrupted setup, network drop, engine stopped, reboot mid-setup, failed
action) applies to any long-running host-environment bootstrap and carries
over directly. Dropped: the "package lifecycle" section (needs real shipped
installers per platform — Phase 6/7 of the hardening plan, not done yet) and
the port-collision scenario (this repo's onboarding never reserves a port —
that was the sibling's single hosted-instance concept; per-Deck ports are
the CLI's own `add`/`environment` concern, already covered by
`NewDeckForm.tsx`'s existing error handling, not onboarding's).

For each scenario below, record the starting state, trigger, visible
onboarding-window state, `omnideck runtime status --json` output, process
list, and result.

## Controlled interruptions

1. **Close the onboarding window mid-setup, relaunch.** Verify `runtime
   ensure`'s own idempotency means relaunching doesn't duplicate work or
   corrupt the partial Podman install — this repo has no resume-record file
   to get out of sync (see bootstrap.rs's doc comment), so this specifically
   tests the CLI's own guarantee, not app-side state.
2. **Disconnect the network mid-download, then restore it.** Confirm the
   onboarding window shows "Download failed" with a "Try again" button
   (`DOWNLOAD_FAILED` → retryable, per `bootstrap.rs`'s `error_state()`),
   and that retrying actually resumes rather than erroring immediately
   again.
3. **Stop the Podman runtime/machine while the dashboard is open** (a Deck
   already running). Verify the dashboard degrades gracefully (per
   DESIGN.md's "Engine unreachable after onboarding" — a per-row or banner
   error, not a full-app block) rather than silently showing stale data.
4. **Reboot mid-setup** (after triggering a restart-required failure, or by
   force-restarting the test machine during the "environment" phase).
   Verify relaunching drives a single, unambiguous resume path — not a
   duplicate Podman machine, not a stuck "already running" state.
5. **Relaunch after a failed, non-retryable action** (e.g., `UNSUPPORTED`).
   Verify the onboarding window still renders correctly (doesn't get stuck
   on a stale error from the previous run) and offers the same "Quit
   Omnideck" action, not a crash or blank window.

## Regression checks specific to this repo's window-label fix

6. **Confirm the dashboard (`"main"`) is never blocked on onboarding.**
   Launch on a machine where the runtime is already ready. Verify the
   dashboard is visible immediately (AGENT.md's "shown immediately, never
   blank" rule) and the onboarding window never becomes visible at all
   during this launch — not even briefly. This is the manual counterpart to
   `tests/policy.test.mjs`'s static check on `bootstrap()`'s ready branch.
7. **Confirm onboarding's IPC bridge is unreachable from the dashboard.**
   With devtools attached to the dashboard window (`"main"`), attempt
   `window.__TAURI__.core.invoke('begin_setup', {})` from its console.
   Verify it's rejected (no `dashboard-bridge` permission for that
   command) — the capability split, not just convention, is what prevents
   the dashboard from driving onboarding's bridge.
