use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdcTrackerProfile {
    pub username: String,
    pub masked_passkey: String,
    pub is_linked: bool,
    pub last_announce_time: Option<i64>,
    pub next_announce_countdown_sec: Option<u32>,
    pub announce_status: String,
    pub ratio: f32,
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
    pub freeleech_active: bool,
    pub bonus_points: f32,
    pub required_seed_time_hours: u32,
}

impl AdcTrackerProfile {
    pub fn mask_passkey(raw: &str) -> String {
        if raw.len() <= 8 {
            "••••••••".to_string()
        } else {
            let tail = &raw[raw.len() - 4..];
            format!("adc_pk_••••••••••••••••{tail}")
        }
    }

    pub fn calculate_ratio(uploaded: u64, downloaded: u64) -> f32 {
        if downloaded == 0 {
            if uploaded > 0 {
                999.0
            } else {
                1.0
            }
        } else {
            (uploaded as f64 / downloaded as f64) as f32
        }
    }

    pub fn project_ratio(uploaded: u64, downloaded: u64, additional_download: u64) -> f32 {
        let total_dl = downloaded + additional_download;
        Self::calculate_ratio(uploaded, total_dl)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SeedTimeRecord {
    pub info_hash: String,
    pub torrent_name: String,
    pub seeding_time_seconds: u64,
    pub required_seeding_seconds: u64,
    pub is_shield_satisfied: bool,
}

impl SeedTimeRecord {
    pub fn new(info_hash: String, torrent_name: String, required_hours: u32) -> Self {
        let req_sec = required_hours as u64 * 3600;
        Self {
            info_hash,
            torrent_name,
            seeding_time_seconds: 0,
            required_seeding_seconds: req_sec,
            is_shield_satisfied: false,
        }
    }

    pub fn add_seeding_seconds(&mut self, seconds: u64) {
        self.seeding_time_seconds += seconds;
        if self.seeding_time_seconds >= self.required_seeding_seconds {
            self.is_shield_satisfied = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_passkey_masking() {
        let raw = "abcdef1234567890abcdef1234567890";
        let masked = AdcTrackerProfile::mask_passkey(raw);
        assert!(masked.starts_with("adc_pk_••••"));
        assert!(masked.ends_with("7890"));
        assert!(!masked.contains("123456"));
    }

    #[test]
    fn test_ratio_projection() {
        let uploaded = 100_000_000_000; // 100 GB
        let downloaded = 50_000_000_000; // 50 GB -> Ratio 2.0
        let new_dl = 25_000_000_000; // +25 GB -> 100 / 75 = 1.33

        let proj = AdcTrackerProfile::project_ratio(uploaded, downloaded, new_dl);
        assert!((proj - 1.333).abs() < 0.01);
    }
}
