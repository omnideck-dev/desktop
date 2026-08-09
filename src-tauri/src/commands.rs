//! `#[tauri::command]` surface the frontend calls. Every command here is a
//! thin wrapper over `cli_bridge` — no podman/docker/CLI-spawn logic lives
//! in this file, per `AGENT.md`'s conventions.

use crate::cli_bridge::{
    self, AddSuggestion, CliError, ConfigInfo, DoctorResult, InstanceStatus, ListEntry, LogsResult,
    NewDeckOptions, RemoveOptions, RemoveResult, VersionInfo,
};

#[tauri::command]
pub async fn list_instances(app: tauri::AppHandle) -> Result<Vec<ListEntry>, CliError> {
    cli_bridge::list(&app).await
}

#[tauri::command]
pub async fn cli_version_contract(app: tauri::AppHandle) -> Result<VersionInfo, CliError> {
    cli_bridge::version(&app).await
}

#[tauri::command]
pub async fn instance_status(
    app: tauri::AppHandle,
    name: String,
) -> Result<InstanceStatus, CliError> {
    cli_bridge::status(&app, &name).await
}

#[tauri::command]
pub async fn instance_logs(
    app: tauri::AppHandle,
    name: String,
    tail: u32,
) -> Result<LogsResult, CliError> {
    cli_bridge::logs(&app, &name, tail).await
}

#[tauri::command]
pub async fn start_instance(
    app: tauri::AppHandle,
    name: String,
) -> Result<InstanceStatus, CliError> {
    cli_bridge::start(&app, &name).await
}

#[tauri::command]
pub async fn stop_instance(
    app: tauri::AppHandle,
    name: String,
) -> Result<InstanceStatus, CliError> {
    cli_bridge::stop(&app, &name).await
}

#[tauri::command]
pub async fn restart_instance(
    app: tauri::AppHandle,
    name: String,
) -> Result<InstanceStatus, CliError> {
    cli_bridge::restart(&app, &name).await
}

#[tauri::command]
pub async fn update_instance(
    app: tauri::AppHandle,
    name: String,
) -> Result<InstanceStatus, CliError> {
    cli_bridge::update(&app, &name).await
}

#[tauri::command]
pub async fn instance_doctor(
    app: tauri::AppHandle,
    name: String,
) -> Result<DoctorResult, CliError> {
    cli_bridge::doctor(&app, &name).await
}

#[tauri::command]
pub async fn instance_config(app: tauri::AppHandle, name: String) -> Result<ConfigInfo, CliError> {
    cli_bridge::config(&app, &name).await
}

#[tauri::command]
pub async fn suggest_new_deck_defaults(app: tauri::AppHandle) -> Result<AddSuggestion, CliError> {
    cli_bridge::suggest_defaults(&app).await
}

#[tauri::command]
pub async fn add_instance(
    app: tauri::AppHandle,
    name: String,
    port: String,
    image: Option<String>,
    memory: Option<String>,
) -> Result<InstanceStatus, CliError> {
    cli_bridge::add(
        &app,
        NewDeckOptions {
            name: &name,
            port: &port,
            image: image.as_deref(),
            memory: memory.as_deref(),
        },
    )
    .await
}

#[tauri::command]
pub async fn remove_instance(
    app: tauri::AppHandle,
    name: String,
    keep_volumes: bool,
    backup: bool,
) -> Result<RemoveResult, CliError> {
    cli_bridge::remove(
        &app,
        &name,
        RemoveOptions {
            keep_volumes,
            backup,
        },
    )
    .await
}
