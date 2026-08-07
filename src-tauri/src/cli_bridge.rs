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
use tauri::Emitter;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// The `jsonContract` version this app build was written against
/// (`JSON_MODE_SPEC.md` §1). Bump only on a deliberate, reviewed change once
/// the CLI's contract itself changes.
pub const EXPECTED_JSON_CONTRACT: u64 = 1;

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
}

#[derive(Debug, Clone, Serialize)]
pub struct CliErrorBody {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
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
        }
    }
}

impl std::error::Error for CliError {}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
    hint: Option<String>,
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

/// Runs `omnideck <args> --json`, returning the parsed stdout JSON value.
///
/// Does not itself distinguish a "single value" response from an NDJSON
/// stream — callers doing streaming commands (`add`/`update`/`remove`/`logs
/// --follow`) should use the process directly instead; this helper is for
/// the single-shot commands (`list`, `status`, `doctor`, `--version`, ...).
async fn run_json<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    args: &[&str],
) -> Result<Value, CliError> {
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.push("--json");

    let output = sidecar_command(app)?
        .args(&full_args)
        .output()
        .await
        .map_err(|e| CliError::Spawn {
            message: e.to_string(),
        })?;

    // stdout is exclusively machine-readable under --json; stderr may carry
    // incidental warnings that are not part of the contract (JSON_MODE_SPEC
    // §1) — never merge the two streams.
    let stdout = String::from_utf8_lossy(&output.stdout);
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
    let value = run_json(app, &["--version"]).await?;
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

    Ok(info)
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
    let value = run_json(app, &["list"]).await?;
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
    let value = run_json(app, &["status", "--name", name]).await?;
    parse_instance_status(value)
}

/// `start`/`stop`/`restart --json` all re-gather and return the same
/// `status --json` shape fresh after the action completes (JSON_MODE_SPEC
/// §6) — no follow-up `status` call needed.
pub async fn start<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<InstanceStatus, CliError> {
    let value = run_json(app, &["start", "--name", name]).await?;
    parse_instance_status(value)
}

pub async fn stop<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<InstanceStatus, CliError> {
    let value = run_json(app, &["stop", "--name", name]).await?;
    parse_instance_status(value)
}

pub async fn restart<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
) -> Result<InstanceStatus, CliError> {
    let value = run_json(app, &["restart", "--name", name]).await?;
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
    let value = run_json(app, &["doctor", "--name", name]).await?;
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
    let value = run_json(app, &["config", "show", "--name", name]).await?;
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
    let value = run_json(app, &["logs", "--name", name, "--tail", &tail_arg]).await?;
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
async fn run_ndjson_stream<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    args: &[&str],
    event_name: &str,
) -> Result<Value, CliError> {
    let (mut rx, _child) =
        sidecar_command(app)?
            .args(args)
            .spawn()
            .map_err(|e| CliError::Spawn {
                message: e.to_string(),
            })?;

    let mut outcome: Option<Result<Value, CliError>> = None;

    while let Some(event) = rx.recv().await {
        let CommandEvent::Stdout(bytes) = event else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parsed: StreamEvent = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    outcome = Some(Err(CliError::Parse {
                        message: e.to_string(),
                        raw: line.to_string(),
                    }));
                    continue;
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
                            action: body.action,
                            action_value: body.action_value,
                            instances: body.instances,
                        }))));
                    }
                }
            } else if parsed.stage == "complete" && parsed.state == "done" {
                outcome = Some(Ok(parsed.result.clone().unwrap_or(Value::Null)));
            }
        }
    }

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
    let value = run_json(app, &["add", "--suggest-defaults"]).await?;
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
    let value = run_ndjson_stream(app, &args, "add-progress").await?;
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

    let value = run_ndjson_stream(app, &args, "remove-progress").await?;
    serde_json::from_value(value.clone()).map_err(|e| CliError::Parse {
        message: e.to_string(),
        raw: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
