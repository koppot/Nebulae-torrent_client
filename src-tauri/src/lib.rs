pub mod commands;
pub mod db;
pub mod utils;

use std::sync::Arc;
use utils::paths::AppPaths;
use db::Database;
use utils::logger::SafeLogger;

pub fn run() {
    // 1. Initialize isolated internal directory layout: ADC/internal/
    let paths = match AppPaths::init() {
        Ok(p) => Arc::new(p),
        Err(e) => {
            eprintln!("Fatal error initializing application paths: {e}");
            return;
        }
    };

    // 2. Initialize internal SQLite persistence
    let db = match Database::new(&paths.db_file) {
        Ok(d) => Arc::new(d),
        Err(e) => {
            eprintln!("Fatal error initializing SQLite database: {e}");
            return;
        }
    };

    SafeLogger::log_to_file(&paths.logs_dir, "INFO", "ADC application initialized successfully");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(paths)
        .manage(db)
        .invoke_handler(tauri::generate_handler![
            commands::diagnostics::open_logs_folder,
            commands::diagnostics::get_diagnostic_summary,
            commands::diagnostics::clear_cache,
            commands::diagnostics::reset_settings,
            commands::diagnostics::export_logs,
            commands::settings::load_user_settings,
            commands::settings::save_user_settings,
            commands::torrent::list_torrents,
            commands::torrent::add_torrent_stub,
            commands::torrent::pause_torrent_stub,
            commands::torrent::resume_torrent_stub,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running ADC application");
}
