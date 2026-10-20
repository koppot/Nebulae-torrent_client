use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Builds the native qBittorrent-style application menu:
/// - Nebulae (macOS App Menu): About, Settings, Services, Hide, Quit
/// - File: Add Torrent File, Add Torrent Link, Create Torrent, Close Window
/// - Edit: Undo, Redo, Cut, Copy, Paste, Select All, Resume, Pause, Delete
/// - View: Media Center, Torrent Manager, Compact Mode, Fullscreen, Connectivity Doctor, VPN Manager, Tracker Dashboard
/// - Window: Minimize, Zoom, Bring All to Front
/// - Help: Documentation & Shortcuts, Discord Support (#nebulae-support), About
pub fn build_app_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, tauri::Error> {
    // 1. App Submenu (macOS native placement for About, Settings, Services, Hide, Quit)
    let app_submenu = {
        let about_i = MenuItem::with_id(app, "menu-about", "About Nebulae", true, None::<&str>)?;
        let sep1 = PredefinedMenuItem::separator(app)?;
        let settings_i =
            MenuItem::with_id(app, "menu-settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
        let sep2 = PredefinedMenuItem::separator(app)?;
        let services_i = PredefinedMenuItem::services(app, None)?;
        let sep3 = PredefinedMenuItem::separator(app)?;
        let hide_i = PredefinedMenuItem::hide(app, None)?;
        let hide_others_i = PredefinedMenuItem::hide_others(app, None)?;
        let show_all_i = PredefinedMenuItem::show_all(app, None)?;
        let sep4 = PredefinedMenuItem::separator(app)?;
        let quit_i = PredefinedMenuItem::quit(app, None)?;

        Submenu::with_items(
            app,
            "Nebulae",
            true,
            &[
                &about_i,
                &sep1,
                &settings_i,
                &sep2,
                &services_i,
                &sep3,
                &hide_i,
                &hide_others_i,
                &show_all_i,
                &sep4,
                &quit_i,
            ],
        )?
    };

    // 2. File Submenu
    let file_submenu = {
        let add_torrent_i = MenuItem::with_id(
            app,
            "menu-add-torrent",
            "Add Torrent File…",
            true,
            Some("CmdOrCtrl+O"),
        )?;
        let add_link_i = MenuItem::with_id(
            app,
            "menu-add-link",
            "Add Torrent Link / Magnet…",
            true,
            Some("CmdOrCtrl+U"),
        )?;
        let create_torrent_i = MenuItem::with_id(
            app,
            "menu-create-torrent",
            "Create New Torrent…",
            true,
            Some("CmdOrCtrl+N"),
        )?;
        let sep = PredefinedMenuItem::separator(app)?;
        let close_i = PredefinedMenuItem::close_window(app, None)?;

        Submenu::with_items(
            app,
            "File",
            true,
            &[
                &add_torrent_i,
                &add_link_i,
                &create_torrent_i,
                &sep,
                &close_i,
            ],
        )?
    };

    // 3. Edit Submenu
    let edit_submenu = {
        let undo_i = PredefinedMenuItem::undo(app, None)?;
        let redo_i = PredefinedMenuItem::redo(app, None)?;
        let sep1 = PredefinedMenuItem::separator(app)?;
        let cut_i = PredefinedMenuItem::cut(app, None)?;
        let copy_i = PredefinedMenuItem::copy(app, None)?;
        let paste_i = PredefinedMenuItem::paste(app, None)?;
        let select_all_i = PredefinedMenuItem::select_all(app, None)?;
        let sep2 = PredefinedMenuItem::separator(app)?;
        let resume_i = MenuItem::with_id(
            app,
            "menu-resume-torrent",
            "Resume Selected",
            true,
            None::<&str>,
        )?;
        let pause_i = MenuItem::with_id(
            app,
            "menu-pause-torrent",
            "Pause Selected",
            true,
            None::<&str>,
        )?;
        let delete_i = MenuItem::with_id(
            app,
            "menu-delete-torrent",
            "Delete Selected…",
            true,
            Some("CmdOrCtrl+Backspace"),
        )?;

        Submenu::with_items(
            app,
            "Edit",
            true,
            &[
                &undo_i,
                &redo_i,
                &sep1,
                &cut_i,
                &copy_i,
                &paste_i,
                &select_all_i,
                &sep2,
                &resume_i,
                &pause_i,
                &delete_i,
            ],
        )?
    };

    // 4. View Submenu
    let view_submenu = {
        let media_i = MenuItem::with_id(
            app,
            "menu-view-media",
            "Media Center",
            true,
            Some("CmdOrCtrl+1"),
        )?;
        let manager_i = MenuItem::with_id(
            app,
            "menu-view-manager",
            "Torrent Manager",
            true,
            Some("CmdOrCtrl+2"),
        )?;
        let sep1 = PredefinedMenuItem::separator(app)?;
        let compact_i = MenuItem::with_id(
            app,
            "menu-view-compact",
            "Toggle Compact Mode",
            true,
            Some("CmdOrCtrl+Shift+C"),
        )?;
        let fullscreen_i = PredefinedMenuItem::fullscreen(app, None)?;
        let sep2 = PredefinedMenuItem::separator(app)?;
        let doctor_i = MenuItem::with_id(
            app,
            "menu-tools-doctor",
            "Connectivity Doctor…",
            true,
            None::<&str>,
        )?;
        let vpn_i = MenuItem::with_id(
            app,
            "menu-tools-vpn",
            "VPN & Kill Switch Manager…",
            true,
            None::<&str>,
        )?;
        let tracker_i = MenuItem::with_id(
            app,
            "menu-tools-tracker",
            "Asian DVD Club Tracker Dashboard…",
            true,
            None::<&str>,
        )?;

        Submenu::with_items(
            app,
            "View",
            true,
            &[
                &media_i,
                &manager_i,
                &sep1,
                &compact_i,
                &fullscreen_i,
                &sep2,
                &doctor_i,
                &vpn_i,
                &tracker_i,
            ],
        )?
    };

    // 5. Window Submenu
    let window_submenu = {
        let minimize_i = PredefinedMenuItem::minimize(app, None)?;
        let zoom_i = PredefinedMenuItem::maximize(app, None)?;
        let sep = PredefinedMenuItem::separator(app)?;
        let bring_all_i = PredefinedMenuItem::bring_all_to_front(app, None)?;

        Submenu::with_items(
            app,
            "Window",
            true,
            &[&minimize_i, &zoom_i, &sep, &bring_all_i],
        )?
    };

    // 6. Help Submenu (preserves Factory's Discord Support entry)
    let help_submenu = {
        let docs_i = MenuItem::with_id(
            app,
            "menu-help-docs",
            "Documentation & Keyboard Shortcuts",
            true,
            Some("F1"),
        )?;
        let discord_i = MenuItem::with_id(
            app,
            "menu-help-discord",
            "Discord Support (#nebulae-support)",
            true,
            None::<&str>,
        )?;
        let sep = PredefinedMenuItem::separator(app)?;
        let about_i = MenuItem::with_id(app, "menu-about", "About Nebulae", true, None::<&str>)?;

        Submenu::with_items(app, "Help", true, &[&docs_i, &discord_i, &sep, &about_i])?
    };

    Menu::with_items(
        app,
        &[
            &app_submenu,
            &file_submenu,
            &edit_submenu,
            &view_submenu,
            &window_submenu,
            &help_submenu,
        ],
    )
}

/// Dispatches menu actions to the active main webview window.
pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();
    if let Some(window) = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())
    {
        let _ = window.emit("nebulae://menu-action", id);
    }
}
