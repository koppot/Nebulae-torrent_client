use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use tracing_subscriber::EnvFilter;

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

        // Redact tracker announce parameters and ADC private swarms
        let is_tracker = sanitized.contains("announce") || sanitized.contains("tracker");
        let has_secret = sanitized.contains("passkey")
            || sanitized.contains("auth")
            || sanitized.contains("token")
            || sanitized.contains("key=")
            || sanitized.contains("asiandvdclub.org");
        if is_tracker && has_secret {
            sanitized = "[REDACTED_TRACKER_URL]".to_string();
        }

        sanitized
    }

    pub fn init_tracing() {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(
                "info,librqbit_tracker_comms=warn,librqbit::tracker=warn,librqbit::tracker_comms=warn",
            )
        });
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init();
    }

    pub fn log_to_file(logs_dir: &Path, level: &str, message: &str) {
        let clean_msg = Self::sanitize(message);
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let line = format!("[{timestamp}] [{level}] {clean_msg}\n");

        let log_file_path = logs_dir.join("nebulae_session.log");
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file_path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_magnet_and_passkey() {
        let magnet_log = "Processing magnet:?xt=urn:btih:abcdef1234567890 in background";
        let sanitized_magnet = SafeLogger::sanitize(magnet_log);
        assert!(!sanitized_magnet.contains("urn:btih:abcdef"));
        assert!(sanitized_magnet.contains("[REDACTED_MAGNET_LINK]"));

        let tracker_log =
            "Announce to http://tracker.asiandvdclub.org:2710/secretpasskey1234/announce failed";
        let sanitized_tracker = SafeLogger::sanitize(tracker_log);
        assert!(!sanitized_tracker.contains("secretpasskey1234"));
        assert_eq!(sanitized_tracker, "[REDACTED_TRACKER_URL]");

        let param_log = "Querying https://example.com/announce?passkey=abcdef123&info_hash=123";
        let sanitized_param = SafeLogger::sanitize(param_log);
        assert!(!sanitized_param.contains("abcdef123"));
        assert_eq!(sanitized_param, "[REDACTED_TRACKER_URL]");
    }
}
