//! `#[tauri::command]` surface the frontend calls. Every command here is a
//! thin wrapper over `cli_bridge` — no podman/docker/CLI-spawn logic lives
//! in this file, per `AGENT.md`'s conventions.

use crate::cli_bridge::{self, CliError, ListEntry, VersionInfo};

#[tauri::command]
pub async fn list_instances(app: tauri::AppHandle) -> Result<Vec<ListEntry>, CliError> {
    cli_bridge::list(&app).await
}

#[tauri::command]
pub async fn cli_version_contract(app: tauri::AppHandle) -> Result<VersionInfo, CliError> {
    cli_bridge::version(&app).await
}
