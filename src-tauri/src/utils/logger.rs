use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub struct SafeLogger;

impl SafeLogger {
    pub fn sanitize(message: &str) -> String {
        // Redact magnet URIs and tracker parameters
        let mut sanitized = message.to_string();

        if let Some(start) = sanitized.find("magnet:?") {
            if let Some(end) = sanitized[start..].find(' ') {
                sanitized.replace_range(start..start + end, "[REDACTED_MAGNET_LINK]");
            } else {
                sanitized.replace_range(start.., "[REDACTED_MAGNET_LINK]");
            }
        }

        // Redact tracker announce parameters
        if sanitized.contains("announce") && sanitized.contains("passkey") {
            sanitized = "[REDACTED_TRACKER_URL]".to_string();
        }

        sanitized
    }

    pub fn log_to_file(logs_dir: &Path, level: &str, message: &str) {
        let clean_msg = Self::sanitize(message);
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let line = format!("[{timestamp}] [{level}] {clean_msg}\n");

        let log_file_path = logs_dir.join("adc_session.log");
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file_path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
}
