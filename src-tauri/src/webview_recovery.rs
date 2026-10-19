//! Stale WebView2 process recovery (Windows).
//!
//! A hard-killed app leaves the WebView2 browser processes running. They
//! keep the EBWebView profile locked, and the next launch fails WebView2
//! initialization with 0x8007139F / 0x80070578 (observed on the Windows
//! test desktop after repeated hard kills).
//!
//! Recovery rules (agreed with the desktop team, narrow on purpose):
//!   - Ownership requires ALL of: process name is msedgewebview2.exe, its
//!     command line carries OUR identifier-scoped user-data marker
//!     (`org.asiandvdclub.nebulae\EBWebView`), and its PARENT process is
//!     gone. Other applications' webviews (different marker) and any live
//!     instance's webviews (parent alive) are never touched.
//!   - The profile directory is never deleted or modified; the lock
//!     releases when the stale holder exits.
//!   - Non-Windows platforms: no-op (WKWebView has no equivalent orphan
//!     lock problem; the call site stays platform-neutral).

/// Command-line marker that pins cleanup to THIS app's webview profile:
/// the Tauri identifier plus WebView2's EBWebView folder name.
pub const WEBVIEW_MARKER: &str = r"org.asiandvdclub.nebulae\EBWebView";

/// Process name owned by WebView2 runtimes.
pub const WEBVIEW_PROCESS: &str = "msedgewebview2.exe";

/// PowerShell that terminates only stale, owned webview runtimes.
/// Unit-tested below: the marker must appear, and the parent-liveness
/// check must gate every kill.
pub fn recovery_script() -> String {
    let filter = format!("Name='{WEBVIEW_PROCESS}'");
    let like = format!("*{WEBVIEW_MARKER}*");
    format!(
        "$ErrorActionPreference='SilentlyContinue'; \
         Get-CimInstance Win32_Process -Filter \"{filter}\" | \
         ForEach-Object {{ \
           if ($_.CommandLine -like '{like}') {{ \
             if (-not (Get-Process -Id $_.ParentProcessId -ErrorAction SilentlyContinue)) {{ \
               Stop-Process -Id $_.ProcessId -Force; \
               Write-Output $_.ProcessId; \
             }} \
           }} \
         }}"
    )
}

/// Run the recovery pass. Returns the stale PIDs that were terminated
/// (empty when clean, and always empty on non-Windows).
pub fn recover_stale_webviews() -> Vec<u32> {
    #[cfg(target_os = "windows")]
    {
        run_recovery()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
fn run_recovery() -> Vec<u32> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &recovery_script(),
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .split_whitespace()
            .filter_map(|s| s.parse::<u32>().ok())
            .collect(),
        Ok(o) => {
            tracing::warn!(
                "[webview-recovery] probe failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            Vec::new()
        }
        Err(e) => {
            tracing::warn!("[webview-recovery] powershell spawn: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_is_scoped_to_our_marker_and_dead_parents() {
        let s = recovery_script();
        assert!(s.contains(WEBVIEW_MARKER), "marker missing");
        assert!(s.contains(WEBVIEW_PROCESS), "process name missing");
        // Every Stop-Process is gated behind the dead-parent check.
        let gate = s.find("ParentProcessId").expect("parent check");
        let kill = s.find("Stop-Process").expect("kill");
        assert!(gate < kill, "kill must follow the parent-liveness check");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn no_op_on_non_windows() {
        assert!(recover_stale_webviews().is_empty());
    }
}
