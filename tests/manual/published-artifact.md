# Published artifact and trust experience

Ported from the sibling repo's `published-artifact.md`. Verify the package
users actually download, including browser reputation and the operating
system's unsigned-package warning — this repo's installers aren't
code-signed yet (no Apple notarization or Windows Authenticode cert; see
`RELEASING.md`'s "Current known gaps").

## Purpose

Confirm a real published release is trustworthy and installable through
the normal user path, not just that CI produced files.

## Procedure

1. Record the target release tag, source commit, OS, architecture, and
   package format.
2. Download the package and its `.sha256` through a normal browser from the
   GitHub release page (`https://github.com/omnideck-dev/desktop/releases`).
   Do not substitute a local build or Actions artifact.
3. Verify the SHA-256 independently:
   ```sh
   sha256sum -c Omnideck_<version>_<platform-suffix>.sha256
   ```
   and verify build provenance:
   ```sh
   gh attestation verify Omnideck_<version>_<platform-suffix> -R omnideck-dev/desktop
   ```
   Record both results. (Confirmed working against a real published
   release while writing this doc — `gh attestation verify` exits `0` with
   no output in a non-interactive shell; run it in a real terminal for the
   human-readable verification table.)
4. Record any browser download warnings. Confirm the filename, version,
   format, and architecture are correct.
5. Open the package through the normal user path and record SmartScreen
   (Windows), Gatekeeper (macOS), or Linux desktop/package-manager
   warnings.
6. Confirm any warning is attributable to the documented unsigned-build
   status, not corruption, a wrong architecture, or malformed packaging.
7. Complete installation, or for the AppImage, set only its executable bit
   and launch it directly (no installation step).

## Pass criteria

Checksum and provenance both pass, the OS recognizes the intended package,
the documented unsigned-build warning is the only warning shown, and no
unexpected publisher, architecture, corruption, or duplicate-launch warning
appears.
