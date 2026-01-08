mod hosts;
pub mod storage;
pub mod cli;

use tauri::Manager;
use window_vibrancy::apply_mica;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Check CLI args
            if cli::run_cli(app.handle()) {
                std::process::exit(0);
            }

            #[cfg(target_os = "windows")]
            {
                let window = app.get_webview_window("main").unwrap();
                // Experimental Mica
                let _ = apply_mica(&window, Some(true));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            hosts::get_system_hosts,
            hosts::save_system_hosts,
            hosts::check_write_permission,
            hosts::hostly_open_url,
            storage::load_config,
            storage::load_common_config,
            storage::save_common_config,
            storage::list_profiles,
            storage::create_profile,
            storage::save_profile_content,
            storage::delete_profile,
            storage::rename_profile,
            storage::toggle_profile_active,
            storage::set_multi_select,
            storage::apply_config,
            storage::import_file,
            storage::export_file,
            storage::import_data,
            storage::export_data,
            storage::import_switchhosts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
