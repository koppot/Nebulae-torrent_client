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
        Self::init_tracing_with_file(None);
    }

    /// Single tracing subscriber writing to BOTH stderr and the session log.
    /// Without the file leg, `tracing::info!` diagnostics (player stages,
    /// webview breadcrumbs) vanish to stderr and the log we ship to the
    /// owner stays empty — which is exactly what hid the playback failure.
    pub fn init_tracing_with_file(logs_dir: Option<&Path>) {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let file = logs_dir.and_then(|dir| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("nebulae_session.log"))
                .ok()
        });
        let make = DualMakeWriter {
            file: std::sync::Arc::new(std::sync::Mutex::new(file)),
        };
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(make)
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

/// Writer that fans each tracing event out to stderr AND the session log.
struct DualMakeWriter {
    file: std::sync::Arc<std::sync::Mutex<Option<std::fs::File>>>,
}

struct DualGuard<'a> {
    file: std::sync::Arc<std::sync::Mutex<Option<std::fs::File>>>,
    _lifetime: std::marker::PhantomData<&'a ()>,
    line: Vec<u8>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for DualMakeWriter {
    type Writer = DualGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        DualGuard {
            file: std::sync::Arc::clone(&self.file),
            _lifetime: std::marker::PhantomData,
            line: Vec::new(),
        }
    }
}

impl std::io::Write for DualGuard<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // tracing events arrive as complete line-formatted writes; buffer
        // them so the file gets a timestamped copy while stderr stays raw.
        self.line.extend_from_slice(buf);
        std::io::stderr().write_all(buf)?;
        // Emit complete lines to the file immediately instead of trusting
        // the subscriber to call flush after every event.
        if self.line.ends_with(b"\n") {
            self.flush_to_file();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_to_file();
        std::io::stderr().flush()
    }
}

impl DualGuard<'_> {
    fn flush_to_file(&mut self) {
        if self.line.is_empty() {
            return;
        }
        let ts = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let text = String::from_utf8_lossy(&self.line)
            .trim_end_matches('\n')
            .to_string();
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(format!("[{ts}] {text}\n").as_bytes());
            }
        }
        self.line.clear();
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

    #[test]
    fn test_sanitize_tokens_and_auth_parameters() {
        let token_log = "tracker announce returned error token=secret_token_9999";
        let sanitized_token = SafeLogger::sanitize(token_log);
        assert!(!sanitized_token.contains("secret_token_9999"));
        assert_eq!(sanitized_token, "[REDACTED_TRACKER_URL]");

        let auth_log = "connecting to tracker https://adc.org/announce?auth=supersecretauth";
        let sanitized_auth = SafeLogger::sanitize(auth_log);
        assert!(!sanitized_auth.contains("supersecretauth"));
        assert_eq!(sanitized_auth, "[REDACTED_TRACKER_URL]");

        let non_secret_log = "Engine listening on port 41413";
        let normal = SafeLogger::sanitize(non_secret_log);
        assert_eq!(normal, "Engine listening on port 41413");
    }

    #[test]
    fn test_safe_logger_file_output_redaction() {
        let tmp = std::env::temp_dir().join(format!(
            "nebulae_log_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();

        SafeLogger::log_to_file(
            &tmp,
            "INFO",
            "Announce to https://tracker.asiandvdclub.org:2710/mysecretpasskey/announce succeeded",
        );
        SafeLogger::log_to_file(
            &tmp,
            "INFO",
            "Loaded magnet:?xt=urn:btih:0123456789abcdef from CLI",
        );

        let content = std::fs::read_to_string(tmp.join("nebulae_session.log")).unwrap();
        assert!(!content.contains("mysecretpasskey"));
        assert!(!content.contains("urn:btih:0123456789abcdef"));
        assert!(content.contains("[REDACTED_TRACKER_URL]"));
        assert!(content.contains("[REDACTED_MAGNET_LINK]"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_tracing_events_reach_the_session_file() {
        // Regression: the tracing subscriber used to write to stderr only,
        // so player-stage and webview breadcrumbs never appeared in the
        // session log (the empty-log mystery). Both legs must land.
        let tmp = std::env::temp_dir().join(format!(
            "nebulae_tracing_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();

        SafeLogger::init_tracing_with_file(Some(&tmp));
        tracing::info!("tracing-file-regression-check");

        let content = std::fs::read_to_string(tmp.join("nebulae_session.log"))
            .expect("session log must exist");
        assert!(
            content.contains("tracing-file-regression-check"),
            "tracing events must land in the session log, got: {content:?}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
