use crate::db::Database;
use crate::engine::SharedEngine;
use crate::models::vpn::{ParsedWireGuardProfile, VpnMode, VpnProfile, VpnProtocol, VpnStatus};
use chrono::Utc;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub fn get_vpn_status(
    db: State<Arc<Database>>,
    engine: State<SharedEngine>,
) -> Result<VpnStatus, String> {
    let mode_str = db
        .get_setting("vpn_mode")
        .unwrap_or(None)
        .unwrap_or_else(|| "NoVpn".to_string());
    let mode = match mode_str.as_str() {
        "SystemVpn" => VpnMode::SystemVpn,
        "AdcOnlyVpn" => VpnMode::AdcOnlyVpn,
        _ => VpnMode::NoVpn,
    };

    let kill_switch_active = db
        .get_setting("vpn_kill_switch")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(true);

    let active_vpn = crate::utils::network::find_active_vpn_interface();

    let (connected, interface_name, assigned_ip) = if let Some(ref iface) = active_vpn {
        (
            true,
            Some(iface.name.clone()),
            iface.ip_addresses.first().cloned(),
        )
    } else {
        (false, None, None)
    };

    // Calculate real data volume transferred through active torrents
    let (bytes_sent, bytes_received) = if let Ok(items) = engine.list_torrents() {
        let mut sent = 0u64;
        let mut recv = 0u64;
        for t in items {
            sent = sent.saturating_add(t.uploaded_bytes);
            recv = recv.saturating_add(t.downloaded_bytes);
        }
        (sent, recv)
    } else {
        (0, 0)
    };

    let endpoint = if let Ok(profiles) = db.list_vpn_profiles() {
        profiles.first().map(|p| p.endpoint.clone())
    } else {
        None
    };

    let kill_switch_triggered = engine.is_kill_switch_active();

    Ok(VpnStatus {
        connected,
        interface_name,
        assigned_ip,
        endpoint,
        handshake_age_secs: None,
        bytes_sent,
        bytes_received,
        kill_switch_active,
        kill_switch_triggered,
        mode,
    })
}

#[tauri::command]
pub fn set_vpn_mode(mode: VpnMode, db: State<Arc<Database>>) -> Result<(), String> {
    let mode_str = match mode {
        VpnMode::NoVpn => "NoVpn",
        VpnMode::SystemVpn => "SystemVpn",
        VpnMode::AdcOnlyVpn => "AdcOnlyVpn",
    };
    db.set_setting("vpn_mode", mode_str)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn toggle_kill_switch(enabled: bool, db: State<Arc<Database>>) -> Result<(), String> {
    db.set_setting("vpn_kill_switch", if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn import_wireguard_profile(
    content: String,
    profile_name: String,
    db: State<Arc<Database>>,
) -> Result<VpnProfile, String> {
    let parsed = ParsedWireGuardProfile::parse_conf(&content)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();

    // Securely route private key to OS Keychain via keyring crate
    let _ = crate::utils::keyring::store_secret(
        &crate::utils::keyring::vpn_secret_target(&id),
        &parsed.private_key,
    );

    let profile = VpnProfile {
        id: id.clone(),
        name: if profile_name.trim().is_empty() {
            "WireGuard Tunnel".to_string()
        } else {
            profile_name
        },
        protocol: VpnProtocol::WireGuard,
        endpoint: parsed.endpoint,
        interface_address: parsed.address,
        dns_servers: parsed.dns,
        requires_auth: false,
        created_at: now,
    };

    db.insert_vpn_profile(&profile).map_err(|e| e.to_string())?;
    Ok(profile)
}

#[tauri::command]
pub fn import_openvpn_profile(
    content: String,
    profile_name: String,
    db: State<Arc<Database>>,
) -> Result<VpnProfile, String> {
    let parsed = crate::models::vpn::ParsedOpenVpnProfile::parse_ovpn(&content)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();

    if let Some(ref client_key) = parsed.client_key {
        let _ = crate::utils::keyring::store_secret(
            &crate::utils::keyring::vpn_secret_target(&id),
            client_key,
        );
    }

    let profile = VpnProfile {
        id: id.clone(),
        name: if profile_name.trim().is_empty() {
            "OpenVPN Tunnel".to_string()
        } else {
            profile_name
        },
        protocol: VpnProtocol::OpenVpn,
        endpoint: parsed.endpoint,
        interface_address: if parsed.dev.is_empty() {
            "tun".to_string()
        } else {
            parsed.dev
        },
        dns_servers: parsed.dns_servers,
        requires_auth: parsed.client_key.is_some() || parsed.client_cert.is_some(),
        created_at: now,
    };

    db.insert_vpn_profile(&profile).map_err(|e| e.to_string())?;
    Ok(profile)
}

#[tauri::command]
pub fn list_vpn_profiles(db: State<Arc<Database>>) -> Result<Vec<VpnProfile>, String> {
    db.list_vpn_profiles().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_vpn_profile(id: String, db: State<Arc<Database>>) -> Result<(), String> {
    let _ = crate::utils::keyring::delete_secret(&crate::utils::keyring::vpn_secret_target(&id));
    db.delete_vpn_profile(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_network_interfaces() -> Result<Vec<crate::utils::network::NetworkInterfaceInfo>, String>
{
    Ok(crate::utils::network::list_network_interfaces())
}
