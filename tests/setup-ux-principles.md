# Desktop setup UX principles

The canonical product and test contract for first-run setup and repair on
every supported operating system. Automated and manual desktop tests must
preserve these principles even when platform requirements force tasks to run
in a different order. Ported from the sibling repo
(`omnideck/desktop`)'s `tests/setup-ux-principles.md` — same principles,
same wording where it still applies to this repo's simpler model (two
runtime phases, no resume record, no Electron-parity fixture to keep in
sync — see `bootstrap.rs`'s doc comment for why those two sibling features
don't have an analog here).

- Omnideck is the primary setup surface and shows as much trustworthy
  progress as the underlying task provides.
- Native password, security, and permission prompts remain visible whenever
  the user must act. Omnideck explains what is about to happen before
  waiting for that prompt.
- Podman, WSL, package-manager, and command-line installer windows remain
  hidden whenever they can run noninteractively. Hidden work must not wait
  on an invisible prompt.
- Progress is truthful: use real percentages when they are available,
  `Waiting for approval` (via `SetupState.status`/`awaitingPermission`,
  driven by the CLI's `runtime ensure` NDJSON `"permission"` state) when the
  user must act, and an indeterminate state when no reliable measurement
  exists. Never synthesize a percentage or time estimate.
- Platform-specific task ordering is allowed when required by the operating
  system (the CLI's `runtime ensure` already handles this — see its
  `substage` vocabulary in `engine/runtime_setup.go`). The surrounding
  layout, language, progress treatment, retry behavior, and error
  presentation remain consistent regardless of which platform's ordering is
  in play.
- Retrying resumes at the failed stage and preserves completed work
  whenever it is safe to do so — `runtime ensure` is idempotent, so simply
  re-running `begin_setup` already satisfies this without a resume-record
  file on this app's side.
- Failures provide a clear next action and keep captured command or
  installer output inside the existing `Technical details` disclosure. Do
  not add a redundant diagnostic-log button or open a separate viewer.
- The Ready state contains the completion copy and the `Continue` button
  (this app's hand-off to the multi-instance dashboard, not a single
  hosted app) without a completed progress bar.
