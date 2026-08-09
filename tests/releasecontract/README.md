# Desktop release contract

Treats the published desktop packages as artifacts, not source-build output.
Verifies the exact 5-package matrix (`.github/workflows/release.yml`'s
3-target build), one matching sha256sum-compatible checksum per package,
nonempty package files, container format signatures (PE for NSIS, UDIF for
DMG, ELF for AppImage, ar for deb, RPM magic bytes), and AppImage executable
architecture.

Run it against a directory containing the release assets:

```sh
node tests/releasecontract/verify-release.mjs \
  --directory dist \
  --version v0.5.0-alpha.2 \
  --report artifacts/desktop-release-contract/report.json
```

This is a static, non-installing contract. It does not prove that an
installer can actually be installed, that a GUI can reach a display server,
or that the bundled sidecar can execute. Those requirements belong to
[`../hardware`](../hardware/README.md) and [`../manual`](../manual/README.md).

`release.yml`'s `publish` job runs this contract against the downloaded
build artifacts before attesting/publishing anything — a totally missing
platform was already caught by `fail_on_unmatched_files: true` on the
publish step, but this catches a subtly wrong one (corrupted, truncated, or
wrong-architecture artifact; a checksum that doesn't match its file).

Not yet ported from the sibling repo: a post-publication re-verification
against the *public* release assets (downloading them fresh and checking
GitHub attestation, not just the build-time artifacts). Worth adding as a
manually-dispatched workflow once this becomes a real concern — see
`TESTING.md`.
