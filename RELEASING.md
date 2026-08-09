# Releasing Omnideck Desktop

Versions follow SemVer. A prerelease suffix (`-alpha.N`, `-beta.N`, `-rc.N`)
selects a prerelease channel; no suffix means stable. Tags are immutable —
never move or reuse one.

## Cut a release

1. Bump `"version"` in `package.json` to the new version (no `v` prefix
   there — the tag gets the `v`).
2. If the pinned CLI needs bumping too, update
   `src-tauri/binaries/vendor-manifest.json` deliberately (see
   `AGENT.md`'s "CLI sidecar" note) — never weaken its checksum checks.
3. Commit, push, and confirm `.github/workflows/ci.yml`'s `test` job is
   green on `main` at the commit you're about to tag. A green `test` job
   doesn't require having also run the full build matrix — but if you want
   that confidence before tagging, `workflow_dispatch` the workflow first.
4. Tag the exact commit on `main` and push the tag:
   ```sh
   git switch main
   git pull --ff-only
   git tag -a v0.1.0 -m "Omnideck v0.1.0"
   git push origin v0.1.0
   ```

Pushing a `v*` tag triggers `ci.yml`'s `test` → `build` → `release` chain:
`test` rejects the tag outright if it doesn't exactly match
`v<package.json version>` (bump the version and retag if you see this),
`build` produces installers for all three platforms, and `release` attests
build provenance, generates checksums, and publishes a GitHub Release with
GitHub's auto-generated notes (commits since the last tag). A tag containing
a prerelease suffix publishes as a GitHub prerelease and doesn't move
"latest"; a bare `vX.Y.Z` tag does.

### Review gate: stable vs. prerelease

The `release` job targets one of two GitHub Environments, chosen by whether
`github.ref_name` contains a `-` (the same check that decides
`prerelease`/`make_latest` above):

- **`release`** (`v0.1.0`, `v1.2.0`, ...) — required reviewers: everyone who
  currently has write access to the repo (`rlnorthcutt`, `lefoulkrod`). The
  job pauses and waits for either one's approval in the repo's Actions tab
  before publishing.
- **`release-preview`** (`v0.1.0-alpha.1`, `v0.1.0-beta.6`, `v0.1.0-rc.2`,
  ...) — no protection rules. Publishes unattended as soon as `build`
  finishes.

GitHub environment reviewers are named users/teams, not a live-tracked
"anyone with write access" role — if collaborators change, update
`release`'s reviewer list by hand (see below) or replace it with a team.

Both environments are configured directly via the GitHub API/repo settings,
not in `ci.yml` — `gh api repos/omnideck-dev/desktop/environments/release`
to inspect or change the required reviewers.

## Current known gaps

- **Unsigned installers.** No Apple Developer ID (notarization) or Windows
  Authenticode certificate is wired in yet. macOS shows a Gatekeeper "unknown
  developer" warning; Windows shows a SmartScreen "unknown publisher" prompt.
  Fine for early alpha/internal use; worth fixing (via GitHub Environment
  secrets + the relevant Tauri signing config) before pointing this at the
  general public.
- **No release-artifact contract check.** Nothing yet asserts the expected
  installer files actually exist with the expected names/checksums before
  publishing — `release.yml`'s `fail_on_unmatched_files: true` catches a
  totally missing platform, but not a subtly wrong one. See
  `reference/desktop-hardening-migration-PLAN.md`'s Phase 7 for the sibling
  repo's `tests/releasecontract` pattern if this becomes worth porting.
- **No `TESTING.md`.** This file assumes `ci.yml`'s green checkmark is
  sufficient evidence; there's no separate document defining test layers,
  promotion gates, or required manual sign-off yet. Fine at this repo's
  current stage (single maintainer, no external users depending on
  releases); revisit if that changes.
