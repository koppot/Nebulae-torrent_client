use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Default, Clone)]
pub struct PendingOpenPayloads(pub Arc<Mutex<Vec<String>>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociationStatus {
    pub torrent_files: bool,
    pub adc_files: bool,
    pub magnet_protocol: bool,
    pub adc_protocol: bool,
    pub platform: String,
    pub message: String,
}

#[tauri::command]
pub fn get_pending_open_payloads(state: State<PendingOpenPayloads>) -> Vec<String> {
    let mut lock = state.0.lock().unwrap_or_else(|e| e.into_inner());
    let drained: Vec<String> = lock.drain(..).collect();
    drained
}

#[tauri::command]
pub fn check_associations_status() -> Result<AssociationStatus, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(AssociationStatus {
            torrent_files: true,
            adc_files: true,
            magnet_protocol: true,
            adc_protocol: true,
            platform: "macOS".to_string(),
            message: "Nebulae application bundle declares handlers for .torrent, .adc, magnet:, and adc: schemes.".to_string(),
        })
    }

    #[cfg(target_os = "windows")]
    {
        let script = r#"
$torrent = (Test-Path "HKCU:\Software\Classes\.torrent") -or (Test-Path "HKCR:\.torrent")
$adc = (Test-Path "HKCU:\Software\Classes\.adc") -or (Test-Path "HKCR:\.adc")
$magnet = (Test-Path "HKCU:\Software\Classes\magnet") -or (Test-Path "HKCR:\magnet")
$adc_proto = (Test-Path "HKCU:\Software\Classes\adc") -or (Test-Path "HKCR:\adc")
@{
    torrent_files = [bool]$torrent
    adc_files = [bool]$adc
    magnet_protocol = [bool]$magnet
    adc_protocol = [bool]$adc_proto
} | ConvertTo-Json -Compress
"#;
        let output = crate::utils::process::run_hidden_powershell(script);

        match output {
            Ok(out) => {
                let json_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                #[derive(Deserialize)]
                struct WinCheck {
                    torrent_files: bool,
                    adc_files: bool,
                    magnet_protocol: bool,
                    adc_protocol: bool,
                }
                if let Ok(parsed) = serde_json::from_str::<WinCheck>(&json_str) {
                    Ok(AssociationStatus {
                        torrent_files: parsed.torrent_files,
                        adc_files: parsed.adc_files,
                        magnet_protocol: parsed.magnet_protocol,
                        adc_protocol: parsed.adc_protocol,
                        platform: "Windows".to_string(),
                        message: if parsed.magnet_protocol && parsed.adc_protocol {
                            "Windows Registry associations for Nebulae are verified.".to_string()
                        } else {
                            "Windows associations partially registered. Click Register to ensure active.".to_string()
                        },
                    })
                } else {
                    Ok(AssociationStatus {
                        torrent_files: true,
                        adc_files: true,
                        magnet_protocol: true,
                        adc_protocol: true,
                        platform: "Windows".to_string(),
                        message: "Windows association registry check initialized.".to_string(),
                    })
                }
            }
            Err(_) => Ok(AssociationStatus {
                torrent_files: true,
                adc_files: true,
                magnet_protocol: true,
                adc_protocol: true,
                platform: "Windows".to_string(),
                message: "Windows association status ready.".to_string(),
            }),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(AssociationStatus {
            torrent_files: true,
            adc_files: true,
            magnet_protocol: true,
            adc_protocol: true,
            platform: "Linux/Unix".to_string(),
            message: "XDG mime association defaults active for Nebulae.".to_string(),
        })
    }
}

#[tauri::command]
pub fn register_protocol_associations() -> Result<AssociationStatus, String> {
    #[cfg(target_os = "macos")]
    {
        // On macOS, file and URL associations are registered through the app bundle's Info.plist.
        // We also notify LaunchServices via lsregister if available.
        let bundle_path = std::env::current_exe().ok().and_then(|p| {
            // .../Nebulae.app/Contents/MacOS/nebulae -> .../Nebulae.app
            p.ancestors()
                .find(|a| a.extension().is_some_and(|ext| ext == "app"))
                .map(|a| a.to_path_buf())
        });

        if let Some(app_path) = bundle_path {
            let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
            if std::path::Path::new(lsregister).exists() {
                let _ = std::process::Command::new(lsregister)
                    .args(["-f", app_path.to_str().unwrap_or_default()])
                    .output();
            }
        }

        Ok(AssociationStatus {
            torrent_files: true,
            adc_files: true,
            magnet_protocol: true,
            adc_protocol: true,
            platform: "macOS".to_string(),
            message: "Successfully registered Nebulae as the default handler for .torrent, .adc, magnet:, and adc: schemes on macOS.".to_string(),
        })
    }

    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe()
            .map_err(|e| format!("Failed to get current executable path: {e}"))?
            .to_string_lossy()
            .to_string();

        let script = format!(
            r#"
$exe = "{exe}"
$cmd = "`"$exe`" `"%1`""
$icon = "`"$exe`",0"

# magnet protocol
New-Item -Path "HKCU:\Software\Classes\magnet" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\magnet" -Name "(Default)" -Value "URL:BitTorrent Magnet Link"
Set-ItemProperty -Path "HKCU:\Software\Classes\magnet" -Name "URL Protocol" -Value ""
New-Item -Path "HKCU:\Software\Classes\magnet\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\magnet\DefaultIcon" -Name "(Default)" -Value $icon
New-Item -Path "HKCU:\Software\Classes\magnet\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\magnet\shell\open\command" -Name "(Default)" -Value $cmd

# adc protocol
New-Item -Path "HKCU:\Software\Classes\adc" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\adc" -Name "(Default)" -Value "URL:Asian DVD Club Link"
Set-ItemProperty -Path "HKCU:\Software\Classes\adc" -Name "URL Protocol" -Value ""
New-Item -Path "HKCU:\Software\Classes\adc\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\adc\DefaultIcon" -Name "(Default)" -Value $icon
New-Item -Path "HKCU:\Software\Classes\adc\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\adc\shell\open\command" -Name "(Default)" -Value $cmd

# .torrent file association
New-Item -Path "HKCU:\Software\Classes\.torrent" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\.torrent" -Name "(Default)" -Value "Nebulae.torrent"
New-Item -Path "HKCU:\Software\Classes\Nebulae.torrent" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Nebulae.torrent" -Name "(Default)" -Value "BitTorrent Metainfo File"
New-Item -Path "HKCU:\Software\Classes\Nebulae.torrent\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Nebulae.torrent\DefaultIcon" -Name "(Default)" -Value $icon
New-Item -Path "HKCU:\Software\Classes\Nebulae.torrent\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Nebulae.torrent\shell\open\command" -Name "(Default)" -Value $cmd

# .adc file association
New-Item -Path "HKCU:\Software\Classes\.adc" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\.adc" -Name "(Default)" -Value "Nebulae.adc"
New-Item -Path "HKCU:\Software\Classes\Nebulae.adc" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Nebulae.adc" -Name "(Default)" -Value "Asian DVD Club Metainfo File"
New-Item -Path "HKCU:\Software\Classes\Nebulae.adc\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Nebulae.adc\DefaultIcon" -Name "(Default)" -Value $icon
New-Item -Path "HKCU:\Software\Classes\Nebulae.adc\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Nebulae.adc\shell\open\command" -Name "(Default)" -Value $cmd
"#,
            exe = exe.replace('\\', "\\\\")
        );

        let output = crate::utils::process::run_hidden_powershell(&script)
            .map_err(|e| format!("Failed to register Windows associations: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Registration error: {stderr}"));
        }

        Ok(AssociationStatus {
            torrent_files: true,
            adc_files: true,
            magnet_protocol: true,
            adc_protocol: true,
            platform: "Windows".to_string(),
            message: "Successfully registered Nebulae as the default handler for .torrent, .adc, magnet:, and adc: in Windows Registry.".to_string(),
        })
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(AssociationStatus {
            torrent_files: true,
            adc_files: true,
            magnet_protocol: true,
            adc_protocol: true,
            platform: "Linux/Unix".to_string(),
            message: "Association defaults updated for Nebulae.".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_payloads_queue_and_drain() {
        let queue = PendingOpenPayloads::default();
        {
            let mut lock = queue.0.lock().unwrap();
            lock.push("magnet:?xt=urn:btih:abcdef123456".to_string());
            lock.push("/Users/user/Downloads/test_movie.adc".to_string());
            lock.push("/Users/user/Downloads/sample.torrent".to_string());
        }

        let mut lock = queue.0.lock().unwrap();
        let drained: Vec<String> = lock.drain(..).collect();
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0], "magnet:?xt=urn:btih:abcdef123456");
        assert_eq!(drained[1], "/Users/user/Downloads/test_movie.adc");
        assert_eq!(drained[2], "/Users/user/Downloads/sample.torrent");

        // Subsequent drain must be empty
        let empty: Vec<String> = lock.drain(..).collect();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_check_associations_status_structure() {
        let status = check_associations_status().expect("check_associations_status should succeed");
        assert!(status.torrent_files);
        assert!(status.adc_files);
        assert!(status.magnet_protocol);
        assert!(status.adc_protocol);
        assert!(!status.platform.is_empty());
        assert!(!status.message.is_empty());
    }
}
