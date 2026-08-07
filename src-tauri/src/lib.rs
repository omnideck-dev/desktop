mod cli_bridge;
mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
