//! Media thumbnail service: poster discovery + WebP cache.
//!
//! ADC releases almost always ship cover scans inside the torrent folder.
//! The largest/best-named image is downscaled and re-encoded as WebP into a
//! per-torrent cache entry so the media grid stays light (owner directive:
//! compressed WebP, small footprint). The cache key folds in the source
//! file's size+mtime so a replaced scan invalidates naturally.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use image::GenericImageView;

const THUMB_WIDTH: u32 = 360;

#[derive(Debug)]
pub enum ThumbError {
    NoCover,
    NotADirectory,
    CacheDir(String),
    Decode(String),
    Encode(String),
}

impl fmt::Display for ThumbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThumbError::NoCover => write!(f, "No cover image found in the release folder"),
            ThumbError::NotADirectory => write!(f, "Torrent path does not exist"),
            ThumbError::CacheDir(e) => write!(f, "Cache dir: {e}"),
            ThumbError::Decode(e) => write!(f, "Decode: {e}"),
            ThumbError::Encode(e) => write!(f, "WebP encode: {e}"),
        }
    }
}

impl std::error::Error for ThumbError {}

/// Candidate basenames ranked best-first (a real poster beats a random scan).
const PREFERRED_STEMS: &[&str] = &[
    "poster",
    "cover",
    "front",
    "folder",
    "disc",
    "dvd",
    "art",
    "scan",
    "thumb",
    "screenshot",
];

fn is_image(p: &Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp" | "bmp" | "tiff" | "tif")
    )
}

/// Collect image files directly inside `dir` (top level only — recursive
/// scans of a 300GB boxset tree would be wasteful).
fn list_images(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_image(p))
        .collect()
}

fn score_candidate(p: &Path) -> i64 {
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stem = p
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // A preferred stem dominates; among equals, bigger files win (higher-res
    // scans); .png/.jpg get a small edge over exotic formats.
    let stem_rank = PREFERRED_STEMS
        .iter()
        .position(|s| stem.contains(s))
        .map(|i| 1_000_000 - (i as i64) * 10_000)
        .unwrap_or(0);
    let size_bonus = (fs::metadata(p)
        .map(|m| m.len())
        .unwrap_or(0)
        .min(50_000_000)
        / 1000) as i64;
    let ext_bonus = if name.ends_with(".jpg") || name.ends_with(".jpeg") || name.ends_with(".png") {
        500
    } else {
        0
    };
    stem_rank + size_bonus + ext_bonus
}

/// Find the best poster image inside a torrent directory.
fn find_poster(torrent_dir: &Path) -> Option<PathBuf> {
    let mut candidates = list_images(torrent_dir);
    // One level down too: some releases nest covers in a "Scans"/"Covers" dir.
    let Ok(subs) = fs::read_dir(torrent_dir) else {
        return None;
    };
    for sub in subs.flatten().filter(|e| e.path().is_dir()).take(24) {
        candidates.extend(list_images(&sub.path()));
    }
    candidates.into_iter().max_by_key(|p| score_candidate(p))
}

/// WebP cache path for a source image, keyed by path+size+mtime.
fn cache_path(cache_dir: &Path, source: &Path) -> PathBuf {
    let meta = fs::metadata(source).ok();
    let key = format!(
        "{}|{}|{}",
        source.display(),
        meta.as_ref().map(|m| m.len()).unwrap_or(0),
        meta.as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    // Stable short digest (no external hash crate needed).
    let mut hash: u64 = 1469598103934665603;
    for b in key.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    cache_dir.join(format!("{hash:016x}.webp"))
}

/// Produce (or reuse) a WebP poster thumbnail for a single image file.
/// Does NOT search the parent directory — it converts the exact image file provided.
pub fn convert_single_image(thumbs_dir: &Path, source_file: &Path) -> Result<PathBuf, ThumbError> {
    if !source_file.is_file() {
        return Err(ThumbError::Decode(format!(
            "Not a file: {}",
            source_file.display()
        )));
    }
    fs::create_dir_all(thumbs_dir).map_err(|e| ThumbError::CacheDir(e.to_string()))?;
    let dest = cache_path(thumbs_dir, source_file);
    if dest.exists() {
        return Ok(dest);
    }

    let img = image::open(source_file)
        .map_err(|e| ThumbError::Decode(format!("{}: {e}", source_file.display())))?;
    let (w, _) = img.dimensions();
    let scaled = if w > THUMB_WIDTH {
        img.resize(THUMB_WIDTH, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    let rgb = scaled.to_rgb8();
    let webp = image::DynamicImage::ImageRgb8(rgb);
    webp.save_with_format(&dest, image::ImageFormat::WebP)
        .map_err(|e| ThumbError::Encode(e.to_string()))?;
    Ok(dest)
}

/// Produce (or reuse) a WebP poster thumbnail for a torrent directory.
/// Returns the cache file path on success. Accepts a file path too (a
/// single-file torrent) — its parent folder is searched.
pub fn ensure_thumbnail(app_cache_dir: &Path, torrent_path: &Path) -> Result<PathBuf, ThumbError> {
    let thumbs_dir = app_cache_dir.join("thumbnails");
    // If the path is already an image file (e.g. downloaded artwork), convert it directly
    // without scanning its parent directory.
    if torrent_path.is_file() && is_image(torrent_path) {
        return convert_single_image(&thumbs_dir, torrent_path);
    }

    let torrent_dir = if torrent_path.is_file() {
        torrent_path.parent().unwrap_or(torrent_path).to_path_buf()
    } else if torrent_path.is_dir() {
        torrent_path.to_path_buf()
    } else {
        return Err(ThumbError::NotADirectory);
    };
    let source = find_poster(&torrent_dir).ok_or(ThumbError::NoCover)?;

    fs::create_dir_all(&thumbs_dir).map_err(|e| ThumbError::CacheDir(e.to_string()))?;
    let dest = cache_path(&thumbs_dir, &source);
    if dest.exists() {
        return Ok(dest);
    }

    let img = image::open(&source)
        .map_err(|e| ThumbError::Decode(format!("{}: {e}", source.display())))?;
    let (w, _) = img.dimensions();
    let scaled = if w > THUMB_WIDTH {
        img.resize(THUMB_WIDTH, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    // The `image` crate's WebP encoder is lossless; at 360px wide a poster
    // scan lands in the tens-of-KB range — plenty light for the cache.
    let rgb = scaled.to_rgb8();
    let webp = image::DynamicImage::ImageRgb8(rgb);
    webp.save_with_format(&dest, image::ImageFormat::WebP)
        .map_err(|e| ThumbError::Encode(e.to_string()))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poster_discovery_prefers_named_covers() {
        let dir = std::env::temp_dir().join(format!("neb-thumbs-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let cover = dir.join("cover.jpg");
        let other = dir.join("random-xyz.png");
        fs::write(&cover, b"not-really-an-image").ok();
        fs::write(&other, b"not-really-an-image").ok();
        let picked = find_poster(&dir).expect("a candidate");
        assert_eq!(picked, cover);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_dir_is_an_error_not_a_panic() {
        assert!(ensure_thumbnail(Path::new("/nonexistent-ro"), Path::new("/nonexistent")).is_err());
    }

    #[test]
    fn convert_single_image_converts_target_without_parent_scan() {
        use image::{ImageBuffer, Rgb};

        let temp_dir =
            std::env::temp_dir().join(format!("neb_thumb_single_test_{}", std::process::id()));
        let source_dir = temp_dir.join("media_sources");
        let app_cache_dir = temp_dir.join("cache");
        let thumbs_dir = app_cache_dir.join("thumbnails");
        let _ = fs::create_dir_all(&source_dir);
        let _ = fs::create_dir_all(&thumbs_dir);

        let target_file = source_dir.join("shana_poster.png");
        let distractor_file = source_dir.join("fuuka_cover.png");

        // Target image is 16x16 red
        let target_img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(16, 16, Rgb([255, 0, 0]));
        target_img.save(&target_file).expect("save target");

        // Distractor image is 64x64 blue (larger, so find_poster would pick it if scanning parent)
        let distractor_img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(64, 64, Rgb([0, 0, 255]));
        distractor_img
            .save(&distractor_file)
            .expect("save distractor");

        let result = convert_single_image(&thumbs_dir, &target_file).expect("convert single image");
        assert!(result.exists(), "Thumbnail should exist");

        // Verify the thumb was created specifically from the target
        let expected_dest = cache_path(&thumbs_dir, &target_file);
        assert_eq!(result, expected_dest);

        // Open resulting thumb and verify it kept the 16x16 dimension and did not become 64x64 distractor
        let decoded = image::open(&result).expect("open decoded thumb");
        let (w, h) = decoded.dimensions();
        assert_eq!(w, 16);
        assert_eq!(h, 16);

        // Also test ensure_thumbnail with a single image file passes through to thumbs_dir
        let ensure_result =
            ensure_thumbnail(&app_cache_dir, &target_file).expect("ensure_thumbnail single file");
        assert_eq!(ensure_result, expected_dest);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
