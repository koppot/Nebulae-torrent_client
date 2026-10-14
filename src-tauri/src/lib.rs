pub mod commands;
pub mod db;
pub mod engine;
pub mod models;
pub mod utils;

use db::Database;
use engine::SharedEngine;
use std::sync::Arc;
use utils::logger::SafeLogger;
use utils::paths::AppPaths;

pub fn run() {
    // 0. Initialize privacy-safe tracing subscriber (redacting passkeys and tracker URLs)
    SafeLogger::init_tracing();

    // 1. Initialize isolated internal directory layout: Nebulae/internal/
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

    // 3. Resolve user settings for engine session (listening_port & download_directory)
    let settings = db
        .get_setting("user_settings_json")
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str::<commands::settings::UserSettings>(&json).ok())
        .unwrap_or_default();

    let download_dir = if !settings.download_directory.trim().is_empty() {
        std::path::PathBuf::from(settings.download_directory.trim())
    } else {
        dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
    };

    let listening_port = settings.listening_port;

    // 4. Initialize production engine (RqbitTorrentEngine) with fallback to MockTorrentEngine
    let engine: SharedEngine = match engine::RqbitTorrentEngine::new(
        Arc::clone(&paths),
        Arc::clone(&db),
        download_dir,
        listening_port,
    ) {
        Ok(real_engine) => {
            SafeLogger::log_to_file(
                &paths.logs_dir,
                "INFO",
                &format!("Initialized librqbit production engine on port {listening_port}"),
            );
            Arc::new(real_engine)
        }
        Err(e) => {
            SafeLogger::log_to_file(
                &paths.logs_dir,
                "WARN",
                &format!("Failed to initialize real engine, using mock engine: {e}"),
            );
            Arc::new(engine::MockTorrentEngine::new())
        }
    };

    SafeLogger::log_to_file(
        &paths.logs_dir,
        "INFO",
        "Nebulae application initialized successfully",
    );

    let initial_payloads = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    for arg in std::env::args().skip(1) {
        let trimmed = arg.trim();
        if trimmed.starts_with("magnet:")
            || trimmed.starts_with("adc:")
            || trimmed.ends_with(".torrent")
            || trimmed.ends_with(".adc")
            || std::path::Path::new(trimmed).is_file()
        {
            if let Ok(mut lock) = initial_payloads.lock() {
                lock.push(trimmed.to_string());
            }
        }
    }
    let pending_payloads = commands::association::PendingOpenPayloads(initial_payloads.clone());

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(paths)
        .manage(db)
        .manage(engine)
        .manage(pending_payloads)
        .setup(|app| {
            use tauri::Manager;

            let show_i =
                tauri::menu::MenuItem::with_id(app, "show", "Open Nebulae", true, None::<&str>)?;
            let quit_i =
                tauri::menu::MenuItem::with_id(app, "quit", "Quit Nebulae", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show_i, &quit_i])?;

            let mut tray_builder = tauri::tray::TrayIconBuilder::with_id("nebulae-tray")
                .tooltip("Nebulae — Torrent Client by Asian DVD Club")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app
                            .get_webview_window("main")
                            .or_else(|| app.webview_windows().into_values().next())
                        {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    }
                    | tauri::tray::TrayIconEvent::DoubleClick { .. } => {
                        let app = tray.app_handle();
                        if let Some(window) = app
                            .get_webview_window("main")
                            .or_else(|| app.webview_windows().into_values().next())
                        {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            let _ = tray_builder.build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                use tauri::Manager;
                let should_close_to_tray = if let Some(db) =
                    window.try_state::<std::sync::Arc<crate::db::Database>>()
                {
                    if let Ok(Some(json)) = db.get_setting("user_settings_json") {
                        if let Ok(parsed) =
                            serde_json::from_str::<crate::commands::settings::UserSettings>(&json)
                        {
                            parsed.close_to_tray
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                } else {
                    true
                };

                if should_close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                use tauri::Emitter;
                for path in paths {
                    let path_str = path.to_string_lossy().to_string();
                    let _ = window.emit("nebulae://file-dropped", path_str);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            // Diagnostics
            commands::diagnostics::open_logs_folder,
            commands::diagnostics::get_diagnostic_summary,
            commands::diagnostics::clear_cache,
            commands::diagnostics::reset_settings,
            commands::diagnostics::export_logs,
            commands::diagnostics::open_external_link,
            commands::diagnostics::quit_app,
            // Settings
            commands::settings::load_user_settings,
            commands::settings::save_user_settings,
            // Torrent & Engine
            commands::torrent::list_torrents,
            commands::torrent::add_torrent_stub,
            commands::torrent::remove_torrent_stub,
            commands::torrent::pause_torrent_stub,
            commands::torrent::resume_torrent_stub,
            commands::torrent::force_recheck_torrent,
            commands::torrent::reannounce_torrent,
            commands::torrent::set_torrent_location,
            commands::torrent::set_torrent_category,
            commands::torrent::set_torrent_speed_limits,
            commands::torrent::toggle_torrent_sequential,
            commands::torrent::toggle_torrent_first_last,
            commands::torrent::toggle_torrent_super_seeding,
            commands::torrent::set_file_priority,
            commands::torrent::rename_torrent,
            commands::torrent::rename_torrent_file,
            commands::torrent::rename_torrent_folder,
            commands::torrent::inspect_torrent_file,
            commands::torrent::add_torrent_bytes,
            commands::torrent::create_torrent_file,
            commands::torrent::export_torrent_file,
            commands::torrent::get_engine_status,
            commands::torrent::trigger_kill_switch_command,
            commands::torrent::reset_kill_switch_command,
            commands::torrent::select_torrent_file,
            commands::torrent::select_source_folder,
            commands::torrent::select_source_file,
            commands::torrent::select_download_directory,
            commands::torrent::get_system_path_presets,
            // File & Protocol Associations
            commands::association::get_pending_open_payloads,
            commands::association::check_associations_status,
            commands::association::register_protocol_associations,
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
        .build(tauri::generate_context!())
        .expect("Error while building Nebulae application");

    let _event_payloads = initial_payloads.clone();
    app.run(move |_app_handle, _event| {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if let tauri::RunEvent::Opened { urls } = _event {
            use tauri::Emitter;
            for url in urls {
                let payload = if url.scheme() == "file" {
                    url.to_file_path()
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| url.to_string())
                } else {
                    url.to_string()
                };
                if let Ok(mut lock) = _event_payloads.lock() {
                    lock.push(payload.clone());
                }
                let _ = _app_handle.emit("nebulae://open-uri", payload);
            }
        }
    });
}
