use crate::utils::media_parser::{MediaTitleParser, ParsedMediaMetadata};
use std::path::Path;

#[tauri::command]
pub fn parse_media_title(raw_title: String) -> Result<ParsedMediaMetadata, String> {
    Ok(MediaTitleParser::parse(&raw_title))
}

#[tauri::command]
pub fn launch_media_player(file_path: String) -> Result<(), String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("Media file does not exist: {file_path}"));
    }

    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg(&file_path)
            .status()
            .map_err(|e| format!("Failed to launch media player on macOS: {e}"))?;

        if !status.success() {
            return Err("Failed to launch default media player".to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd = crate::utils::process::create_hidden_command("cmd.exe");
        cmd.args(["/C", "start", "", &file_path]);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to launch media player on Windows: {e}"))?;

        if !status.success() {
            return Err("Failed to launch default media player on Windows".to_string());
        }
    }

    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("xdg-open")
            .arg(&file_path)
            .status()
            .map_err(|e| format!("Failed to launch media player on Linux: {e}"))?;

        if !status.success() {
            return Err("Failed to launch media player via xdg-open".to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_nonexistent_media() {
        let res = launch_media_player("/nonexistent/movie/file.mkv".to_string());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_parse_media_title_command() {
        let meta =
            parse_media_title("Ikiru.1952.Remastered.1080p.BluRay.x264.FLAC-ADC".to_string())
                .expect("parsing failed");
        assert_eq!(meta.clean_title, "Ikiru");
        assert_eq!(meta.year, Some(1952));
        assert_eq!(meta.resolution, Some("1080p".to_string()));
        assert_eq!(meta.source, Some("BluRay".to_string()));
        assert_eq!(meta.codec, Some("x264".to_string()));
        assert_eq!(meta.audio, Some("FLAC".to_string()));
        assert_eq!(meta.release_group, Some("ADC".to_string()));
    }
}
