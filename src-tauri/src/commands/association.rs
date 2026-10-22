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

fn status(platform: &str, flags: [bool; 4], message: &str) -> AssociationStatus {
    AssociationStatus {
        torrent_files: flags[0],
        adc_files: flags[1],
        magnet_protocol: flags[2],
        adc_protocol: flags[3],
        platform: platform.into(),
        message: message.into(),
    }
}

#[tauri::command]
pub fn get_pending_open_payloads(state: State<PendingOpenPayloads>) -> Vec<String> {
    state
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_associations(register: bool) -> Result<AssociationStatus, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$exe = $env:NEBULAE_ASSOC_EXE
if ($env:NEBULAE_ASSOC_REGISTER -eq '1') {
    $root = [Microsoft.Win32.Registry]::CurrentUser
    function Put([string]$path, [string]$name, [string]$value) {
        $key = $root.CreateSubKey('Software\Classes\' + $path)
        try { $key.SetValue($name, $value, [Microsoft.Win32.RegistryValueKind]::String) }
        finally { $key.Dispose() }
    }
    $command = '"' + $exe + '" "%1"'
    $icon = '"' + $exe + '",0'
    foreach ($extension in @('torrent', 'adc')) {
        $id = 'Nebulae.' + $extension
        Put ('.' + $extension) '' $id
        Put $id '' ('Nebulae ' + $extension + ' file')
        Put ($id + '\DefaultIcon') '' $icon
        Put ($id + '\shell\open\command') '' $command
    }
    foreach ($scheme in @('magnet', 'adc')) {
        Put $scheme '' ('URL:Nebulae ' + $scheme)
        Put $scheme 'URL Protocol' ''
        Put ($scheme + '\DefaultIcon') '' $icon
        Put ($scheme + '\shell\open\command') '' $command
    }
}
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NebulaeAssociations {
    [DllImport("shlwapi.dll", CharSet = CharSet.Unicode)]
    public static extern int AssocQueryStringW(uint flags, uint str, string assoc, string extra, StringBuilder output, ref uint length);
    [DllImport("shell32.dll")]
    public static extern void SHChangeNotify(uint eventId, uint flags, IntPtr item1, IntPtr item2);
    public static string Executable(string assoc, uint flags) {
        uint length = 32768;
        var output = new StringBuilder((int)length);
        int result = AssocQueryStringW(flags, 2, assoc, "open", output, ref length);
        return result == 0 ? output.ToString() : "";
    }
}
'@
if ($env:NEBULAE_ASSOC_REGISTER -eq '1') {
    [NebulaeAssociations]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}
function Owned([string]$association, [uint32]$flags) {
    return [string]::Equals([NebulaeAssociations]::Executable($association, $flags), $exe, [StringComparison]::OrdinalIgnoreCase)
}
@{
    torrent_files = (Owned '.torrent' 0)
    adc_files = (Owned '.adc' 0)
    magnet_protocol = (Owned 'magnet' 0x1000)
    adc_protocol = (Owned 'adc' 0x1000)
    platform = 'Windows'
    message = 'Effective handlers queried from Windows. False means another handler is selected or no executable handler was found. Registration does not override protected user defaults; select Nebulae in Windows Default Apps when necessary.'
} | ConvertTo-Json -Compress
"#;
    let output = crate::utils::process::create_hidden_command("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .env("NEBULAE_ASSOC_EXE", exe)
        .env("NEBULAE_ASSOC_REGISTER", if register { "1" } else { "0" })
        .output()
        .map_err(|e| format!("Cannot run Windows association operation: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Windows association operation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid Windows association response: {e}"))
}

#[cfg(target_os = "linux")]
const DESKTOP_ID: &str = "org.asiandvdclub.nebulae.desktop";
#[cfg(target_os = "linux")]
const MIME_TYPES: [&str; 4] = [
    "application/x-bittorrent",
    "application/x-asiandvdclub-torrent",
    "x-scheme-handler/magnet",
    "x-scheme-handler/adc",
];

#[cfg(target_os = "linux")]
fn xdg(args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("xdg-mime")
        .args(args)
        .output()
        .map_err(|e| {
            format!("Cannot run xdg-mime; install xdg-utils and use a desktop session: {e}")
        })?;
    if !out.status.success() {
        return Err(format!(
            "xdg-mime operation failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(any(target_os = "linux", test))]
fn desktop_exec(path: &str) -> Result<String, String> {
    if path.contains(['\n', '\r', '\0', '=']) {
        return Err("Executable path contains unsupported desktop-entry characters".into());
    }
    let mut quoted = String::from("\"");
    for c in path.chars() {
        match c {
            '\\' => quoted.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(c),
        }
    }
    quoted.push_str("\" %U");
    Ok(quoted)
}

#[cfg(target_os = "linux")]
fn linux_status() -> Result<AssociationStatus, String> {
    let mut flags = [false; 4];
    for (index, mime) in MIME_TYPES.iter().enumerate() {
        flags[index] = xdg(&["query", "default", mime])? == DESKTOP_ID;
    }
    Ok(status("Linux", flags, "Queried XDG desktop defaults. False means this Nebulae desktop entry is not the selected default."))
}

#[tauri::command]
pub fn check_associations_status() -> Result<AssociationStatus, String> {
    #[cfg(target_os = "windows")]
    {
        windows_associations(false)
    }
    #[cfg(target_os = "linux")]
    {
        linux_status()
    }
    #[cfg(target_os = "macos")]
    {
        Ok(status("macOS", [true; 4], "Handlers are declared by the application bundle; this does not verify that Nebulae is the selected default."))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err("Association verification is unsupported on this platform".into())
    }
}

#[tauri::command]
pub fn register_protocol_associations() -> Result<AssociationStatus, String> {
    #[cfg(target_os = "windows")]
    {
        windows_associations(true)
    }
    #[cfg(target_os = "linux")]
    {
        let executable = std::env::var_os("APPIMAGE")
            .map(std::path::PathBuf::from)
            .map(Ok)
            .unwrap_or_else(std::env::current_exe)
            .map_err(|e| e.to_string())?;
        if !executable.is_absolute() || !executable.is_file() {
            return Err("Cannot register a missing or relative application executable".into());
        }
        let executable = executable
            .to_str()
            .ok_or("Executable path must be UTF-8 for a desktop entry")?;
        let exec = desktop_exec(executable)?;
        let data = dirs::data_dir().ok_or("Cannot resolve user data directory")?;
        let applications = data.join("applications");
        let packages = data.join("mime/packages");
        std::fs::create_dir_all(&applications).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&packages).map_err(|e| e.to_string())?;
        let desktop = format!("[Desktop Entry]\nType=Application\nName=Nebulae\nExec={exec}\nIcon=nebulae\nTerminal=false\nCategories=Network;FileTransfer;\nMimeType={};\n", MIME_TYPES.join(";"));
        std::fs::write(applications.join(DESKTOP_ID), desktop)
            .map_err(|e| format!("Cannot write desktop entry: {e}"))?;
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info"><mime-type type="application/x-asiandvdclub-torrent"><comment>Asian DVD Club Metainfo</comment><glob pattern="*.adc"/></mime-type></mime-info>
"#;
        std::fs::write(packages.join("nebulae-adc.xml"), xml).map_err(|e| e.to_string())?;
        for (tool, directory) in [
            ("update-mime-database", data.join("mime")),
            ("update-desktop-database", applications),
        ] {
            let result = std::process::Command::new(tool)
                .arg(directory)
                .output()
                .map_err(|e| format!("Cannot run {tool}: {e}"))?;
            if !result.status.success() {
                return Err(format!(
                    "{tool} failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                ));
            }
        }
        for mime in MIME_TYPES {
            xdg(&["default", DESKTOP_ID, mime])?;
        }
        linux_status()
    }
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let bundle = exe
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or("Association registration requires an installed Nebulae.app bundle")?;
        let out = std::process::Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
            .arg("-f").arg(bundle).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "LaunchServices registration failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        check_associations_status()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err("Association registration is unsupported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_preserves_missing_handlers() {
        let s = status("test", [false, true, false, true], "checked");
        assert!(!s.torrent_files);
        assert!(s.adc_files);
        assert!(!s.magnet_protocol);
        assert!(s.adc_protocol);
    }
    #[test]
    fn queue_drains_once() {
        let queue = PendingOpenPayloads::default();
        queue
            .0
            .lock()
            .unwrap()
            .push("magnet:?xt=urn:btih:abc".into());
        let values: Vec<_> = queue.0.lock().unwrap().drain(..).collect();
        assert_eq!(values.len(), 1);
        assert!(queue.0.lock().unwrap().is_empty());
    }
    #[test]
    fn desktop_paths_are_data() {
        assert_eq!(
            desktop_exec("/opt/Nebulae App/nebulae").unwrap(),
            "\"/opt/Nebulae App/nebulae\" %U"
        );
        assert!(desktop_exec("/tmp/bad\nExec=other").is_err());
        assert!(desktop_exec("/tmp/100%/nebulae").unwrap().contains("100%%"));
    }
}
