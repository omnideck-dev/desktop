mod bootstrap;
mod cli_bridge;
mod commands;

use bootstrap::BootstrapState;
use tauri::Manager;

/// Read-only packaged-build smoke check (`tests/hardware/validate-proof.mjs`
/// validates the proof this writes) — ported from the sibling repo's
/// `record_packaged_smoke`/`run_packaged_smoke`/`record_smoke_error` in
/// `lib.rs`. Only runs when `OMNIDECK_DESKTOP_SMOKE_FILE` is set (never in
/// normal use); performs exactly `--version` and `runtime status --json`,
/// never a mutating command, and writes a small proof file a real installed
/// build actually reached and parsed real CLI output on this OS — not a
/// fixture test against canned JSON.
fn record_packaged_smoke(
    version: &cli_bridge::VersionInfo,
    status: &cli_bridge::RuntimeStatus,
) -> std::io::Result<()> {
    let Some(path) = std::env::var_os("OMNIDECK_DESKTOP_SMOKE_FILE") else {
        return Ok(());
    };
    let proof = serde_json::json!({
        "cliVersion": version.version,
        "cliCommit": version.commit,
        "schemaVersion": status.schema_version,
        "ready": status.ready,
        "operations": ["--version", "--json runtime status"],
        "mutation": false,
    });
    std::fs::write(path, serde_json::to_vec_pretty(&proof)?)
}

fn record_smoke_error(error: &cli_bridge::CliError) {
    let Some(path) = std::env::var_os("OMNIDECK_DESKTOP_SMOKE_FILE") else {
        return;
    };
    let proof = serde_json::json!({
        "stage": "host-smoke-failed",
        "error": error.to_string(),
        "mutation": false,
    });
    if let Ok(encoded) = serde_json::to_vec_pretty(&proof) {
        let _ = std::fs::write(path, encoded);
    }
}

async fn run_packaged_smoke(app: tauri::AppHandle) {
    let version = match cli_bridge::version(&app).await {
        Ok(version) => version,
        Err(error) => return record_smoke_error(&error),
    };
    let status = match cli_bridge::runtime_status(&app).await {
        Ok(status) => status,
        Err(error) => return record_smoke_error(&error),
    };
    if let Err(error) = record_packaged_smoke(&version, &status) {
        eprintln!("failed to write packaged smoke proof: {error}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin registered (Tauri's own requirement, load-order
        // sensitive on Windows). A second launch just focuses the single "main"
        // window instead of opening a second process. No-op on macOS, which
        // already prevents a second instance at the OS level.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(BootstrapState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_instances,
            commands::cli_version_contract,
            commands::instance_status,
            commands::instance_logs,
            commands::start_instance,
            commands::stop_instance,
            commands::restart_instance,
            commands::update_instance,
            commands::instance_doctor,
            commands::instance_config,
            commands::suggest_new_deck_defaults,
            commands::add_instance,
            commands::remove_instance,
            bootstrap::bootstrap,
            bootstrap::begin_setup,
            bootstrap::run_action,
        ])
        .setup(|app| {
            if std::env::var_os("OMNIDECK_DESKTOP_SMOKE_FILE").is_some() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(run_packaged_smoke(handle));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
