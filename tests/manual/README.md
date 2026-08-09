# Manual desktop release tests

These procedures cover behavior hosted CI cannot safely or reliably prove:
runtime installation and elevation, restart/resume, real desktop
integration, and destructive recovery. Ported from the sibling repo
(`omnideck/desktop`)'s manual test suite, adapted to this repo's actual
scope — see each file for what changed and why.

Run them against a real packaged build (`npm run build:appimage`/etc, not
`tauri dev`) on a disposable VM or dedicated test machine wherever a
procedure installs anything or changes host state. An agent or tester must
inventory the exact host before a mutating step and stop if the procedure
could alter resources not created for the test.

Required procedures:

- [Clean-machine first run](clean-first-run.md)
- [Recovery lifecycle](recovery-lifecycle.md)
- [Published artifact and trust experience](published-artifact.md)

Not yet ported from the sibling: `hosted-app-behavior.md` (specific to the
sibling's single hosted-instance webview, which this repo's dashboard model
doesn't have — the closest analog here is the per-Deck instance webview,
DESIGN.md #7, already covered by this repo's existing manual verification
practice, not a new document), and `visual-platform.md` (worth adding once
this app has real usage on more than one platform — right now only Linux
has been built/run and verified at all, including the one real published
release so far).

Every execution must record:

```text
Build: (npm run build:appimage output, or equivalent)
CLI version/commit bundled: (from vendor-manifest.json)
OS and exact version:
CPU architecture:
Package format:
Display server / desktop environment:
WebView version:
Podman/runtime baseline (podman --version, engine mode):
Clean machine or reused state:
Scenario:
Start/end timestamps:
Result: pass | fail | blocked
Screenshots/log locations:
Starting resource inventory (containers/volumes/processes):
Final resource inventory:
Residual processes/files:
Issue links:
Tester:
```

Record unavailable hardware or an unexecuted step as `blocked`; never infer
a pass from a successful cross-build or from `tauri dev`. Redact tokens and
sensitive home-directory details before attaching evidence.
