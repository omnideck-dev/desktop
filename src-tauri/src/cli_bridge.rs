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
use tauri_plugin_shell::ShellExt;

/// The `jsonContract` version this app build was written against
/// (`JSON_MODE_SPEC.md` §1). Bump only on a deliberate, reviewed change once
/// the CLI's contract itself changes.
pub const EXPECTED_JSON_CONTRACT: u64 = 1;

const CLI_BIN: &str = "omnideck";

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
    Cli {
        code: String,
        message: String,
        hint: Option<String>,
        action: Option<String>,
        #[serde(rename = "actionValue")]
        action_value: Option<String>,
        instances: Option<Vec<String>>,
    },
    /// The CLI's `jsonContract` doesn't match what this app build expects.
    ContractMismatch { expected: u64, actual: u64 },
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Spawn { message } => write!(f, "failed to run omnideck CLI: {message}"),
            CliError::NoOutput => write!(f, "omnideck CLI produced no output"),
            CliError::Parse { message, .. } => {
                write!(f, "failed to parse omnideck CLI output: {message}")
            }
            CliError::Cli { code, message, .. } => write!(f, "omnideck CLI error [{code}]: {message}"),
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

    let output = app
        .shell()
        .command(CLI_BIN)
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
            return Err(CliError::Cli {
                code: body.code,
                message: body.message,
                hint: body.hint,
                action: body.action,
                action_value: body.action_value,
                instances: body.instances,
            });
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
