use crate::db::Database;
use crate::engine::SharedEngine;
use crate::utils::network::find_active_vpn_interface;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub fn spawn_kill_switch_watcher(engine: SharedEngine, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        let mut triggered_by_watcher = false;

        loop {
            interval.tick().await;

            let kill_switch_armed = db
                .get_setting("vpn_kill_switch")
                .unwrap_or(None)
                .map(|v| v == "true")
                .unwrap_or(true);

            let vpn_mode = db
                .get_setting("vpn_mode")
                .unwrap_or(None)
                .unwrap_or_else(|| "NoVpn".to_string());

            let is_vpn_required = vpn_mode == "AdcOnlyVpn" || vpn_mode == "SystemVpn";

            if kill_switch_armed && is_vpn_required {
                let vpn_active = find_active_vpn_interface().is_some();

                if !vpn_active && !triggered_by_watcher {
                    warn!("VPN interface dropped while kill switch is armed! Pausing transfers to prevent data leak.");
                    let _ = engine.trigger_kill_switch();
                    triggered_by_watcher = true;
                } else if vpn_active && triggered_by_watcher {
                    info!("VPN interface restored. Disengaging kill switch protection.");
                    let _ = engine.reset_kill_switch();
                    let _ = engine.resume_all();
                    triggered_by_watcher = false;
                }
            } else if triggered_by_watcher {
                let _ = engine.reset_kill_switch();
                triggered_by_watcher = false;
            }
        }
    });
}
