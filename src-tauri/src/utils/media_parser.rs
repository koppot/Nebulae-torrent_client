use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ParsedMediaMetadata {
    pub raw_title: String,
    pub clean_title: String,
    pub year: Option<u32>,
    pub resolution: Option<String>,
    pub source: Option<String>,
    pub codec: Option<String>,
    pub audio: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub edition: Option<String>,
    pub release_group: Option<String>,
}

pub struct MediaTitleParser;

impl MediaTitleParser {
    pub fn parse(raw_name: &str) -> ParsedMediaMetadata {
        let mut text = raw_name.trim();

        // 1. Strip file extension if present
        let video_extensions = [
            ".mkv", ".mp4", ".avi", ".iso", ".ts", ".m2ts", ".mov", ".wmv", ".vob",
        ];
        for ext in video_extensions {
            if text.to_lowercase().ends_with(ext) {
                text = &text[..text.len() - ext.len()];
                break;
            }
        }

        // 2. Extract release group if trailing hyphenated tag exists (e.g. -ADC, -SubsPlease)
        let mut release_group = None;
        let mut working_text = text.to_string();
        if let Some(hyphen_idx) = working_text.rfind('-') {
            let candidate_group = working_text[hyphen_idx + 1..].trim();
            // Reasonable release group length (2 to 20 alphanumeric chars)
            if candidate_group.len() >= 2
                && candidate_group.len() <= 20
                && candidate_group
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_')
                && !candidate_group.eq_ignore_ascii_case("x264")
                && !candidate_group.eq_ignore_ascii_case("x265")
            {
                release_group = Some(candidate_group.to_string());
                working_text = working_text[..hyphen_idx].to_string();
            }
        }

        // 3. Normalise delimiters: replace '.', '_', '[', ']', '(', ')' with spaces
        let normalised = working_text.replace(['.', '_', '[', ']', '(', ')'], " ");

        let raw_tokens: Vec<&str> = normalised.split_whitespace().collect();

        let mut year = None;
        let mut resolution = None;
        let mut source = None;
        let mut codec = None;
        let mut audio = None;
        let mut season = None;
        let mut episode = None;
        let mut edition = None;

        let mut first_meta_token_idx = None;

        for (idx, &token) in raw_tokens.iter().enumerate() {
            let lower = token.to_lowercase();

            // Check Season and Episode pattern (e.g. S01E02, s4e28)
            if season.is_none() && lower.starts_with('s') {
                if let Some(e_idx) = lower.find('e') {
                    if e_idx > 1 {
                        let s_part = &lower[1..e_idx];
                        let e_part = &lower[e_idx + 1..];
                        if let (Ok(s_num), Ok(e_num)) =
                            (s_part.parse::<u32>(), e_part.parse::<u32>())
                        {
                            season = Some(s_num);
                            episode = Some(e_num);
                            if first_meta_token_idx.is_none() {
                                first_meta_token_idx = Some(idx);
                            }
                            continue;
                        }
                    }
                }
            }

            // Check Year (1900 to 2099)
            if year.is_none() && token.len() == 4 {
                if let Ok(y) = token.parse::<u32>() {
                    if (1900..=2099).contains(&y) {
                        year = Some(y);
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                }
            }

            // Check Resolution
            if resolution.is_none() {
                match lower.as_str() {
                    "2160p" | "4k" | "uhd" => {
                        resolution = Some("2160p".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "1080p" | "1080i" | "fhd" => {
                        resolution = Some("1080p".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "720p" | "hd" => {
                        resolution = Some("720p".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "576p" | "576i" => {
                        resolution = Some("576p".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "480p" | "480i" | "sd" => {
                        resolution = Some("480p".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    _ => {}
                }
            }

            // Check Source
            if source.is_none() {
                match lower.as_str() {
                    "remux" => {
                        source = Some("Remux".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "bluray" | "blu-ray" | "bdrip" | "brrip" => {
                        source = Some("BluRay".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "web-dl" | "webdl" => {
                        source = Some("WEB-DL".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "webrip" | "web-rip" => {
                        source = Some("WEBRip".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "hdtv" => {
                        source = Some("HDTV".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "dvd" | "dvdrip" | "dvd9" | "dvd5" => {
                        source = Some("DVD".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    _ => {}
                }
            }

            // Check Codec
            if codec.is_none() {
                match lower.as_str() {
                    "hevc" | "x265" | "h265" | "h 265" => {
                        codec = Some("x265".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "x264" | "h264" | "h 264" | "avc" => {
                        codec = Some("x264".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "av1" => {
                        codec = Some("AV1".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "vc-1" | "vc1" => {
                        codec = Some("VC-1".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "mpeg2" | "mpeg-2" => {
                        codec = Some("MPEG-2".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    _ => {}
                }
            }

            // Check Audio
            if audio.is_none() {
                match lower.as_str() {
                    "truehd" => {
                        audio = Some("TrueHD".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "atmos" => {
                        audio = Some("Atmos".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "dts-hd" | "dtshd" | "dts-ma" => {
                        audio = Some("DTS-HD MA".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "dts" => {
                        audio = Some("DTS".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "flac" => {
                        audio = Some("FLAC".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "lpcm" | "pcm" => {
                        audio = Some("LPCM".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "aac" | "aac2 0" => {
                        audio = Some("AAC".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "ac3" | "dd5 1" | "ddp" | "eac3" => {
                        audio = Some("AC3".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    _ => {}
                }
            }

            // Check Edition
            if edition.is_none() {
                match lower.as_str() {
                    "criterion" => {
                        edition = Some("Criterion".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "remastered" => {
                        edition = Some("Remastered".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "extended" => {
                        edition = Some("Extended".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    "uncut" => {
                        edition = Some("Uncut".to_string());
                        if first_meta_token_idx.is_none() {
                            first_meta_token_idx = Some(idx);
                        }
                        continue;
                    }
                    _ => {}
                }
            }
        }

        // 4. Derive clean title from tokens preceding first_meta_token_idx
        let clean_title = match first_meta_token_idx {
            Some(idx) if idx > 0 => raw_tokens[..idx].join(" "),
            Some(0) => raw_tokens[0].to_string(),
            _ => {
                if raw_tokens.is_empty() {
                    raw_name.to_string()
                } else {
                    raw_tokens.join(" ")
                }
            }
        };

        ParsedMediaMetadata {
            raw_title: raw_name.to_string(),
            clean_title,
            year,
            resolution,
            source,
            codec,
            audio,
            season,
            episode,
            edition,
            release_group,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_classic_asian_cinema_release() {
        let title = "Seven.Samurai.1954.Criterion.1080p.BluRay.x264.DTS-ADC";
        let parsed = MediaTitleParser::parse(title);

        assert_eq!(parsed.clean_title, "Seven Samurai");
        assert_eq!(parsed.year, Some(1954));
        assert_eq!(parsed.edition, Some("Criterion".to_string()));
        assert_eq!(parsed.resolution, Some("1080p".to_string()));
        assert_eq!(parsed.source, Some("BluRay".to_string()));
        assert_eq!(parsed.codec, Some("x264".to_string()));
        assert_eq!(parsed.audio, Some("DTS".to_string()));
        assert_eq!(parsed.release_group, Some("ADC".to_string()));
    }

    #[test]
    fn test_parse_anime_series_episode() {
        let title = "Attack.on.Titan.S04E28.1080p.WEB-DL.AAC.x264-SubsPlease.mkv";
        let parsed = MediaTitleParser::parse(title);

        assert_eq!(parsed.clean_title, "Attack on Titan");
        assert_eq!(parsed.season, Some(4));
        assert_eq!(parsed.episode, Some(28));
        assert_eq!(parsed.resolution, Some("1080p".to_string()));
        assert_eq!(parsed.source, Some("WEB-DL".to_string()));
        assert_eq!(parsed.codec, Some("x264".to_string()));
        assert_eq!(parsed.audio, Some("AAC".to_string()));
        assert_eq!(parsed.release_group, Some("SubsPlease".to_string()));
    }

    #[test]
    fn test_parse_4k_remux_release() {
        let title = "Drive.My.Car.2021.2160p.UHD.Remux.HEVC.TrueHD.Atmos-ADC";
        let parsed = MediaTitleParser::parse(title);

        assert_eq!(parsed.clean_title, "Drive My Car");
        assert_eq!(parsed.year, Some(2021));
        assert_eq!(parsed.resolution, Some("2160p".to_string()));
        assert_eq!(parsed.source, Some("Remux".to_string()));
        assert_eq!(parsed.codec, Some("x265".to_string()));
        assert_eq!(parsed.audio, Some("TrueHD".to_string()));
        assert_eq!(parsed.release_group, Some("ADC".to_string()));
    }

    #[test]
    fn test_parse_plain_folder_without_metadata() {
        let title = "Personal Home Videos Collection";
        let parsed = MediaTitleParser::parse(title);

        assert_eq!(parsed.clean_title, "Personal Home Videos Collection");
        assert_eq!(parsed.year, None);
        assert_eq!(parsed.resolution, None);
        assert_eq!(parsed.release_group, None);
    }
}
