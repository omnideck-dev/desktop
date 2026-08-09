# Clean-machine first-run test

Ported from the sibling repo's `clean-first-run.md`. Changed for this repo:
the expected screen sequence/copy (two phases — software/environment — not
four; "Continue" hands off to the multi-instance dashboard, not a single
hosted app), and step 3's Electron-parity DOM comparison is dropped (no
Electron app here to diff against).

## Starting state and safety

- Use a disposable VM snapshot or a dedicated, restorable machine with no
  prior Podman/Omnideck state.
- Capture installed applications, running processes, containers, machines,
  volumes, and ports before starting.
- Never run this against a machine with real Podman instances you care
  about — step 7 onward genuinely installs/starts a Podman runtime.

## Procedure

1. Install and launch through the normal user-facing path (the packaged
   AppImage/deb/rpm/dmg/nsis installer, not `tauri dev`). Confirm a desktop
   window (the dashboard, `"main"`) appears promptly and no terminal window
   accompanies it — AGENT.md's non-negotiable rule, verified against a real
   packaged build, not just dev mode.
2. On a machine with no ready Podman runtime, confirm the isolated
   onboarding window appears (not the dashboard) within a few seconds of
   launch, showing:
   - Eyebrow `WELCOME`, title "Let's get Omnideck ready", a `Set up
     Omnideck` button, and nothing else actionable.
3. Click "Set up Omnideck". Confirm the sequence:
   - Eyebrow switches to `SETTING UP`, an indeterminate progress bar
     appears, and the activity line shows the CLI's own copy ("Getting your
     computer ready…" or similar — this app doesn't invent its own copy for
     this, see `bootstrap.rs`'s doc comment on `preparing_state`).
   - Once `runtime ensure` reports real progress, the bar becomes
     determinate and advances monotonically (never backwards, never stuck
     past the phase's own completion).
   - If the platform needs a managed Podman machine (macOS/Windows), a
     second phase with its own activity text appears after the first
     completes.
4. Confirm light/dark selection follows the OS (`prefers-color-scheme`,
   `setup.js`'s `applyTheme()`).
5. At any OS elevation/permission prompt, dismiss once. Confirm the
   onboarding window shows a "Permission needed" title with a "Try again"
   button (`bootstrap.rs`'s `error_state()` for `PERMISSION_DENIED`), not a
   raw error or stack trace.
6. Retry and approve. Confirm setup continues from where it left off, not
   from scratch (this is `runtime ensure`'s own idempotency on the CLI
   side — see bootstrap.rs's doc comment on why there's no separate resume
   record in this app).
7. If a restart is required (e.g., WSL2 just enabled on Windows), confirm
   the onboarding window shows "Restart needed" with only a "Quit Omnideck"
   action (not retryable) — restart the machine, relaunch, and verify setup
   resumes rather than restarting from scratch.
8. Continue to the "ready" state: eyebrow `READY`, title "Omnideck is
   ready", a "Continue" button. Click it and confirm the onboarding window
   hides and the dashboard (`"main"`) is shown and focused, in its own
   empty state (no Decks yet) with a path to create the first one
   (`NewDeckForm.tsx`) — onboarding's job ends at runtime readiness; Deck
   creation is the dashboard's own already-built flow, not onboarding's.
9. Quit and relaunch. Confirm the onboarding window does **not** reappear —
   the dashboard shows directly. This is the exact regression this repo's
   own review caught and fixed (`bootstrap()` unconditionally showing
   onboarding on every launch); `tests/policy.test.mjs` has an automated
   guard for the code shape, but this step is the real end-to-end check.
10. Record the bundled CLI version/commit (`omnideck --version --json`,
    compare against `vendor-manifest.json`), final Podman state
    (`podman machine list` / `podman info`), and final process list.

## Pass criteria

The published package completes the real first-run journey, the permission
and restart paths work, the dashboard opens with the correct empty state,
a second launch does not re-show onboarding, and no unrelated Podman
resources exist afterward.
