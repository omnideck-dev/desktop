//! Owns 100% of `omnideck` CLI subprocess spawn + JSON/NDJSON parsing.
//!
//! Every `--json` invocation writes exactly one JSON value (or, for the
//! streaming commands, one NDJSON line per event) to stdout and nothing else
//! there — see `docs/JSON_MODE_SPEC.md` in the CLI repo (mirrored at
//! `reference/` in this repo). stderr may carry advisory warnings that are
//! not part of the contract; only stdout is ever parsed.
//!
//! This module carries zero podman/docker-specific knowledge of its own —
//! that's the CLI's job. It validates the reported `jsonContract` against
//! [`EXPECTED_JSON_CONTRACT`] and returns a distinguishable error on
//! mismatch, per `AGENT.md`'s non-negotiable rules.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tauri::Emitter;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// Caps on how much stdout/stderr a single sidecar invocation may produce
/// before this app gives up on it, so a runaway/hung `omnideck` process
/// can't exhaust memory reading its output. Modeled on the sibling repo's
/// `STDOUT_LIMIT`/`STDERR_LIMIT` in `lib.rs`.
const STDOUT_LIMIT: usize = 1_000_000;
const STDERR_LIMIT: usize = 256 * 1024;

/// Per-operation timeouts. Short for anything that only inspects state
/// (`list`/`status`/`doctor`/`config show`/`logs`); long for anything that
/// can pull a container image or otherwise do first-run work (`add`,
/// `update`). Modeled on the sibling's `FixedOperation::timeout()`.
const INSPECTION_TIMEOUT: Duration = Duration::from_secs(15);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// The `jsonContract` version this app build was written against
/// (`JSON_MODE_SPEC.md` §1). Bump only on a deliberate, reviewed change once
/// the CLI's contract itself changes. `3` as of CLI `v0.11.0-alpha.2` —
/// confirmed directly against a real `v0.11.0-alpha.2` sidecar binary's
/// `--version --json` output and `contracts/json/v3/version.schema.json` in
/// the CLI repo, not assumed from stale docs. Contract `3`'s actual changes
/// (vs. `2`): `runtime ensure`'s NDJSON events gained `substage`/`status`
/// fields and a `"permission"` state value, and the error envelope gained a
/// `detail` field plus 4 new codes (`PERMISSION_CANCELLED`/
/// `WINDOWS_FEATURES_FAILED`/`PACKAGE_INDEX_FAILED`/`INSTALLER_FAILED`) —
/// all additive, all now handled in `bootstrap.rs`.
pub const EXPECTED_JSON_CONTRACT: u64 = 3;

/// The lowest CLI release this app build is verified against. Checked as a
/// floor (`>=`), not an exact match — see
/// `reference/desktop-hardening-migration-PLAN.md`'s "Decisions from
/// review": the bundled sidecar is always exactly whatever
/// `vendor-manifest.json` pinned at build time (checksummed, so an exact
/// runtime match would be redundant with that), so this check exists to
/// catch a build mistake or corrupted binary, not to gate against a
/// different externally-supplied CLI. `v0.11.0-alpha.2` specifically because
/// that's the first CLI release with contract `3`'s `substage`/`status`/
/// `"permission"`-state runtime-setup fields this app now depends on for
/// its permission-wait UI (see `bootstrap.rs`'s `preparing_state`).
pub const MINIMUM_CLI_VERSION: &str = "v0.11.0-alpha.2";

/// The `omnideck` binary is bundled as a Tauri sidecar (`bundle.externalBin`
/// in `tauri.conf.json`, source at `src-tauri/binaries/omnideck-<target-triple>`)
/// rather than resolved on PATH. This matters right now specifically because
/// the JSON-capable CLI build isn't published anywhere a package manager
/// could install it from yet — a system-installed pre-`--json` CLI is a
/// real, silent failure mode otherwise (it doesn't error on the unrecognized
/// flag, it just answers as if `--json` weren't there). `app.shell().sidecar()`
/// resolves next to the running executable in both dev and packaged builds,
/// so this works the same way everywhere with no PATH dependency at all.
///
/// AGENT.md's non-negotiable rule — pin by version + checksum, never
/// silently track "latest" — still applies once this binary comes from a
/// real released tag instead of a local build; see the binaries/ directory
/// for the current provenance note.
const CLI_SIDECAR: &str = "omnideck";

/// Every failure mode of talking to the CLI, distinguishable so the frontend
/// can render each one differently (missing binary vs. a structured CLI
/// error vs. a version-contract mismatch).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CliError {
    /// The `omnideck` binary couldn't be spawned at all (not on PATH, no
    /// exec permission, etc).
    Spawn { message: String },
    /// The process produced no stdout at all.
    NoOutput,
    /// stdout wasn't valid JSON, or didn't match the shape this command
    /// expects.
    Parse { message: String, raw: String },
    /// The CLI's own structured error envelope (`JSON_MODE_SPEC.md` §3).
    /// Boxed to keep `CliError` itself small (clippy::result_large_err) —
    /// this is the only variant with more than two fields.
    Cli(Box<CliErrorBody>),
    /// The CLI's `jsonContract` doesn't match what this app build expects.
    ContractMismatch { expected: u64, actual: u64 },
    /// The CLI reports a version older than [`MINIMUM_CLI_VERSION`], or a
    /// version string this app can't parse as `vMAJOR.MINOR.PATCH[-pre]`.
    VersionTooOld { minimum: String, actual: String },
    /// The sidecar's stdout or stderr exceeded [`STDOUT_LIMIT`]/[`STDERR_LIMIT`]
    /// — the process is killed immediately rather than let unbounded output
    /// accumulate in memory.
    OutputLimitExceeded { stream: String },
    /// The sidecar didn't finish within its operation's timeout — the
    /// process is killed rather than left to block the calling command
    /// indefinitely.
    Timeout,
    /// `runtime status`/`runtime ensure`'s `schemaVersion` doesn't match
    /// [`EXPECTED_RUNTIME_SCHEMA`]. Independent from `jsonContract` — per the
    /// CLI repo's `contracts/README.md`, "JSON contract 3 currently carries
    /// runtime status schema 4"; the two axes version separately.
    RuntimeSchemaMismatch { expected: u32, actual: u32 },
}

#[derive(Debug, Clone, Serialize)]
pub struct CliErrorBody {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
    /// Longer explanatory text alongside `message`/`hint` (JSON contract
    /// v3's error envelope, new since v2) — e.g. `runtime ensure`'s Linux
    /// permission-wait event explains *why* a native prompt is about to
    /// appear before it does, per the sibling's setup-UX principle "explain
    /// what's about to happen first". Not yet surfaced in `bootstrap.rs`'s
    /// `error_state()` copy (still just uses `message`) — captured here so
    /// it's available when that's worth doing.
    pub detail: Option<String>,
    pub action: Option<String>,
    #[serde(rename = "actionValue")]
    pub action_value: Option<String>,
    pub instances: Option<Vec<String>>,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Spawn { message } => write!(f, "failed to run omnideck CLI: {message}"),
            CliError::NoOutput => write!(f, "omnideck CLI produced no output"),
            CliError::Parse { message, .. } => {
                write!(f, "failed to parse omnideck CLI output: {message}")
            }
            CliError::Cli(body) => write!(f, "omnideck CLI error [{}]: {}", body.code, body.message),
            CliError::ContractMismatch { expected, actual } => write!(
                f,
                "omnideck CLI jsonContract mismatch: this app expects {expected}, CLI reports {actual}"
            ),
            CliError::VersionTooOld { minimum, actual } => write!(
                f,
                "omnideck CLI version too old: this app requires at least {minimum}, CLI reports {actual}"
            ),
            CliError::OutputLimitExceeded { stream } => {
                write!(f, "omnideck CLI exceeded the {stream} output limit")
            }
            CliError::Timeout => write!(f, "omnideck CLI did not finish in time"),
            CliError::RuntimeSchemaMismatch { expected, actual } => write!(
                f,
                "omnideck CLI runtime status schema mismatch: this app expects {expected}, CLI reports {actual}"
            ),
        }
    }
}

/// Parses a `vMAJOR.MINOR.PATCH[-prerelease]` version string (the shape both
/// this app's [`MINIMUM_CLI_VERSION`] and every real CLI release tag use).
fn parse_semver(version: &str) -> Option<(u64, u64, u64, Option<&str>)> {
    let version = version.strip_prefix('v').unwrap_or(version);
    let (core, prerelease) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, prerelease))
}

/// `true` if `version >= minimum`. A prerelease is ordered below the plain
/// release at the same `major.minor.patch` (matches semver precedence:
/// `1.0.0-alpha < 1.0.0`); two prereleases at the same core version compare
/// lexicographically, which is only exactly correct for same-width numeric
/// suffixes (`alpha.9` < `alpha.10` lexicographically says otherwise) — an
/// accepted imprecision here since `MINIMUM_CLI_VERSION` itself is always a
/// plain release, so that comparison path is never exercised in practice.
/// Unparseable version strings never satisfy the floor.
fn meets_minimum_version(version: &str, minimum: &str) -> bool {
    let Some((major, minor, patch, prerelease)) = parse_semver(version) else {
        return false;
    };
    let Some((min_major, min_minor, min_patch, min_prerelease)) = parse_semver(minimum) else {
        return false;
    };
    match (major, minor, patch).cmp(&(min_major, min_minor, min_patch)) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (prerelease, min_prerelease) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(pre), Some(min_pre)) => pre >= min_pre,
        },
    }
}

impl std::error::Error for CliError {}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
    hint: Option<String>,
    detail: Option<String>,
    action: Option<String>,
    #[serde(rename = "actionValue")]
    action_value: Option<String>,
    instances: Option<Vec<String>>,
}

/// Builds the sidecar command, stripped of the environment variables the
/// AppImage runtime injects for *our own* GTK/WebKit process to find its
/// bundled libraries (`LD_LIBRARY_PATH` chief among them). Those vars are
/// inherited by every child process by default — including this sidecar,
/// and in turn *its* child (podman) — and podman dynamically linking
/// against the AppImage's bundled versions of shared libraries it also
/// happens to depend on (not the host's) causes container inspection to
/// silently fail (`status: "unknown"` for every instance, even running
/// ones) without erroring outright. Confirmed by comparing this process's
/// real environment against a manual reproduction, not a guess — see
/// AGENT.md. Only `LD_LIBRARY_PATH` actually affects `ld.so`'s dynamic
/// linking; the GTK/GST/Qt-specific module-search vars are irrelevant to
/// podman and left alone.
fn sidecar_command<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<tauri_plugin_shell::process::Command, CliError> {
    Ok(app
        .shell()
        .sidecar(CLI_SIDECAR)
        .map_err(|e| CliError::Spawn {
            message: e.to_string(),
        })?
        .env("LD_LIBRARY_PATH", ""))
}

/// Accumulates a stream chunk into `destination`, failing once the running
/// total would exceed `limit` — never silently truncates. Modeled on the
/// sibling repo's `append_bounded()` in `lib.rs`.
fn append_bounded(
    destination: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    stream: &str,
) -> Result<(), CliError> {
    if destination.len().saturating_add(chunk.len()) > limit {
        return Err(CliError::OutputLimitExceeded {
            stream: stream.to_string(),
        });
    }
    destination.extend_from_slice(chunk);
    Ok(())
}

/// Reassembles complete lines across `CommandEvent::Stdout` chunk
/// boundaries. `tauri-plugin-shell`'s event stream is not guaranteed to
/// split exactly on `\n` — a single logical JSON line can arrive split
/// across two chunks — so iterating `chunk.lines()` directly (as this
/// module used to) can hand a caller half a JSON object. Modeled on the
/// sibling repo's `LineBuffer` in `lib.rs`, which hit this exact bug.
#[derive(Default)]
struct LineBuffer {
    pending: Vec<u8>,
}

impl LineBuffer {
    fn push<F: FnMut(&str)>(&mut self, chunk: &[u8], on_line: &mut F) {
        self.pending.extend_from_slice(chunk);
        while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=index).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            Self::deliver(&line, on_line);
        }
    }

    fn flush<F: FnMut(&str)>(&mut self, on_line: &mut F) {
        let pending = std::mem::take(&mut self.pending);
        Self::deliver(&pending, on_line);
    }

    fn deliver<F: FnMut(&str)>(line: &[u8], on_line: &mut F) {
        let text = String::from_utf8_lossy(line);
        let text = text.trim();
        if !text.is_empty() {
            on_line(text);
        }
    }
}

/// Only `stdout` is kept: this app's existing convention (predating this
/// refactor) never inspects exit code or stderr — `JSON_MODE_SPEC.md` §8
/// requires every failure to be a structured `error` object on stdout
/// regardless of exit code, which is what [`run_json`]/[`run_ndjson_stream`]
/// already parse for directly.
struct ProcessResult {
    stdout: Vec<u8>,
}

/// Spawns the sidecar with `args`, enforcing [`STDOUT_LIMIT`]/[`STDERR_LIMIT`]
/// and `timeout_duration` (killing the child on either violation), and
/// calling `on_line` with each complete, reassembled stdout line as it
/// arrives — used both by [`run_json`] (which ignores the lines and reads
/// the accumulated stdout once the process exits) and [`run_ndjson_stream`]
/// (which is driven entirely by the lines). Modeled on the sibling repo's
/// `run_cli()` in `lib.rs`.
async fn run_cli<R: tauri::Runtime, F: FnMut(&str)>(
    app: &tauri::AppHandle<R>,
    args: &[&str],
    timeout_duration: Duration,
    mut on_line: F,
) -> Result<ProcessResult, CliError> {
    let (mut events, child) =
        sidecar_command(app)?
            .args(args)
            .spawn()
            .map_err(|e| CliError::Spawn {
                message: e.to_string(),
            })?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdout_lines = LineBuffer::default();
    let mut timeout = Box::pin(tokio::time::sleep(timeout_duration));

    loop {
        tokio::select! {
            _ = &mut timeout => {
                let _ = child.kill();
                return Err(CliError::Timeout);
            }
            event = events.recv() => match event {
                Some(CommandEvent::Stdout(bytes)) => {
                    if let Err(e) = append_bounded(&mut stdout, &bytes, STDOUT_LIMIT, "stdout") {
                        let _ = child.kill();
                        return Err(e);
                    }
                    stdout_lines.push(&bytes, &mut on_line);
                }
                Some(CommandEvent::Stderr(bytes)) => {
                    if let Err(e) = append_bounded(&mut stderr, &bytes, STDERR_LIMIT, "stderr") {
                        let _ = child.kill();
                        return Err(e);
                    }
                }
                Some(CommandEvent::Error(message)) => {
                    let _ = child.kill();
                    return Err(CliError::Spawn { message });
                }
                Some(CommandEvent::Terminated(_)) => {
                    stdout_lines.flush(&mut on_line);
                    return Ok(ProcessResult { stdout });
                }
                Some(_) => {}
                None => {
                    let _ = child.kill();
                    return Err(CliError::NoOutput);
                }
            }
        }
    }
}

/// Runs `omnideck <args> --json`, returning the parsed stdout JSON value.
///
/// Does not itself distinguish a "single value" response from an NDJSON
/// stream — callers doing streaming commands (`add`/`update`/`remove`/`logs
/// --follow`) should use [`run_ndjson_stream`] instead; this helper is for
/// the single-shot commands (`list`, `status`, `doctor`, `--version`, ...).
async fn run_json<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    args: &[&str],
    timeout_duration: Duration,
) -> Result<Value, CliError> {
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.push("--json");

    let result = run_cli(app, &full_args, timeout_duration, |_| {}).await?;

    // stdout is exclusively machine-readable under --json; stderr may carry
    // incidental warnings that are not part of the contract (JSON_MODE_SPEC
    // §1) — never merge the two streams.
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stdout = stdout.trim();

    if stdout.is_empty() {
        return Err(CliError::NoOutput);
    }

    let value: Value = serde_json::from_str(stdout).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: stdout.to_string(),
    })?;

    // Exit code alone never distinguishes a structured error from a valid
    // "non-affirmative state" body (JSON_MODE_SPEC §8) — always inspect the
    // body itself for the error envelope's shape.
    if let Value::Object(map) = &value {
        if let Some(err_val) = map.get("error") {
            let body: ErrorBody =
                serde_json::from_value(err_val.clone()).map_err(|e| CliError::Parse {
                    message: e.to_string(),
                    raw: stdout.to_string(),
                })?;
            return Err(CliError::Cli(Box::new(CliErrorBody {
                code: body.code,
                message: body.message,
                hint: body.hint,
                detail: body.detail,
                action: body.action,
                action_value: body.action_value,
                instances: body.instances,
            })));
        }
    }

    Ok(value)
}

/// `omnideck --version --json`, validated against [`EXPECTED_JSON_CONTRACT`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub commit: String,
    pub date: String,
    #[serde(rename = "jsonContract")]
    pub json_contract: u64,
}

pub async fn version<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<VersionInfo, CliError> {
    let value = run_json(app, &["--version"], INSPECTION_TIMEOUT).await?;
    let info: VersionInfo = serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })?;

    if info.json_contract != EXPECTED_JSON_CONTRACT {
        return Err(CliError::ContractMismatch {
            expected: EXPECTED_JSON_CONTRACT,
            actual: info.json_contract,
        });
    }

    if !meets_minimum_version(&info.version, MINIMUM_CLI_VERSION) {
        return Err(CliError::VersionTooOld {
            minimum: MINIMUM_CLI_VERSION.to_string(),
            actual: info.version.clone(),
        });
    }

    Ok(info)
}

/// `runtimeStatusPayload`'s schema version (`cmd/runtime.go` in the CLI
/// repo) — independent of [`EXPECTED_JSON_CONTRACT`], see
/// [`CliError::RuntimeSchemaMismatch`].
pub const EXPECTED_RUNTIME_SCHEMA: u32 = 4;

/// `runtime status`/`runtime ensure --json`'s shape — the *shared*, not
/// per-instance, Podman runtime's readiness. Consumed by `bootstrap.rs`.
/// Field shape confirmed directly against `cmd/runtime.go`'s
/// `runtimeStatusPayload`, not assumed from docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub schema_version: u32,
    pub runtime: String,
    pub state: String,
    pub ready: bool,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(rename = "machineName", default)]
    pub machine_name: Option<String>,
    pub phase: String,
    pub activity: String,
    pub resources: RuntimeResources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeResources {
    pub container: RuntimeContainerResources,
    pub machine: RuntimeMachineResources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeContainerResources {
    pub memory: String,
    #[serde(rename = "shmSize")]
    pub shm_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMachineResources {
    pub mode: String,
    #[serde(default)]
    pub cpus: Option<u32>,
    #[serde(rename = "memoryMB", default)]
    pub memory_mb: Option<u64>,
    #[serde(rename = "diskGB", default)]
    pub disk_gb: Option<u32>,
}

fn parse_runtime_status(value: Value) -> Result<RuntimeStatus, CliError> {
    let status: RuntimeStatus =
        serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
            message: e.to_string(),
            raw: value.to_string(),
        })?;
    if status.schema_version != EXPECTED_RUNTIME_SCHEMA {
        return Err(CliError::RuntimeSchemaMismatch {
            expected: EXPECTED_RUNTIME_SCHEMA,
            actual: status.schema_version,
        });
    }
    Ok(status)
}

/// `omnideck runtime status --json` — cheap, side-effect-free inspection of
/// the shared Podman runtime. Never installs or starts anything; see
/// [`runtime_ensure`] for that.
pub async fn runtime_status<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<RuntimeStatus, CliError> {
    let value = run_json(app, &["runtime", "status"], INSPECTION_TIMEOUT).await?;
    parse_runtime_status(value)
}

/// One line of `runtime ensure --json`'s NDJSON progress stream when there's
/// actual setup work to do — distinct from [`StreamEvent`] (`add`/`update`/
/// `remove`'s shape): carries `activity`/`progress` instead of `result`.
/// Modeled directly on the CLI's `RuntimeSetupEvent` (`engine/runtime_setup.go`).
///
/// `state` *is* captured here (unlike before CLI `v0.11.0-alpha.2`/contract
/// `3`): the caller ([`runtime_ensure`]) still branches on `error`/`complete`
/// before reaching this struct, but the remaining values now matter beyond
/// just "start"/"progress" — a real `"permission"` state means the CLI is
/// about to show (or is showing) a native OS permission prompt and is
/// waiting on the user, not doing background work. `bootstrap.rs` uses this
/// to follow the sibling's setup-UX principle of keeping native prompts
/// visible and giving truthful "waiting for you" copy instead of a
/// synthesized progress percentage. `substage`/`status` are also new in this
/// contract version: `substage` is a stable machine-readable id (e.g.
/// `"wsl-permission"`, `"package-index"`) for diagnostics; `status` is a
/// short human label (e.g. `"Password required"`) distinct from the longer
/// `activity` headline — see `engine/runtime_setup_linux_host.go` for a
/// worked example of all four together.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSetupEvent {
    pub stage: String,
    #[serde(default)]
    pub substage: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub activity: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub progress: Option<f64>,
}

/// Runs `runtime ensure --json`, calling `on_event` for each progress line,
/// and resolving to the final [`RuntimeStatus`]. Two real response shapes
/// from the CLI, both handled here: if the runtime is already ready, this
/// is a **single JSON value** (a bare `RuntimeStatus`, no envelope, no
/// events) — the CLI has nothing to do and says so immediately. Otherwise
/// it's an **NDJSON stream** of `RuntimeSetupEvent` lines (`stage` is
/// `"software"` or `"environment"`, JSON_MODE_SPEC's shared progress shape)
/// ending in `{"stage":"complete","state":"done","result":<RuntimeStatus>}`
/// or a `state:"error"` line. `on_event` is only called for the two real
/// progress stages, not the terminal complete/error envelope lines.
pub async fn runtime_ensure<R, F>(
    app: &tauri::AppHandle<R>,
    mut on_event: F,
) -> Result<RuntimeStatus, CliError>
where
    R: tauri::Runtime,
    F: FnMut(RuntimeSetupEvent),
{
    let mut outcome: Option<Result<Value, CliError>> = None;

    run_cli(
        app,
        &["runtime", "ensure", "--json"],
        MUTATION_TIMEOUT,
        |line| {
            let parsed: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    outcome = Some(Err(CliError::Parse {
                        message: e.to_string(),
                        raw: line.to_string(),
                    }));
                    return;
                }
            };

            // A bare RuntimeStatus (already ready) has no "stage" field —
            // NDJSON progress/envelope lines always do.
            let Some(stage) = parsed.get("stage").and_then(Value::as_str) else {
                outcome = Some(Ok(parsed));
                return;
            };

            let state = parsed
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default();

            if state == "error" {
                if let Some(err_val) = parsed.get("error").cloned() {
                    if let Ok(body) = serde_json::from_value::<ErrorBody>(err_val) {
                        outcome = Some(Err(CliError::Cli(Box::new(CliErrorBody {
                            code: body.code,
                            message: body.message,
                            hint: body.hint,
                            detail: body.detail,
                            action: body.action,
                            action_value: body.action_value,
                            instances: body.instances,
                        }))));
                    }
                }
                return;
            }
            if stage == "complete" && state == "done" {
                outcome = Some(Ok(parsed.get("result").cloned().unwrap_or(Value::Null)));
                return;
            }

            // A real "software"/"environment" progress line.
            if let Ok(event) = serde_json::from_value::<RuntimeSetupEvent>(parsed) {
                on_event(event);
            }
        },
    )
    .await?;

    let value = outcome.unwrap_or(Err(CliError::NoOutput))?;
    parse_runtime_status(value)
}

/// One row of `omnideck list --json`. The five live-stat fields plus
/// `uptime`/`restarts` are explicit JSON `null` (never omitted, never a
/// misleading zero) whenever the instance isn't active — modeled here as
/// `Option`, not defaulted, so the frontend can render a real placeholder
/// instead of a fake "0%" (JSON_MODE_SPEC §4, `list --json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListEntry {
    pub name: String,
    pub container: String,
    pub status: String,
    pub image: String,
    #[serde(rename = "webUiPort")]
    pub web_ui_port: String,
    pub cpu: Option<String>,
    #[serde(rename = "cpuPct")]
    pub cpu_pct: Option<f64>,
    pub ram: Option<String>,
    #[serde(rename = "ramTotal")]
    pub ram_total: Option<String>,
    #[serde(rename = "ramPct")]
    pub ram_pct: Option<f64>,
    pub uptime: Option<String>,
    pub restarts: Option<u32>,
    /// `""` when no healthcheck is configured — always a plain string, never
    /// null (JSON_MODE_SPEC §4).
    pub health: String,
    pub created: String,
}

pub async fn list<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Vec<ListEntry>, CliError> {
    let value = run_json(app, &["list"], INSPECTION_TIMEOUT).await?;
    serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })
}

/// A volume's existence, part of the `status --json` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeStatus {
    pub name: String,
    pub exists: bool,
}

/// Ollama reachability, part of the `status --json` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaStatus {
    pub reachable: bool,
    pub host: String,
}

/// `omnideck status --json` — also the exact shape reused by `start`/`stop`/
/// `restart --json` (JSON_MODE_SPEC §4/§6) once the action completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceStatus {
    pub name: String,
    pub container: String,
    pub status: String,
    pub image: String,
    pub engine: String,
    #[serde(rename = "webUiPort")]
    pub web_ui_port: String,
    #[serde(rename = "homeVolume")]
    pub home_volume: VolumeStatus,
    #[serde(rename = "stateVolume")]
    pub state_volume: VolumeStatus,
    pub ollama: OllamaStatus,
}

fn parse_instance_status(value: Value) -> Result<InstanceStatus, CliError> {
    serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })
}

pub async fn status<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<InstanceStatus, CliError> {
    let value = run_json(app, &["status", "--name", name], INSPECTION_TIMEOUT).await?;
    parse_instance_status(value)
}

/// `start`/`stop`/`restart --json` all re-gather and return the same
/// `status --json` shape fresh after the action completes (JSON_MODE_SPEC
/// §6) — no follow-up `status` call needed.
pub async fn start<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<InstanceStatus, CliError> {
    let value = run_json(app, &["start", "--name", name], INSPECTION_TIMEOUT).await?;
    parse_instance_status(value)
}

pub async fn stop<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<InstanceStatus, CliError> {
    let value = run_json(app, &["stop", "--name", name], INSPECTION_TIMEOUT).await?;
    parse_instance_status(value)
}

pub async fn restart<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<InstanceStatus, CliError> {
    let value = run_json(app, &["restart", "--name", name], INSPECTION_TIMEOUT).await?;
    parse_instance_status(value)
}

/// One check from `doctor --json` (JSON_MODE_SPEC §4/§7). `action`/
/// `actionLabel`/`actionValue` are omitted together when there's nothing
/// actionable — surfaced here as `None`, not empty strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub label: String,
    pub status: String,
    pub detail: String,
    pub hint: String,
    pub action: Option<String>,
    #[serde(rename = "actionLabel")]
    pub action_label: Option<String>,
    #[serde(rename = "actionValue")]
    pub action_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorResult {
    pub checks: Vec<DoctorCheck>,
    #[serde(rename = "allPass")]
    pub all_pass: bool,
}

/// `doctor --name <name> --json` (DESIGN.md #6). Exit code is `1` whenever
/// `allPass` is `false` (JSON_MODE_SPEC §4), but the body is still valid —
/// `run_json` already ignores exit code and parses stdout regardless.
pub async fn doctor<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<DoctorResult, CliError> {
    let value = run_json(app, &["doctor", "--name", name], INSPECTION_TIMEOUT).await?;
    serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })
}

/// `config show --name <name> --json` (JSON_MODE_SPEC §4) — read-only in
/// this app for now; `config set` isn't wired up yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigInfo {
    #[serde(rename = "containerName")]
    pub container_name: String,
    #[serde(rename = "homeVolume")]
    pub home_volume: String,
    #[serde(rename = "stateVolume")]
    pub state_volume: String,
    pub memory: String,
    #[serde(rename = "shmSize")]
    pub shm_size: String,
    #[serde(rename = "webUiPort")]
    pub web_ui_port: String,
    pub runtime: String,
    pub image: String,
    #[serde(rename = "installedAt")]
    pub installed_at: String,
}

pub async fn config<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<ConfigInfo, CliError> {
    let value = run_json(app, &["config", "show", "--name", name], INSPECTION_TIMEOUT).await?;
    serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })
}

/// `omnideck logs --json` (non-follow — JSON_MODE_SPEC §4). Follow mode is a
/// separate NDJSON stream, not handled by this helper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsResult {
    pub lines: Vec<String>,
}

pub async fn logs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
    tail: u32,
) -> Result<LogsResult, CliError> {
    let tail_arg = tail.to_string();
    let value = run_json(
        app,
        &["logs", "--name", name, "--tail", &tail_arg],
        INSPECTION_TIMEOUT,
    )
    .await?;
    serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })
}

/// One line of an `add`/`update`/`remove --json` NDJSON stream
/// (JSON_MODE_SPEC §5's shared envelope). Forwarded to the frontend as-is
/// via a Tauri event — the per-stage shapes are meant to be read live by
/// whichever progress UI is driving the flow, not re-modeled per stage here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub stage: String,
    pub state: String,
    pub detail: Option<String>,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

/// Runs an NDJSON-streaming command (`add`/`update`/`remove`), emitting each
/// parsed line to the frontend as `event_name` and resolving once the
/// stream's final line arrives. JSON_MODE_SPEC §5 guarantees the final line
/// is always exactly one of `stage:"complete",state:"done"` (success,
/// carries `result`) or any `state:"error"` (carries the standard error
/// envelope) — nothing follows either, so seeing one ends the read loop.
/// Built on [`run_cli`], so it inherits bounded output, a timeout, and
/// correct line reassembly across chunk boundaries for free.
async fn run_ndjson_stream<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    args: &[&str],
    event_name: &str,
    timeout_duration: Duration,
) -> Result<Value, CliError> {
    let mut outcome: Option<Result<Value, CliError>> = None;

    run_cli(app, args, timeout_duration, |line| {
        let parsed: StreamEvent = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                outcome = Some(Err(CliError::Parse {
                    message: e.to_string(),
                    raw: line.to_string(),
                }));
                return;
            }
        };
        let _ = app.emit(event_name, &parsed);

        if parsed.state == "error" {
            if let Some(err_val) = parsed.error.clone() {
                if let Ok(body) = serde_json::from_value::<ErrorBody>(err_val) {
                    outcome = Some(Err(CliError::Cli(Box::new(CliErrorBody {
                        code: body.code,
                        message: body.message,
                        hint: body.hint,
                        detail: body.detail,
                        action: body.action,
                        action_value: body.action_value,
                        instances: body.instances,
                    }))));
                }
            }
        } else if parsed.stage == "complete" && parsed.state == "done" {
            outcome = Some(Ok(parsed.result.clone().unwrap_or(Value::Null)));
        }
    })
    .await?;

    outcome.unwrap_or(Err(CliError::NoOutput))
}

/// `add --suggest-defaults --json` — non-mutating, previews what a plain
/// `add` would pick, for pre-filling a New Deck form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSuggestion {
    pub name: String,
    #[serde(rename = "webUiPort")]
    pub web_ui_port: String,
}

pub async fn suggest_defaults<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<AddSuggestion, CliError> {
    let value = run_json(app, &["add", "--suggest-defaults"], INSPECTION_TIMEOUT).await?;
    serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })
}

pub struct NewDeckOptions<'a> {
    pub name: &'a str,
    pub port: &'a str,
    pub image: Option<&'a str>,
    pub memory: Option<&'a str>,
}

/// `add --json` (DESIGN.md #4) — streams progress via the `"add-progress"`
/// event, resolves to the new instance's status on success.
pub async fn add<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    opts: NewDeckOptions<'_>,
) -> Result<InstanceStatus, CliError> {
    let mut args: Vec<&str> = vec!["add", "--name", opts.name, "--port", opts.port, "--json"];
    if let Some(image) = opts.image {
        args.push("--image");
        args.push(image);
    }
    if let Some(memory) = opts.memory {
        args.push("--memory");
        args.push(memory);
    }
    let value = run_ndjson_stream(app, &args, "add-progress", MUTATION_TIMEOUT).await?;
    parse_instance_status(value)
}

/// `update --json` — stages are `pull_image` (with `progress` events) then
/// `recreate` (JSON_MODE_SPEC §5). A cancelled/failed update rolls back to
/// the previous working container automatically on the CLI side; this
/// resolves to the post-update status on success.
pub async fn update<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<InstanceStatus, CliError> {
    let value = run_ndjson_stream(
        app,
        &["update", "--name", name, "--json"],
        "update-progress",
        MUTATION_TIMEOUT,
    )
    .await?;
    parse_instance_status(value)
}

/// `remove --json` (DESIGN.md #5) final result — `removedVolumes` is `[]`,
/// never null, when nothing was deleted; `backupPath` is present only when a
/// backup was actually made.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveResult {
    #[serde(rename = "containerStopped")]
    pub container_stopped: bool,
    #[serde(rename = "containerRemoved")]
    pub container_removed: bool,
    #[serde(rename = "removedVolumes")]
    pub removed_volumes: Vec<String>,
    #[serde(rename = "backupPath")]
    pub backup_path: Option<String>,
}

pub struct RemoveOptions {
    /// `true` → `--keep-volumes`, `false` → `--delete-volumes`.
    pub keep_volumes: bool,
    /// Only consulted when `keep_volumes` is `false` — `true` → `--backup`,
    /// `false` → `--no-backup`.
    pub backup: bool,
}

/// `remove <name> --yes --json` (DESIGN.md #5) — the CLI requires explicit,
/// non-defaulted choices here (JSON_MODE_SPEC §5); this signature mirrors
/// that by requiring both fields of [`RemoveOptions`] rather than defaulting
/// either.
pub async fn remove<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
    opts: RemoveOptions,
) -> Result<RemoveResult, CliError> {
    let mut args: Vec<&str> = vec!["remove", name, "--yes", "--json"];
    if opts.keep_volumes {
        args.push("--keep-volumes");
    } else {
        args.push("--delete-volumes");
        args.push(if opts.backup {
            "--backup"
        } else {
            "--no-backup"
        });
    }

    let value = run_ndjson_stream(app, &args, "remove-progress", MUTATION_TIMEOUT).await?;
    serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_bounded() {
        let mut output = Vec::new();
        append_bounded(&mut output, &[b'a'; 4], 5, "stdout").unwrap();
        append_bounded(&mut output, b"b", 5, "stdout").unwrap();
        let error = append_bounded(&mut output, b"c", 5, "stdout").unwrap_err();
        assert!(matches!(
            error,
            CliError::OutputLimitExceeded { stream } if stream == "stdout"
        ));
    }

    #[test]
    fn json_lines_are_reassembled_across_process_chunks() {
        let mut buffer = LineBuffer::default();
        let mut lines = Vec::new();
        buffer.push(br#"{"stage":"pull"#, &mut |line| {
            lines.push(line.to_owned())
        });
        buffer.push(b"_image\"}\r\n{\"stage\":\"start", &mut |line| {
            lines.push(line.to_owned())
        });
        buffer.push(b"_container\"}", &mut |line| lines.push(line.to_owned()));
        buffer.flush(&mut |line| lines.push(line.to_owned()));
        assert_eq!(
            lines,
            [
                r#"{"stage":"pull_image"}"#,
                r#"{"stage":"start_container"}"#
            ]
        );
    }

    /// The frontend's TS union expects the error envelope's fields flattened
    /// alongside `"kind":"cli"` (see src/types/cli.ts) — pins that serde's
    /// internally-tagged representation still flattens a boxed newtype
    /// variant the same way it would an inline struct variant.
    #[test]
    fn cli_error_serializes_flat() {
        let err = CliError::Cli(Box::new(CliErrorBody {
            code: "AMBIGUOUS_INSTANCE".to_string(),
            message: "Multiple instances found — specify --name".to_string(),
            hint: None,
            detail: None,
            action: None,
            action_value: None,
            instances: Some(vec!["demo".to_string(), "work".to_string()]),
        }));

        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "cli");
        assert_eq!(json["code"], "AMBIGUOUS_INSTANCE");
        assert_eq!(json["instances"], serde_json::json!(["demo", "work"]));
    }

    /// Canned fixture straight from JSON_MODE_SPEC.md §4 — `list --json`'s
    /// null live-stat fields for a stopped instance must round-trip as
    /// `None`, not a default/zero value.
    #[test]
    fn list_entry_null_stats_for_stopped_instance() {
        let raw = r#"{
            "name": "staging",
            "container": "staging",
            "status": "exited",
            "image": "ghcr.io/omnideck-dev/omnideck:latest",
            "webUiPort": "2338",
            "cpu": null,
            "cpuPct": null,
            "ram": null,
            "ramTotal": null,
            "ramPct": null,
            "uptime": null,
            "restarts": null,
            "health": "",
            "created": "2026-06-01"
        }"#;

        let entry: ListEntry = serde_json::from_str(raw).unwrap();
        assert_eq!(entry.status, "exited");
        assert_eq!(entry.cpu, None);
        assert_eq!(entry.ram_pct, None);
        assert_eq!(entry.restarts, None);
        assert_eq!(entry.health, "");
    }

    /// Canned fixture from JSON_MODE_SPEC.md §4 `status --json`.
    #[test]
    fn instance_status_parses_full_shape() {
        let raw = r#"{
            "name": "omnideck",
            "container": "omnideck",
            "status": "running",
            "image": "ghcr.io/omnideck-dev/omnideck:latest",
            "engine": "podman",
            "webUiPort": "2337",
            "homeVolume": {"name": "omnideck-home", "exists": true},
            "stateVolume": {"name": "omnideck-state", "exists": true},
            "ollama": {"reachable": false, "host": "127.0.0.1:11434"}
        }"#;

        let status: InstanceStatus = serde_json::from_str(raw).unwrap();
        assert_eq!(status.status, "running");
        assert!(status.home_volume.exists);
        assert!(!status.ollama.reachable);
    }

    /// Canned fixture from a real `doctor --name demo --json` run — pins the
    /// per-check shape including a populated `action`/`actionLabel`/
    /// `actionValue` triple (JSON_MODE_SPEC §7).
    #[test]
    fn doctor_result_parses_with_action() {
        let raw = r#"{
            "checks": [
                {"label": "Container runtime", "status": "pass", "detail": "Podman is ready · 5.8.4", "hint": ""},
                {
                    "label": "Omnideck instance",
                    "status": "fail",
                    "detail": "demo is stopped",
                    "hint": "Run: omnideck start --name demo",
                    "action": "start_instance",
                    "actionLabel": "Start Omnideck",
                    "actionValue": "demo"
                }
            ],
            "allPass": false
        }"#;

        let result: DoctorResult = serde_json::from_str(raw).unwrap();
        assert!(!result.all_pass);
        assert_eq!(result.checks.len(), 2);
        assert_eq!(result.checks[1].action.as_deref(), Some("start_instance"));
        assert_eq!(result.checks[0].action, None);
    }

    /// Pins the exact string the real bundled `v0.11.0-alpha.2` sidecar
    /// reports (verified directly by running a downloaded release binary's
    /// `--version --json`, not assumed) so a future contract/version bump
    /// can't silently drift this fixture.
    #[test]
    fn version_info_parses_the_real_v0_11_0_alpha_2_shape() {
        let raw = r#"{"version":"v0.11.0-alpha.2","commit":"6ea721020691","date":"2026-08-09T23:44:42Z","jsonContract":3}"#;
        let info: VersionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.version, "v0.11.0-alpha.2");
        assert_eq!(info.json_contract, EXPECTED_JSON_CONTRACT);
        assert!(meets_minimum_version(&info.version, MINIMUM_CLI_VERSION));
    }

    #[test]
    fn minimum_version_floor_accepts_equal_and_newer() {
        assert!(meets_minimum_version("v0.10.0", "v0.10.0"));
        assert!(meets_minimum_version("v0.10.1", "v0.10.0"));
        assert!(meets_minimum_version("v0.11.0", "v0.10.0"));
        assert!(meets_minimum_version("v1.0.0", "v0.10.0"));
    }

    #[test]
    fn minimum_version_floor_rejects_older_and_prerelease_of_the_floor() {
        assert!(!meets_minimum_version("v0.9.1", "v0.10.0"));
        assert!(!meets_minimum_version("v0.10.0-alpha.2", "v0.10.0"));
        assert!(!meets_minimum_version("not-a-version", "v0.10.0"));
    }

    /// Canned fixture straight from a real `runtime status --json` run
    /// against this host's actual podman install (host-native mode, no
    /// managed machine) — captured directly, not invented.
    #[test]
    fn runtime_status_parses_the_real_host_native_shape() {
        let raw = serde_json::json!({
            "schemaVersion": 4,
            "runtime": "podman",
            "state": "ready",
            "ready": true,
            "path": "/usr/bin/podman",
            "version": "5.8.4",
            "phase": "environment",
            "activity": "Preparing a secure space to run in…",
            "resources": {
                "container": {"memory": "6g", "shmSize": "3072m"},
                "machine": {"mode": "host-native"}
            }
        });
        let status = parse_runtime_status(raw).unwrap();
        assert!(status.ready);
        assert_eq!(status.resources.machine.mode, "host-native");
        assert_eq!(status.resources.machine.memory_mb, None);
    }

    #[test]
    fn runtime_status_rejects_wrong_schema() {
        let raw = serde_json::json!({
            "schemaVersion": 5,
            "runtime": "podman",
            "state": "ready",
            "ready": true,
            "phase": "environment",
            "activity": "x",
            "resources": {
                "container": {"memory": "2g", "shmSize": "1g"},
                "machine": {"mode": "host-native"}
            }
        });
        let error = parse_runtime_status(raw).unwrap_err();
        assert!(matches!(
            error,
            CliError::RuntimeSchemaMismatch {
                expected: 4,
                actual: 5
            }
        ));
    }

    /// Fixture from `cmd/runtime.go`'s `runtimeStatusPayload` for the
    /// "missing" case — `phase` switches to `"software"` before Podman
    /// exists at all.
    #[test]
    fn runtime_status_missing_uses_the_software_phase() {
        let raw = serde_json::json!({
            "schemaVersion": 4,
            "runtime": "podman",
            "state": "missing",
            "ready": false,
            "phase": "software",
            "activity": "Getting your computer ready…",
            "resources": {
                "container": {"memory": "2g", "shmSize": "1g"},
                "machine": {"mode": "podman-managed", "cpus": 4, "memoryMB": 4096, "diskGB": 40}
            }
        });
        let status = parse_runtime_status(raw).unwrap();
        assert!(!status.ready);
        assert_eq!(status.phase, "software");
        assert_eq!(status.resources.machine.memory_mb, Some(4096));
    }

    /// Fixture matching `cmd/runtime.go`'s `runtimeSetupEventPayload` shape
    /// for a real in-progress line (not the terminal complete/error).
    #[test]
    fn runtime_setup_event_parses_a_progress_line() {
        let raw = r#"{"stage":"software","state":"progress","activity":"Getting your computer ready…","detail":"Downloading Podman…","progress":0.4}"#;
        let event: RuntimeSetupEvent = serde_json::from_str(raw).unwrap();
        assert_eq!(event.stage, "software");
        assert_eq!(event.state.as_deref(), Some("progress"));
        assert_eq!(event.detail.as_deref(), Some("Downloading Podman…"));
        assert_eq!(event.progress, Some(0.4));
        assert_eq!(event.substage, None);
        assert_eq!(event.status, None);
    }

    /// Real shape from `engine/runtime_setup_linux_host.go`'s permission-wait
    /// event (CLI `v0.11.0-alpha.2`, contract `3`) — the first real use of
    /// `state: "permission"`/`substage`/`status` together.
    #[test]
    fn runtime_setup_event_parses_a_permission_wait_line() {
        let raw = r#"{"stage":"software","substage":"linux-permission","state":"permission","activity":"Waiting for approval from your computer…","status":"Password required","detail":"Your computer will ask you to approve installing Podman — the software omnideck uses to run in an isolated space. omnideck never sees or stores your password."}"#;
        let event: RuntimeSetupEvent = serde_json::from_str(raw).unwrap();
        assert_eq!(event.state.as_deref(), Some("permission"));
        assert_eq!(event.substage.as_deref(), Some("linux-permission"));
        assert_eq!(event.status.as_deref(), Some("Password required"));
        assert_eq!(event.progress, None);
    }

    /// Canned fixture from a real `config show --name demo --json` run.
    #[test]
    fn config_info_parses() {
        let raw = r#"{
            "containerName": "demo",
            "homeVolume": "demo-home",
            "stateVolume": "demo-state",
            "memory": "1g",
            "shmSize": "512m",
            "webUiPort": "2338",
            "runtime": "podman",
            "image": "ghcr.io/omnideck-dev/omnideck:latest",
            "installedAt": "2026-07-11T22:28:14-04:00"
        }"#;

        let info: ConfigInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.container_name, "demo");
        assert_eq!(info.memory, "1g");
    }
}
