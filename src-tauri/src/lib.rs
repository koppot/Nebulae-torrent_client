pub mod commands;
pub mod db;
pub mod models;
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
            // Diagnostics
            commands::diagnostics::open_logs_folder,
            commands::diagnostics::get_diagnostic_summary,
            commands::diagnostics::clear_cache,
            commands::diagnostics::reset_settings,
            commands::diagnostics::export_logs,
            // Settings
            commands::settings::load_user_settings,
            commands::settings::save_user_settings,
            // Torrent
            commands::torrent::list_torrents,
            commands::torrent::add_torrent_stub,
            commands::torrent::pause_torrent_stub,
            commands::torrent::resume_torrent_stub,
            // Smart Peering
            commands::peering::get_peering_settings,
            commands::peering::save_peering_settings,
            commands::peering::get_peer_telemetry,
            // Connectivity Doctor
            commands::connectivity::run_connectivity_doctor_test,
            commands::connectivity::retest_listening_port,
            commands::connectivity::get_connectivity_certificate,
            // VPN
            commands::vpn::get_vpn_status,
            commands::vpn::set_vpn_mode,
            commands::vpn::toggle_kill_switch,
            commands::vpn::import_wireguard_profile,
            commands::vpn::import_openvpn_profile,
            commands::vpn::list_vpn_profiles,
            commands::vpn::delete_vpn_profile,
            // AsianDVDClub Tracker Integration
            commands::tracker::get_adc_tracker_profile,
            commands::tracker::link_adc_account,
            commands::tracker::unlink_adc_account,
            commands::tracker::get_seed_time_records,
            commands::tracker::calculate_ratio_projection,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running ADC application");
}
