#![recursion_limit = "256"]
pub mod app_menu;
pub mod commands;
pub mod db;
pub mod engine;
pub mod models;
pub mod player;
pub mod qbit_import;
pub mod shutdown;
pub mod single_instance_gate;
pub mod speed_format;
pub mod status_bar;
pub mod tailscale;
pub mod taskbar_speed;
pub mod taskbar_windows;
pub mod thumbs;
#[cfg(target_os = "windows")]
pub mod tray_speed;
pub mod updater;
pub mod utils;
pub mod webview_recovery;

use commands::diagnostics::EngineInitState;
use db::Database;
use engine::SharedEngine;
use std::sync::Arc;
use tailscale::TsState;
use utils::logger::SafeLogger;
use utils::paths::AppPaths;

pub static TS_STATE: std::sync::OnceLock<std::sync::Arc<TsState>> = std::sync::OnceLock::new();

/// Handles the arguments of a second launch: focus the running window and
/// queue any file/link payload. Shared by the plugin callback and the
/// pre-startup hand-off consumer.
fn handle_second_instance(app: &tauri::AppHandle, argv: &[String]) {
    use tauri::{Emitter, Manager};
    for arg in argv {
        if arg == "--quit" || arg == "-q" || arg == "--exit" {
            crate::commands::diagnostics::quit_app(app.clone());
            return;
        }
    }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        #[cfg(target_os = "windows")]
        if let Ok(hwnd) = win.hwnd() {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
            }
        }
    }
    for arg in argv.iter().skip(1) {
        let lower = arg.to_ascii_lowercase();
        if lower.ends_with(".torrent")
            || lower.ends_with(".adc")
            || lower.starts_with("magnet:")
            || lower.starts_with("adc:")
            || (!arg.starts_with('-') && std::path::Path::new(arg).exists())
        {
            if let Some(pending) = app.try_state::<commands::association::PendingOpenPayloads>() {
                if let Ok(mut queue) = pending.0.lock() {
                    queue.push(arg.clone());
                }
            }
            let _ = app.emit("nebulae://open-uri", arg.clone());
        }
    }
}

pub fn run() {
    // Before any side effect (engine, Tailscale, WebView cleanup, DB): a second
    // launch hands its arguments to the running instance and exits untouched.
    if let Ok(early_paths) = AppPaths::init() {
        if single_instance_gate::acquire(&early_paths.internal_dir)
            == single_instance_gate::Gate::HandedOff
        {
            return;
        }
    }

    taskbar_windows::init_taskbar_app_id();

    // Portable BD-J: point libbluray (inside the bundled VLC plugins) at the
    // JRE shipped in resources/jre BEFORE any player library is loaded.
    if let Ok(exe) = std::env::current_exe() {
        let mut jre_candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Some(dir) = exe.parent() {
            jre_candidates.push(dir.join("resources").join("jre"));
            jre_candidates.push(dir.join("jre"));
            // Linux: /usr/bin/nebulae -> /usr/lib/Nebulae/resources/jre
            jre_candidates.push(
                dir.parent()
                    .unwrap_or(dir)
                    .join("lib/Nebulae/resources/jre"),
            );
        }
        if let Some(parent) = exe.parent().and_then(|d| d.parent()) {
            jre_candidates.push(parent.join("Resources/resources/jre"));
            jre_candidates.push(parent.join("lib/Nebulae/resources/jre"));
            jre_candidates.push(parent.join("share/nebulae/resources/jre"));
            jre_candidates.push(parent.join("lib/nebulae/resources/jre"));
        }
        if let Ok(appdir) = std::env::var("APPDIR") {
            jre_candidates
                .push(std::path::PathBuf::from(&appdir).join("usr/lib/Nebulae/resources/jre"));
            jre_candidates
                .push(std::path::PathBuf::from(&appdir).join("usr/share/nebulae/resources/jre"));
        }
        jre_candidates.push(std::path::PathBuf::from("/usr/lib/Nebulae/resources/jre"));
        jre_candidates.push(std::path::PathBuf::from("src-tauri/resources/jre"));
        jre_candidates.push(std::path::PathBuf::from("resources/jre"));
        if let Some(jre) = jre_candidates
            .into_iter()
            .find(|c| c.join("bin").exists() || c.join("lib").exists())
        {
            std::env::set_var("JAVA_HOME", &jre);
            let bin = jre.join("bin");
            if bin.exists() {
                if let Ok(curr_path) = std::env::var("PATH") {
                    let mut paths = std::env::split_paths(&curr_path).collect::<Vec<_>>();
                    if !paths.iter().any(|p| p == &bin) {
                        paths.insert(0, bin);
                        if let Ok(new_path) = std::env::join_paths(paths) {
                            std::env::set_var("PATH", new_path);
                        }
                    }
                }
            }
        }
    }

    // Stale WebView2 cleanup BEFORE any window exists: a previously
    // hard-killed app can leave webview runtimes holding the EBWebView
    // profile lock, which fails WebView2 init on the next launch. The
    // pass is ownership-scoped (our marker + dead parent only) and never
    // touches the profile on disk.
    {
        let stale = webview_recovery::recover_stale_webviews();
        if !stale.is_empty() {
            SafeLogger::log_to_file(
                &AppPaths::init().map(|p| p.logs_dir).unwrap_or_default(),
                "INFO",
                &format!("Recovered stale WebView2 runtimes: {:?}", stale),
            );
        }
    }

    // Embedded Tailscale (ADC-network) state container + app-owned
    // lifecycle: start the bundled daemon at launch (readiness-waited on a
    // background thread; missing binaries are logged loudly, never silent).
    let _ = TS_STATE.set(std::sync::Arc::new(TsState::default()));
    std::thread::Builder::new()
        .name("tailscale-startup".into())
        .spawn(|| {
            if let Err(e) = tailscale::ensure_daemon() {
                tracing::warn!("[tailscale] not started at launch: {e}");
            }
        })
        .ok();

    // 0. Initialize isolated internal directory layout: Nebulae/internal/
    let paths = match AppPaths::init() {
        Ok(p) => Arc::new(p),
        Err(e) => {
            eprintln!("Fatal error initializing application paths: {e}");
            return;
        }
    };

    // 0b. Initialize privacy-safe tracing subscriber (redacting passkeys and
    // tracker URLs). The subscriber writes to stderr AND the session log —
    // the file leg is what makes player-stage diagnostics and webview
    // breadcrumbs visible in the log we ship (they used to vanish).
    SafeLogger::init_tracing_with_file(Some(&paths.logs_dir));
    tracing::info!(
        build_id = env!("NEBULAE_BUILD_ID"),
        "Embedded source build provenance"
    );

    // 2. Initialize internal SQLite persistence
    let db = match Database::new(&paths.db_file) {
        Ok(d) => Arc::new(d),
        Err(e) => {
            eprintln!("Fatal error initializing SQLite database: {e}");
            return;
        }
    };

    // 3. Resolve user settings for engine session (listening_port & download_directory)
    let settings = {
        let mut s = db
            .get_setting("user_settings_json")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<commands::settings::UserSettings>(&json).ok())
            .unwrap_or_default();
        // Normalize BEFORE deriving engine inputs: the stored default port
        // must move off the stock Transmission port (51413) so a standalone
        // Transmission install can't collide with the bundled daemon.
        if s.engine.trim() == "librqbit" {
            s.engine = "transmission".to_string();
        }
        if s.listening_port == 51413 {
            s.listening_port = 41413;
        }
        s
    };

    let download_dir = if let Ok(resolved) =
        crate::utils::paths::resolve_local_dir(&settings.download_directory)
    {
        resolved
    } else {
        dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
    };

    let listening_port = settings.listening_port;

    // Stamp the running version into the settings. When a new install takes
    // over from an older instance, this re-persists the settings with the
    // new version and any newly introduced defaults applied.
    let settings = {
        let mut s = settings;
        let current = env!("CARGO_PKG_VERSION");
        if s.app_version.as_deref() != Some(current) {
            s.app_version = Some(current.to_string());
            if let Ok(json) = serde_json::to_string(&s) {
                if db.set_setting("user_settings_json", &json).is_ok() {
                    SafeLogger::log_to_file(
                        &paths.logs_dir,
                        "INFO",
                        &format!("Settings updated for Nebulae v{current}"),
                    );
                }
            }
        }
        if s.engine.trim() == "librqbit" {
            s.engine = "transmission".to_string();
        }
        // Migrate the stock Transmission default port to the Nebulae-specific
        // default so a standalone Transmission install on the same machine
        // (stock port 51413) never fights the bundled daemon for the bind.
        if s.listening_port == 51413 {
            s.listening_port = 41413;
        }
        s
    };

    let mut real_engine_opt: Option<SharedEngine> = None;
    let mut init_error: Option<String> = None;

    // The local engine always runs: the Manager tab is the local installation.
    // A configured Nebulae Server is reached through the Server tab (ScopedEngine).
    let local_engine: SharedEngine = {
        // Initialize production Transmission engine.
        // Tier 1: Single bounded retry after 500ms backoff if initialization fails.
        // Transmission engine (default)
        let res_dir = crate::utils::paths::resolve_bundled_resource_dir();
        match engine::TransmissionTorrentEngine::new(
            Arc::clone(&paths),
            Arc::clone(&db),
            download_dir.clone(),
            &settings,
            Some(&res_dir),
        ) {
            Ok(real_engine) => {
                SafeLogger::log_to_file(
                        &paths.logs_dir,
                        "INFO",
                        &format!("Initialized transmission-daemon production engine on port {listening_port}"),
                    );
                real_engine_opt = Some(Arc::new(real_engine));
            }
            Err(e) => {
                SafeLogger::log_to_file(
                        &paths.logs_dir,
                        "WARN",
                        &format!(
                            "First transmission-daemon initialization attempt failed: {e}. Retrying in 500ms..."
                        ),
                    );
                std::thread::sleep(std::time::Duration::from_millis(500));
                match engine::TransmissionTorrentEngine::new(
                    Arc::clone(&paths),
                    Arc::clone(&db),
                    download_dir,
                    &settings,
                    Some(&res_dir),
                ) {
                    Ok(real_engine) => {
                        SafeLogger::log_to_file(
                                &paths.logs_dir,
                                "INFO",
                                &format!("Initialized transmission-daemon production engine on retry on port {listening_port}"),
                            );
                        real_engine_opt = Some(Arc::new(real_engine));
                    }
                    Err(retry_err) => {
                        SafeLogger::log_to_file(
                                &paths.logs_dir,
                                "ERROR",
                                &format!("Failed to initialize transmission-daemon engine after retry: {retry_err}"),
                            );
                        init_error = Some(retry_err);
                    }
                }
            }
        }

        if let Some(real_engine) = real_engine_opt {
            // Re-apply saved peering settings (peer limits) to the newly-started
            // Transmission session. Settings are persisted in DB; this restores
            // them across app restarts without requiring the user to re-save.
            if let Ok(Some(json)) = db.get_setting("peering_settings_json") {
                if let Ok(peering) =
                    serde_json::from_str::<crate::models::peering::PeeringSettings>(&json)
                {
                    if let Err(e) = real_engine.apply_peering_settings(&peering) {
                        SafeLogger::log_to_file(
                            &paths.logs_dir,
                            "WARN",
                            &format!("Could not apply saved peering settings at startup: {e}"),
                        );
                    }
                }
            }
            real_engine
        } else {
            let err_msg = init_error
                .clone()
                .unwrap_or_else(|| "Engine initialization failed".to_string());
            SafeLogger::log_to_file(
                &paths.logs_dir,
                "ERROR",
                &format!("Engine initialization failed. Entering recovery state: {err_msg}"),
            );
            Arc::new(engine::FailedTorrentEngine::new(err_msg))
        }
    };

    let engine_init_state = EngineInitState(Arc::new(std::sync::Mutex::new(init_error)));

    SafeLogger::log_to_file(
        &paths.logs_dir,
        "INFO",
        "Nebulae application initialized successfully",
    );

    let initial_payloads = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    for arg in std::env::args().skip(1) {
        let trimmed = arg.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("magnet:")
            || lower.starts_with("adc:")
            || lower.ends_with(".torrent")
            || lower.ends_with(".adc")
            || (!trimmed.starts_with('-') && std::path::Path::new(trimmed).exists())
        {
            if let Ok(mut lock) = initial_payloads.lock() {
                lock.push(trimmed.to_string());
            }
        }
    }
    let pending_payloads = commands::association::PendingOpenPayloads(initial_payloads.clone());
    // The GUI hides options the active engine cannot fulfill (e.g. features
    // rtorrent does not have) based on this session-wide mode.
    let engine_mode = commands::settings::EngineMode("local".to_string());

    let scoped_engine = Arc::new(engine::ScopedEngine::new(Arc::clone(&local_engine)));
    let engine: SharedEngine = scoped_engine.clone();
    let local_engine_state = engine::LocalEngine(Arc::clone(&local_engine));

    let engine_for_seeding = Arc::clone(&local_engine);
    let db_for_seeding = Arc::clone(&db);
    let engine_for_killswitch = Arc::clone(&local_engine);
    let db_for_killswitch = Arc::clone(&db);
    let engine_for_telemetry = Arc::clone(&local_engine);
    let engine_for_peer_history = Arc::clone(&local_engine);
    let db_for_peer_history = Arc::clone(&db);
    let engine_for_port_watch = Arc::clone(&local_engine);
    let db_for_port_watch = Arc::clone(&db);
    let engine_for_watcher = Arc::clone(&local_engine);
    let db_for_watcher = Arc::clone(&db);
    let engine_for_rss = Arc::clone(&local_engine);
    let db_for_rss = Arc::clone(&db);
    let db_for_stats_poller = Arc::clone(&db);
    let db_for_inbox_poller = Arc::clone(&db);
    let internal_dir_for_handoff = paths.internal_dir.clone();
    let engine_for_delivery_poller = Arc::clone(&local_engine);
    let db_for_delivery_poller = Arc::clone(&db);

    let app = tauri::Builder::default()
        // Single instance (owner): double-clicking another .torrent must
        // focus the running app and hand it the file, not spawn a second
        // instance. Registered FIRST per the plugin's requirement.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_second_instance(app, &argv);
        }))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .manage(paths)
        .manage(db.clone())
        .manage(engine)
        .manage(local_engine_state)
        .manage(scoped_engine.clone())
        .manage(engine_init_state)
        .manage(engine_mode)
        .manage(pending_payloads)
        .manage(updater::StagedUpdateState::default())
        .manage(updater::PendingUpdateState::default())
        .manage(player::PlayerState::default())
        .manage(commands::qbit_import::QbitImportState::default())
        .setup(move |app| {
            use tauri::Manager;
            // Rounded app corners (owner, Movist-style): apply once at
            // startup — the old call only ran when the player attached.
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                let h: tauri::AppHandle = app.handle().clone();
                let radius_handle = h.clone();
                let _ = tauri::AppHandle::run_on_main_thread(&h, move || unsafe {
                    player::apply_window_corner_radius(&radius_handle, 10.0);
                });
            }
            commands::media::maybe_autoplay(app.handle());
            commands::torrent::register_app_handle(app.handle());

            // Start-minimized (owner setting): hide the window when the app
            // was launched by the OS autostart entry with --minimized.
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            }

            // Re-apply the autostart registration at boot: keeps the
            // LaunchAgent / Run key / desktop entry pointing at THIS
            // executable after the app moves or updates.
            {
                let db_handle = db.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(Some(json)) = db_handle.get_setting("user_settings_json") {
                        if let Ok(settings) =
                            serde_json::from_str::<crate::commands::settings::UserSettings>(&json)
                        {
                            if let Err(e) = crate::commands::settings::apply_autostart(
                                settings.start_on_boot,
                                settings.start_minimized,
                            ) {
                                tracing::warn!("[autostart] reapply failed: {e}");
                            }
                        }
                    }
                });
            }

            #[cfg(target_os = "windows")]
            crate::taskbar_speed::init_taskbar_speed(app.handle());

            // Diagnostics for the reported white-screen-on-launch: log the
            // URL the main window was actually pointed at.
            if let Some(w) = app.get_webview_window("main") {
                let url = w
                    .url()
                    .map(|u| u.to_string())
                    .unwrap_or_else(|e| format!("url error: {e}"));
                eprintln!("[diag] main window url = {url}");
                println!("[diag] main window url = {url}");
            }

            // Main window is created in code so a TEST build can opt into a
            // WebView2/CDP debugging port via NEBULAE_TEST_CDP=1 (never set
            // in production; keeps packaged behavior identical).
            {
                let mut builder = tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::default(),
                )
                .title("Nebulae — Torrent Client by Asian DVD Club")
                .inner_size(1280., 840.)
                .min_inner_size(980., 640.)
                .decorations(false)
                .transparent(true)
                .center();
                if let Some(icon) = app.default_window_icon() {
                    builder = builder.icon(icon.clone())?;
                }
                if std::env::var("NEBULAE_TEST_CDP").as_deref() == Ok("1") {
                    builder = builder.additional_browser_args(
                        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port=9222 --remote-allow-origins=*",
                    );
                }
                builder.build()?;
            }

            {
                let handle = app.handle().clone();
                single_instance_gate::spawn_consumer(internal_dir_for_handoff, move |argv| {
                    handle_second_instance(&handle, &argv)
                });
            }
            engine::seeding::spawn_seeding_rules_loop(engine_for_seeding, db_for_seeding);
            engine::killswitch::spawn_kill_switch_watcher(engine_for_killswitch, db_for_killswitch);
            engine::telemetry::spawn_telemetry_loop(
                app.handle().clone(),
                engine_for_telemetry,
                scoped_engine.clone(),
            );
            engine::telemetry::spawn_peer_history_loop(
                engine_for_peer_history,
                db_for_peer_history,
            );
            // D23: log port reachability transitions over the session so the
            // "listening port closes mid-session" report can be attributed.
            engine::telemetry::spawn_port_watch_loop(engine_for_port_watch, db_for_port_watch);
            engine::watcher::spawn_watched_folder_loop(engine_for_watcher, db_for_watcher);
            engine::rss::spawn_rss_loop(engine_for_rss, db_for_rss);
            commands::tracker::spawn_stats_poller(db_for_stats_poller);
            commands::inbox::spawn_inbox_poller(app.handle().clone(), db_for_inbox_poller);
            commands::adc_device::spawn_delivery_poller(
                app.handle().clone(),
                engine_for_delivery_poller,
                db_for_delivery_poller,
            );

            #[cfg(desktop)]
            {
                let handle = app.handle().clone();
                let db_handle = db.clone();
                updater::spawn_silent_background_updater(&handle, db_handle);
                taskbar_windows::update_taskbar_jump_list();
            }

            let show_i =
                tauri::menu::MenuItem::with_id(app, "show", "Open Nebulae", true, None::<&str>)?;
            let add_torrent_i = tauri::menu::MenuItem::with_id(
                app,
                "add-torrent",
                "Add Torrent",
                true,
                None::<&str>,
            )?;
            let create_torrent_i = tauri::menu::MenuItem::with_id(
                app,
                "create-torrent",
                "Create New Torrent",
                true,
                None::<&str>,
            )?;
            let pause_all_i = tauri::menu::MenuItem::with_id(
                app,
                "pause-all",
                "Pause All Torrents",
                true,
                None::<&str>,
            )?;
            let resume_all_i = tauri::menu::MenuItem::with_id(
                app,
                "resume-all",
                "Resume All Torrents",
                true,
                None::<&str>,
            )?;
            let quit_i =
                tauri::menu::MenuItem::with_id(app, "quit", "Quit application", true, None::<&str>)?;
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let menu = tauri::menu::Menu::with_items(
                app,
                &[
                    &show_i,
                    &add_torrent_i,
                    &create_torrent_i,
                    &pause_all_i,
                    &resume_all_i,
                    &separator,
                    &quit_i,
                ],
            )?;

            let mut tray_builder = tauri::tray::TrayIconBuilder::with_id("nebulae-tray")
                .tooltip("Nebulae — Client by Asian DVD Club")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        crate::shutdown::initiate_shutdown(app);
                    }
                    "show" => {
                        if let Some(window) = app
                            .get_webview_window("main")
                            .or_else(|| app.webview_windows().into_values().next())
                        {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                            #[cfg(target_os = "windows")]
                            if let Ok(hwnd) = window.hwnd() {
                                unsafe {
                                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                                }
                            }
                        }
                    }
                    "add-torrent" | "create-torrent" => {
                        use tauri::Emitter;
                        if let Some(window) = app
                            .get_webview_window("main")
                            .or_else(|| app.webview_windows().into_values().next())
                        {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                            #[cfg(target_os = "windows")]
                            if let Ok(hwnd) = window.hwnd() {
                                unsafe {
                                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                                }
                            }
                            let action = if event.id.as_ref() == "add-torrent" {
                                "add-torrent"
                            } else {
                                "create-torrent"
                            };
                            let _ = window.emit("nebulae://tray-action", action);
                        }
                    }
                    "pause-all" => {
                        let engine = app.state::<engine::LocalEngine>();
                        if let Err(e) = engine.pause_all() {
                            tracing::warn!("Tray pause-all failed: {e}");
                        }
                    }
                    "resume-all" => {
                        let engine = app.state::<engine::LocalEngine>();
                        if let Err(e) = engine.resume_all() {
                            tracing::warn!("Tray resume-all failed: {e}");
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } => {
                        // Left-click toggles the main window: hide if visible,
                        // show/restore if hidden or minimized. Right-click
                        // (show_menu_on_left_click = false) opens the context menu.
                        let app = tray.app_handle();
                        if let Some(window) = app
                            .get_webview_window("main")
                            .or_else(|| app.webview_windows().into_values().next())
                        {
                            let is_minimized = window.is_minimized().unwrap_or(false);
                            let is_visible = window.is_visible().unwrap_or(false);
                            if is_visible && !is_minimized {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                                #[cfg(target_os = "windows")]
                                if let Ok(hwnd) = window.hwnd() {
                                    unsafe {
                                        let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                                    }
                                }
                            }
                        }
                    }
                    tauri::tray::TrayIconEvent::DoubleClick { .. } => {
                        let app = tray.app_handle();
                        if let Some(window) = app
                            .get_webview_window("main")
                            .or_else(|| app.webview_windows().into_values().next())
                        {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                            #[cfg(target_os = "windows")]
                            if let Ok(hwnd) = window.hwnd() {
                                unsafe {
                                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                                }
                            }
                        }
                    }
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            let _ = tray_builder.build(app)?;

            // Initialize status bar and set tray ID for macOS
            #[cfg(target_os = "macos")]
            {
                use crate::status_bar;
                status_bar::init_status_bar(app.handle());
                // The tray ID is "nebulae-tray" as defined in the TrayIconBuilder
                status_bar::set_tray_id("nebulae-tray".to_string());
                if let Some(db) = app.try_state::<Arc<Database>>() {
                    let enabled = commands::settings::load_user_settings_from_db(&db)
                        .show_taskbar_speed_overlay;
                    status_bar::set_speed_display_enabled(app.handle(), enabled);
                }
                crate::shutdown::install_macos_terminate_hook(app.handle().clone());
            }

            #[cfg(desktop)]
            {
                let native_menu = app_menu::build_app_menu(app.handle())?;
                let _ = app.set_menu(native_menu);
                // The window draws its own title bar. On Windows the native
                // menu bar would otherwise sit inside the window's frame,
                // over the video surface that is sized to the window rect.
                #[cfg(target_os = "windows")]
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide_menu();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                use tauri::Manager;
                // If explicit application shutdown has been initiated, allow windows to close cleanly
                if crate::shutdown::is_shutting_down() {
                    return;
                }
                // Only intercept close on the main window; individual player or dialog windows close normally
                if window.label() == "main" {
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

                    let app = window.app_handle();
                    if let Some(player_state) = app.try_state::<crate::player::PlayerState>() {
                        crate::commands::media::stop_session(app, &player_state);
                    }

                    if should_close_to_tray {
                        api.prevent_close();
                        // Windows keeps the taskbar button (minimized) rather than hiding to the tray.
                        #[cfg(target_os = "windows")]
                        let _ = window.minimize();
                        #[cfg(not(target_os = "windows"))]
                        let _ = window.hide();
                    } else {
                        crate::shutdown::initiate_shutdown(app);
                    }
                }
            }
            tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                use tauri::Emitter;
                let dropped_paths: Vec<String> = paths
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect();
                if !dropped_paths.is_empty() {
                    let _ = window.emit("nebulae://paths-dropped", dropped_paths);
                }
            }
            _ => {}
        })
        .on_menu_event(app_menu::handle_menu_event)
        .invoke_handler({
            let handler: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static> =
                Box::new(tauri::generate_handler![
            // Diagnostics & Recovery
            commands::diagnostics::get_engine_init_error,
            commands::diagnostics::log_frontend,
            commands::diagnostics::open_logs_folder,
            commands::diagnostics::get_diagnostic_summary,
            commands::diagnostics::clear_cache,
            commands::diagnostics::reset_settings,
            commands::diagnostics::export_logs,
            commands::diagnostics::open_external_link,
            commands::diagnostics::quit_app,
            commands::diagnostics::generate_bug_report_bundle,
            commands::diagnostics::bug_report_endpoint,
            commands::diagnostics::save_bug_report_draft,
            commands::diagnostics::get_bug_report_draft,
            commands::diagnostics::clear_bug_report_draft,
            // Settings
            commands::settings::load_user_settings,
            commands::settings::save_user_settings,
            commands::settings::get_engine_mode,
            commands::torrent::set_engine_scope,
            commands::settings::get_app_platform,
            commands::settings::test_rtorrent_server_connection,
            commands::settings::get_engine_identity_info,
            // Remote server proxy (token stays in Rust, never sent to WebView)
            commands::remote_server::remote_server_status,
            commands::remote_server::remote_list_torrents,
            commands::remote_server::remote_pause_torrent,
            commands::remote_server::remote_resume_torrent,
            commands::remote_server::remote_pause_all,
            commands::remote_server::remote_resume_all,
            commands::remote_server::remote_remove_torrent,
            commands::remote_server::remote_set_kill_switch,
            commands::remote_server::remote_reannounce_torrent,
            commands::remote_server::remote_recheck_torrent,
            commands::remote_server::remote_set_torrent_limits,
            // Torrent & Engine
            commands::torrent::list_torrents,
            commands::torrent::get_torrent_details,
            commands::torrent::open_torrent_folder,
            commands::torrent::add_torrent,
            commands::torrent::remove_torrent,
            commands::torrent::get_active_torrent_operations,
            commands::torrent::dismiss_torrent_operation,
            commands::torrent::pause_torrent,
            commands::torrent::resume_torrent,
            commands::torrent::pause_all_torrents,
            commands::torrent::resume_all_torrents,
            commands::torrent::match_torrent_content,
            commands::torrent::apply_torrent_renames,
            commands::torrent::force_recheck_torrent,
            commands::torrent::reannounce_torrent,
            commands::torrent::set_torrent_location,
            commands::torrent::set_torrent_category,
            commands::torrent::set_torrent_speed_limits,
            commands::torrent::toggle_torrent_sequential,
            commands::torrent::toggle_torrent_first_last,
            commands::torrent::toggle_torrent_super_seeding,
            commands::torrent::set_torrent_racing_mode,
            commands::torrent::set_file_priority,
            commands::torrent::set_files_priority,
            commands::torrent::rename_torrent,
            commands::torrent::rename_torrent_file,
            commands::torrent::rename_torrent_folder,
            commands::torrent::inspect_torrent_file,
            commands::torrent::add_torrent_bytes,
            commands::torrent::add_torrent_staged,
            commands::torrent::create_torrent_file,
            commands::torrent::export_torrent_file,
            commands::torrent::save_torrent_file_as,
            commands::tailscale::tailscale_status,
            commands::tailscale::tailscale_enroll,
            commands::tailscale::tailscale_disconnect,
            commands::tailscale::adc_private_probe,
            commands::tailscale::tailscale_adc_fetch,
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
            // ADC Direct One-Click Downloads
            commands::adc_download::resolve_adc_torrent,
            commands::adc_download::get_adc_download_warning_suppressed,
            commands::adc_download::set_adc_download_warning_suppressed,
            commands::adc_download::get_adc_client_targets,
            commands::adc_download::set_adc_client_targets,
            // ADC Device Registration & Delivery
            commands::adc_device::get_adc_device_info,
            commands::adc_device::register_adc_device,
            commands::adc_device::redeem_adc_delivery,
            commands::adc_device::set_adc_device_priority,
            // Smart Peering
            commands::peering::get_peering_settings,
            commands::peering::save_peering_settings,
            commands::peering::get_peer_telemetry,
            commands::peering::get_peer_history,
            commands::peering::clear_peer_history,
            commands::peering::get_peer_block_capability,
            commands::peering::list_blocked_peer_ips,
            commands::peering::block_peer_ip,
            commands::peering::unblock_peer_ip,
            // Connectivity Doctor
            commands::connectivity::run_connectivity_doctor_test,
            commands::connectivity::get_engine_ipv6_diagnostic,
            commands::connectivity::get_port_negotiation_result,
            commands::connectivity::get_connectivity_certificate,
            commands::connectivity::get_tailscale_status,
            // VPN
            commands::vpn::get_vpn_status,
            commands::vpn::set_vpn_mode,
            commands::vpn::toggle_kill_switch,
            commands::vpn::import_wireguard_profile,
            commands::vpn::import_openvpn_profile,
            commands::vpn::list_vpn_profiles,
            commands::vpn::delete_vpn_profile,
            commands::vpn::get_network_interfaces,
            commands::vpn::get_egress_source_ip,
            // Media Station & Optical Disc Cinema — ONE engine: the user's
            // VLC, loaded as a library and embedded in the app window.
            commands::media::player_open,
            commands::media::player_close,
            commands::media::player_toggle_pause,
            commands::media::player_toggle_fullscreen,
            commands::media::player_window_minimize,
            commands::media::window_minimize_cmd,
            commands::media::window_maximize_toggle_cmd,
            commands::media::window_close_cmd,
            commands::media::window_is_maximized_cmd,
            commands::media::player_seek,
            commands::media::player_set_volume,
            commands::media::player_poll,
            commands::media::player_set_title,
            commands::media::player_set_chapter,
            commands::media::player_step_title,
            commands::media::player_chapter_marks,
            commands::media::player_disc_chapters,
            commands::media::player_disc_display_title,
            commands::media::player_set_always_on_top,
            commands::media::player_always_on_top,
            commands::media::add_external_media,
            commands::media::list_external_media,
            commands::media::remove_external_media,
            commands::media::set_external_media_title,
            commands::media::relink_external_media,
            commands::media::relink_torrent_media,
            commands::media::set_custom_poster,
            commands::media::check_media_path,
            commands::media::set_media_title_hidden,
            commands::media::list_hidden_media_titles,
            commands::media::restore_all_hidden_titles,
            commands::torrent::select_iso_file,
            commands::torrent::select_disc_source,
            commands::torrent::select_disc_folder,
            commands::torrent::select_disc_file,
            commands::torrent::select_image_file,
            commands::media::player_list_tracks,
            commands::media::player_track_languages,
            commands::media::window_begin_drag,
            commands::media::player_engine_playback,
            commands::media::player_set_track,
            commands::media::player_menu_navigate,
            commands::media::player_menu_mouse_move,
            commands::media::player_menu_mouse_click,
            commands::media::player_return_to_menu,
            commands::media::player_play_main_feature,
            commands::media::player_sync_viewport,
            commands::media::player_device_discovery_start,
            commands::media::player_device_discovery_stop,
            commands::media::player_device_list,
            commands::media::player_device_preflight,
            commands::media::player_device_start,
            commands::media::player_device_return_to_local,
            commands::media::player_device_stop,
            commands::media::player_device_status,
            commands::media::player_airplay_mount,
            commands::media::player_airplay_unmount,
            commands::media::get_disc_player_status,
            commands::media::detect_disc_format,
            commands::media::classify_dropped_media_path,
            commands::media::detect_disc_set,
            commands::media::scan_disc_artwork,
            commands::media::parse_media_title,
            commands::media::get_volume_disk_space,
            commands::media::clean_cache_files,
            commands::media::organize_completed_media,
            commands::media::get_media_thumbnail,
            commands::posters::fetch_adc_poster_map,
            commands::posters::download_adc_poster,
            commands::media::fetch_tmdb_release,
            commands::media::fetch_adc_page,
            commands::media::tmdb_configured,
            // AsianDVDClub Tracker Integration
            commands::tracker::get_adc_tracker_profile,
            commands::tracker::unlock_adc_credential,
            commands::tracker::link_adc_account,
            commands::tracker::unlink_adc_account,
            commands::tracker::import_adc_passkey,
            commands::tracker::get_seed_time_records,
            commands::tracker::calculate_ratio_projection,
            commands::tracker::sync_adc_stats,
            commands::tracker::adc_swarm_members,
            // ADC Inbox & Notifications
            commands::inbox::get_adc_inbox_summary,
            commands::inbox::sync_adc_inbox,
            commands::inbox::mark_adc_inbox_read,
            commands::inbox::clear_adc_inbox,
            // Feature 6: editable listing names
            commands::library::set_user_display_title,
            commands::library::reset_user_display_title,
            // Feature 3: poster ordering
            commands::library::set_media_order,
            commands::library::get_downloaded_media_order,
            commands::library::get_external_media_order,
            commands::library::set_external_media_order,
            // Feature 4: posters-per-row, poster-size & poster-spacing
            commands::library::get_posters_per_row,
            commands::library::set_posters_per_row,
            commands::library::get_poster_size,
            commands::library::set_poster_size,
            commands::library::get_poster_spacing,
            commands::library::set_poster_spacing,
            // Feature 8/9: metadata provider attribution
            commands::library::set_media_provider,
            commands::library::set_torrent_poster,
            commands::library::get_torrent_poster,
            commands::library::save_media_metadata_and_poster,
            // Feature 8/9: multi-result metadata search
            commands::media::search_tmdb_titles,
            commands::media::search_mal_titles,
            commands::media::fetch_mal_anime,
            // qBittorrent Ingestion & Migration
            commands::qbit_import::qbit_auto_locate,
            commands::qbit_import::qbit_validate_dir,
            commands::qbit_import::qbit_check_running,
            commands::qbit_import::qbit_scan_torrents,
            commands::qbit_import::qbit_execute_import,
            commands::qbit_import::qbit_restore_backup,
            // Software Updater
            updater::check_for_updates,
            updater::get_available_update,
            updater::download_and_install_update,
            updater::get_update_staged,
            ]);
            move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
                let cmd = invoke.message.command().to_string();
                let t0 = std::time::Instant::now();
                let res = handler(invoke);
                let elapsed_ms = t0.elapsed().as_millis();
                if elapsed_ms > 200 {
                    tracing::warn!("[perf] Tauri command '{}' took {}ms (>200ms)", cmd, elapsed_ms);
                }
                res
            }
        })
        .build(tauri::generate_context!())
        .expect("Error while building Nebulae application");

    let _event_payloads = initial_payloads.clone();
    app.run(move |_app_handle, _event| match &_event {
        tauri::RunEvent::ExitRequested { .. } => {
            crate::shutdown::initiate_shutdown(_app_handle);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        tauri::RunEvent::Opened { urls } => {
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
        // macOS: clicking the Dock icon after close-to-tray must bring the
        // window back; without this the app looked gone after pressing X.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        tauri::RunEvent::Reopen { .. } => {
            use tauri::Manager;
            if let Some(window) = _app_handle
                .get_webview_window("main")
                .or_else(|| _app_handle.webview_windows().into_values().next())
            {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        _ => {}
    });
    std::process::exit(0);
}
