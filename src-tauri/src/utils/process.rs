use std::ffi::OsStr;
use std::io;
use std::process::{Command, Output};

#[cfg(target_os = "windows")]
pub const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Creates a standard Command configured with CREATE_NO_WINDOW on Windows
/// to ensure no console window, command prompt, or terminal flashes visibly.
pub fn create_hidden_command<S: AsRef<OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs a PowerShell script completely hidden on Windows without any visible console window,
/// flashing prompt, or interactive profile loading.
pub fn run_hidden_powershell(script: &str) -> io::Result<Output> {
    let mut cmd = create_hidden_command("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    cmd.output()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_hidden_command() {
        let cmd = create_hidden_command("echo");
        assert_eq!(cmd.get_program(), "echo");
    }

    #[test]
    fn test_hidden_powershell_structure() {
        let mut cmd = create_hidden_command("powershell");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "Write-Output 'OK'",
        ]);
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert!(args.contains(&OsStr::new("-NoProfile")));
        assert!(args.contains(&OsStr::new("-NonInteractive")));
        assert!(args.contains(&OsStr::new("-WindowStyle")));
        assert!(args.contains(&OsStr::new("Hidden")));
    }
}
