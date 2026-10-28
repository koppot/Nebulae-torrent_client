//! Encoded black-bar detection and crop calculation.
//!
//! Follows VideoLAN LibVLC 3.0 crop geometry specifications and FFmpeg cropdetect
//! principles (normalized digital black limits, noise tolerance, and stable evidence).
//!
//! Two properties of the environment shape the design:
//!
//! * `libvlc_video_take_snapshot` captures the *cropped* picture once a crop is
//!   applied (`vout_snapshot_Set` is handed the display's cropped source format),
//!   so a snapshot taken after a crop never shows the bars again. A cropped frame
//!   without bars therefore says nothing about whether the crop is still wanted.
//! * Black-dominant frames (title cards, warning screens, credits, fades) are
//!   mostly black *picture*. Measuring the black border of such a frame yields the
//!   bounding box of the text or object, not encoded bars.
//!
//! So a frame is only accepted as evidence when it shows a picture that plausibly
//! fills the active area; everything else is an abstention (`Unknown`), which never
//! changes crop state. An applied crop is held until the title, menu or stream
//! changes (see `FramingState`), never released or tightened from later samples.

use std::path::Path;

/// Maximum luma value considered digital black in video frames.
///
/// In digital video (ITU-R BT.709 and BT.2020), limited range black is 16.
/// Decoded frames converted to sRGB / tone-mapped often contain compression noise,
/// dithering, and grain up to 24.
pub const BLACK_LUMA_LIMIT: u8 = 24;

/// Smallest bar worth cropping as a fraction of the frame dimension (0.4%).
///
/// DVD masters often carry 3-6 px side bars on a 720 px wide frame (0.4%-0.8%).
pub const MIN_BAR_FRACTION: f64 = 0.004;

/// Largest bar accepted per edge as a fraction of the frame dimension (35%).
pub const MAX_BAR_FRACTION: f64 = 0.35;

/// Consecutive matching observations required to confirm a crop geometry.
pub const DEFAULT_CONFIRMATIONS: u32 = 2;

/// Luma at or above which a pixel counts as visible picture content.
pub const LIT_LUMA: u8 = 40;

/// Share of the active area that must be visible picture content before a frame is
/// trusted as evidence. Below this the frame is black-dominant (or too dark to
/// tell bars from picture) and the detector abstains.
pub const MIN_LIT_FRACTION: f64 = 0.35;

/// Smallest share of the frame the active picture may occupy. 2.76:1 on a 16:9
/// frame leaves 64%; anything below half is the bounding box of something else.
pub const MIN_ACTIVE_AREA: f64 = 0.5;

/// Bars at or above this fraction of an axis are "large": they must be roughly
/// symmetric, and may not appear on both axes at once (letterbox XOR pillarbox).
const LARGE_BAR_FRACTION: f64 = 0.03;

/// Measured black borders around the active picture (in source-video pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bars {
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub sar: f64,
}

impl Bars {
    /// Format as a LibVLC 3.0 border-crop geometry string:
    /// `"<left>+<top>+<right>+<bottom>"`
    pub fn crop_spec(&self) -> String {
        format!("{}+{}+{}+{}", self.left, self.top, self.right, self.bottom)
    }

    pub fn active_width(&self) -> u32 {
        self.frame_width.saturating_sub(self.left + self.right)
    }

    pub fn active_height(&self) -> u32 {
        self.frame_height.saturating_sub(self.top + self.bottom)
    }

    pub fn active_aspect(&self) -> f64 {
        let w = self.active_width();
        let h = self.active_height();
        if h == 0 {
            0.0
        } else {
            (w as f64 / h as f64) * self.sar
        }
    }

    /// Whether another measurement matches within sampling tolerance.
    pub fn close_to(&self, other: &Bars) -> bool {
        self.frame_width == other.frame_width
            && self.frame_height == other.frame_height
            && edge_close(self.top, other.top)
            && edge_close(self.bottom, other.bottom)
            && edge_close(self.left, other.left)
            && edge_close(self.right, other.right)
            && (self.sar - other.sar).abs() < 0.01
    }

    /// The per-edge minimum of two measurements: never crops more than either saw.
    fn least_aggressive(&self, other: &Bars) -> Bars {
        Bars {
            top: self.top.min(other.top),
            bottom: self.bottom.min(other.bottom),
            left: self.left.min(other.left),
            right: self.right.min(other.right),
            ..*self
        }
    }

    /// Whether this crop was measured on a stream of a different coded size.
    pub fn stream_changed(&self, width: u32, height: u32) -> bool {
        width > 0 && height > 0 && (width != self.frame_width || height != self.frame_height)
    }
}

/// Two measurements of one edge agree when they differ by at most 8 source pixels,
/// or by at most half of the larger bar when it is narrow (a 4 px bar and no bar at
/// all are different geometries).
fn edge_close(a: u32, b: u32) -> bool {
    a.abs_diff(b) <= (a.max(b) / 2).clamp(2, 8)
}

/// The result of inspecting a sampled frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarObservation {
    /// Frame has measurable black bars on one or more borders.
    Bars(Bars),
    /// Frame is picture edge to edge with no encoded bars exceeding the minimum threshold.
    NoBars,
    /// No trustworthy reading: black edge to edge, black-dominant (title card, fade,
    /// credits), too dark to separate bars from picture, or implausible geometry.
    Unknown,
}

/// Actions produced by the crop state tracker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CropAction {
    /// Apply this new confirmed crop geometry.
    Set(Bars),
    /// The observation agrees with the current state (applied crop or confirmed full frame).
    Stable,
    /// Not enough consecutive evidence yet to take action.
    None,
}

/// Calculates perceptual luma using ITU-R BT.709 coefficients.
#[inline]
fn rgb_luma(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 54 + g as u32 * 183 + b as u32 * 19) >> 8) as u8
}

/// Black borders measured on the snapshot itself, in snapshot pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SampleBars {
    top: usize,
    bottom: usize,
    left: usize,
    right: usize,
}

/// Scans inward from each edge for rows and columns that are black.
///
/// `None` when the frame is black from edge to edge.
fn scan_black_borders(img: &image::RgbImage) -> Option<SampleBars> {
    let (w, h) = (img.width(), img.height());

    // Skip the outer 5% on each side to avoid edge anti-aliasing / window border artifacts.
    let x_eval_start = (w as f64 * 0.05).round() as u32;
    let x_eval_end = (w as f64 * 0.95).round() as u32;
    let eval_width = x_eval_end.saturating_sub(x_eval_start).max(1);

    let is_row_black = |y: u32| -> bool {
        let mut sum_luma = 0u64;
        let mut high_count = 0usize;
        for x in x_eval_start..x_eval_end {
            let px = img.get_pixel(x, y);
            let l = rgb_luma(px[0], px[1], px[2]);
            sum_luma += l as u64;
            // Count pixels exceeding 125% of black limit (e.g. > 30)
            if l > (BLACK_LUMA_LIMIT as u32 * 5 / 4) as u8 {
                high_count += 1;
            }
        }
        let mean = sum_luma / (eval_width as u64);
        let high_ratio = high_count as f64 / (eval_width as f64);
        mean <= (BLACK_LUMA_LIMIT as u64) && high_ratio <= 0.05
    };

    let mut top = 0usize;
    while top < h as usize && is_row_black(top as u32) {
        top += 1;
    }

    let mut bottom = 0usize;
    while bottom < h as usize && is_row_black((h as usize - 1 - bottom) as u32) {
        bottom += 1;
    }

    // If frame is black edge-to-edge vertically, it is a fade or blank frame.
    if top + bottom >= h as usize {
        return None;
    }

    // Scan vertical columns within active picture rows
    let y_eval_start = (top as u32).max((h as f64 * 0.05).round() as u32);
    let y_eval_end = ((h as usize - bottom) as u32).min((h as f64 * 0.95).round() as u32);
    let eval_height = y_eval_end.saturating_sub(y_eval_start).max(1);

    let is_col_black = |x: u32| -> bool {
        let mut sum_luma = 0u64;
        let mut high_count = 0usize;
        for y in y_eval_start..y_eval_end {
            let px = img.get_pixel(x, y);
            let l = rgb_luma(px[0], px[1], px[2]);
            sum_luma += l as u64;
            if l > (BLACK_LUMA_LIMIT as u32 * 5 / 4) as u8 {
                high_count += 1;
            }
        }
        let mean = sum_luma / (eval_height as u64);
        let high_ratio = high_count as f64 / (eval_height as f64);
        mean <= (BLACK_LUMA_LIMIT as u64) && high_ratio <= 0.05
    };

    let mut left = 0usize;
    while left < w as usize && is_col_black(left as u32) {
        left += 1;
    }

    let mut right = 0usize;
    while right < w as usize && is_col_black((w as usize - 1 - right) as u32) {
        right += 1;
    }

    if left + right >= w as usize {
        return None;
    }

    Some(SampleBars {
        top,
        bottom,
        left,
        right,
    })
}

/// Share of the pixels inside the measured active rectangle that are visible picture.
fn lit_fraction(img: &image::RgbImage, bars: &SampleBars) -> f64 {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let (x0, x1) = (bars.left, w - bars.right);
    let (y0, y1) = (bars.top, h - bars.bottom);
    let total = (x1 - x0) * (y1 - y0);
    if total == 0 {
        return 0.0;
    }
    let mut lit = 0usize;
    for y in y0..y1 {
        for x in x0..x1 {
            let px = img.get_pixel(x as u32, y as u32);
            if rgb_luma(px[0], px[1], px[2]) >= LIT_LUMA {
                lit += 1;
            }
        }
    }
    lit as f64 / total as f64
}

/// Large bars on one axis must be roughly symmetric, as encoder-added bars are.
fn bars_balanced(a: usize, b: usize, dim: usize) -> bool {
    let larger = a.max(b);
    if (larger as f64 / dim as f64) < LARGE_BAR_FRACTION {
        return true;
    }
    a.abs_diff(b) <= (larger / 4).max(4)
}

/// Whether the measured borders describe encoded bars around a real picture rather
/// than the bounding box of text, sparse objects, or a dark scene.
fn is_trustworthy(img: &image::RgbImage, bars: &SampleBars) -> bool {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let frac = |px: usize, dim: usize| px as f64 / dim as f64;

    let active = (w - bars.left - bars.right) * (h - bars.top - bars.bottom);
    if (active as f64 / (w * h) as f64) < MIN_ACTIVE_AREA {
        return false;
    }

    let oversized = frac(bars.top, h) > MAX_BAR_FRACTION
        || frac(bars.bottom, h) > MAX_BAR_FRACTION
        || frac(bars.left, w) > MAX_BAR_FRACTION
        || frac(bars.right, w) > MAX_BAR_FRACTION;
    if oversized {
        return false;
    }

    // Encoded content is letterboxed or pillarboxed. Borders on both axes outline a
    // bright box on black, which is what a title card or logo looks like.
    let both_axes = frac(bars.top + bars.bottom, h) > LARGE_BAR_FRACTION
        && frac(bars.left + bars.right, w) > LARGE_BAR_FRACTION;
    if both_axes {
        return false;
    }

    if !bars_balanced(bars.top, bars.bottom, h) || !bars_balanced(bars.left, bars.right, w) {
        return false;
    }

    lit_fraction(img, bars) >= MIN_LIT_FRACTION
}

/// Inspects a decoded RGB image buffer and measures encoded black bars.
pub fn observe_image(
    img: &image::RgbImage,
    frame_width: u32,
    frame_height: u32,
    sar: Option<f64>,
) -> BarObservation {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 || frame_width == 0 || frame_height == 0 {
        return BarObservation::Unknown;
    }
    let sar_val = sar.unwrap_or(1.0);

    let Some(sample) = scan_black_borders(img) else {
        return BarObservation::Unknown;
    };
    if !is_trustworthy(img, &sample) {
        return BarObservation::Unknown;
    }

    // Map sample coordinates to full source dimensions, apply the size thresholds
    // and round to even pixel counts (YUV alignment).
    let to_source = |sample_px: usize, sample_dim: u32, frame_dim: u32| -> u32 {
        let px = ((sample_px as f64 / sample_dim as f64) * frame_dim as f64).round() as u32;
        let fraction = px as f64 / frame_dim as f64;
        if (MIN_BAR_FRACTION..=MAX_BAR_FRACTION).contains(&fraction) {
            (px / 2) * 2
        } else {
            0
        }
    };

    let top = to_source(sample.top, h, frame_height);
    let bottom = to_source(sample.bottom, h, frame_height);
    let mut left = to_source(sample.left, w, frame_width);
    let mut right = to_source(sample.right, w, frame_width);

    let active_w = frame_width.saturating_sub(left + right);
    let active_h = frame_height.saturating_sub(top + bottom);
    let displayed_aspect = if active_h > 0 {
        (active_w as f64 / active_h as f64) * sar_val
    } else {
        0.0
    };

    // User rule: Preserve the full intended 4:3 picture and its normal side pillarboxing.
    // If the active picture has an intended displayed aspect ratio of ~4:3 (1.25 to 1.45),
    // do NOT crop the vertical left and right bars.
    if (1.25..=1.45).contains(&displayed_aspect) {
        left = 0;
        right = 0;
    }

    if top == 0 && bottom == 0 && left == 0 && right == 0 {
        BarObservation::NoBars
    } else {
        BarObservation::Bars(Bars {
            top,
            bottom,
            left,
            right,
            frame_width,
            frame_height,
            sar: sar_val,
        })
    }
}

/// Reads a PNG file written by `libvlc_video_take_snapshot` and returns its observation.
pub fn observe_png_file<P: AsRef<Path>>(
    path: P,
    frame_width: u32,
    frame_height: u32,
    sar: Option<f64>,
) -> Option<BarObservation> {
    let img = image::open(path).ok()?.to_rgb8();
    Some(observe_image(&img, frame_width, frame_height, sar))
}

/// The unconfirmed evidence the tracker is currently accumulating.
#[derive(Debug, Clone, Copy)]
enum Streak {
    NoBars(u32),
    Bars(Bars, u32),
}

/// Tracks observations over time and requires consecutive confirmations
/// before applying a crop (FFmpeg reset_count=0 stability principle).
///
/// Once a crop is applied it is held. Snapshots of a cropped picture cannot show
/// the bars again, so "no bars" is the expected reading and never a reason to clear;
/// differing bars inside an already cropped picture are never used to tighten it.
/// Releasing a crop is the caller's job, on a title, menu, stream or media change.
#[derive(Debug, Clone)]
pub struct CropTracker {
    streak: Option<Streak>,
    confirmations_required: u32,
    applied: Option<Bars>,
}

impl CropTracker {
    pub fn new(confirmations_required: u32) -> Self {
        Self {
            streak: None,
            confirmations_required,
            applied: None,
        }
    }

    pub fn reset(&mut self) {
        self.streak = None;
        self.applied = None;
    }

    pub fn applied(&self) -> Option<Bars> {
        self.applied
    }

    pub fn observe(&mut self, obs: BarObservation) -> CropAction {
        if let Some(applied) = self.applied {
            return match obs {
                BarObservation::NoBars => CropAction::Stable,
                BarObservation::Bars(seen) if applied.close_to(&seen) => CropAction::Stable,
                BarObservation::Bars(_) | BarObservation::Unknown => CropAction::None,
            };
        }

        match obs {
            BarObservation::Unknown => CropAction::None,
            BarObservation::NoBars => {
                let count = match self.streak {
                    Some(Streak::NoBars(n)) => n.saturating_add(1),
                    _ => 1,
                };
                self.streak = Some(Streak::NoBars(count));
                if count >= self.confirmations_required {
                    CropAction::Stable
                } else {
                    CropAction::None
                }
            }
            BarObservation::Bars(seen) => {
                let (merged, count) = match self.streak {
                    Some(Streak::Bars(prev, n)) if prev.close_to(&seen) => {
                        (prev.least_aggressive(&seen), n.saturating_add(1))
                    }
                    _ => (seen, 1),
                };
                if count >= self.confirmations_required {
                    self.streak = None;
                    self.applied = Some(merged);
                    CropAction::Set(merged)
                } else {
                    self.streak = Some(Streak::Bars(merged, count));
                    CropAction::None
                }
            }
        }
    }
}

impl Default for CropTracker {
    fn default() -> Self {
        Self::new(DEFAULT_CONFIRMATIONS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{imageops, Rgb, RgbImage};

    // Snapshot geometry the app requests: 960 px wide, proportional height.
    const SNAP_W: u32 = 960;
    const SNAP_H: u32 = 540;

    // Source geometries used by the scenarios.
    const HD: (u32, u32) = (1920, 1080);
    const UHD: (u32, u32) = (3840, 2160);
    const DVD: (u32, u32) = (720, 480);
    const DVD_16_9_SAR: f64 = 32.0 / 27.0;

    /// Picture content with enough texture and brightness to count as a real image.
    fn picture_px(x: u32, y: u32) -> Rgb<u8> {
        let v = 70 + ((x * 7 + y * 13) % 120) as u8;
        Rgb([v, v, v])
    }

    fn black() -> Rgb<u8> {
        Rgb([0, 0, 0])
    }

    /// A full frame of picture surrounded by black bars of the given sizes.
    fn framed(w: u32, h: u32, top: u32, bottom: u32, left: u32, right: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            if y < top || y >= h - bottom || x < left || x >= w - right {
                black()
            } else {
                picture_px(x, y)
            }
        })
    }

    fn all_black(w: u32, h: u32) -> RgbImage {
        RgbImage::from_pixel(w, h, black())
    }

    /// Text-like strokes: sparse bright pixels inside the block, black elsewhere.
    fn paint_text(img: &mut RgbImage, x0: u32, y0: u32, w: u32, h: u32) {
        for y in y0..(y0 + h).min(img.height()) {
            for x in x0..(x0 + w).min(img.width()) {
                if x % 6 < 3 && y % 14 < 9 {
                    img.put_pixel(x, y, Rgb([235, 235, 235]));
                }
            }
        }
    }

    fn paint_rect(img: &mut RgbImage, x0: u32, y0: u32, w: u32, h: u32, v: u8) {
        for y in y0..(y0 + h).min(img.height()) {
            for x in x0..(x0 + w).min(img.width()) {
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
    }

    fn centered_text_on_black() -> RgbImage {
        let mut f = all_black(SNAP_W, SNAP_H);
        paint_text(&mut f, 300, 250, 360, 40);
        f
    }

    fn credits_near_edges() -> RgbImage {
        let mut f = all_black(SNAP_W, SNAP_H);
        paint_text(&mut f, 60, 40, 220, 440);
        paint_text(&mut f, 700, 60, 200, 420);
        f
    }

    fn sparse_objects_on_black() -> RgbImage {
        let mut f = all_black(SNAP_W, SNAP_H);
        for &(x, y) in &[
            (120, 90),
            (800, 120),
            (460, 300),
            (200, 420),
            (880, 470),
            (640, 60),
        ] {
            paint_rect(&mut f, x, y, 14, 14, 255);
        }
        f
    }

    fn scaled(img: &RgbImage, level: f64) -> RgbImage {
        RgbImage::from_fn(img.width(), img.height(), |x, y| {
            let p = img.get_pixel(x, y);
            Rgb([
                (p[0] as f64 * level) as u8,
                (p[1] as f64 * level) as u8,
                (p[2] as f64 * level) as u8,
            ])
        })
    }

    fn observe(img: &RgbImage, src: (u32, u32), sar: f64) -> BarObservation {
        observe_image(img, src.0, src.1, Some(sar))
    }

    /// Drives the tracker the way the player does: one snapshot per sample, taken
    /// from what LibVLC would show with the current crop applied.
    struct Session {
        tracker: CropTracker,
        src: (u32, u32),
        sar: f64,
        actions: Vec<CropAction>,
    }

    impl Session {
        fn new(src: (u32, u32), sar: f64) -> Self {
            Self {
                tracker: CropTracker::default(),
                src,
                sar,
                actions: Vec::new(),
            }
        }

        /// LibVLC snapshots show only the cropped region, stretched to the request.
        fn snapshot(&self, screen: &RgbImage) -> RgbImage {
            let Some(b) = self.tracker.applied() else {
                return screen.clone();
            };
            let sx = screen.width() as f64 / self.src.0 as f64;
            let sy = screen.height() as f64 / self.src.1 as f64;
            let x = (b.left as f64 * sx).round() as u32;
            let y = (b.top as f64 * sy).round() as u32;
            let w = (b.active_width() as f64 * sx).round() as u32;
            let h = (b.active_height() as f64 * sy).round() as u32;
            let cropped = imageops::crop_imm(screen, x, y, w, h).to_image();
            imageops::resize(
                &cropped,
                screen.width(),
                screen.height(),
                imageops::FilterType::Triangle,
            )
        }

        /// Same, for a LibVLC build whose snapshots are not crop-aware.
        fn sample_uncropped(&mut self, screen: &RgbImage) -> CropAction {
            let obs = observe(screen, self.src, self.sar);
            let action = self.tracker.observe(obs);
            self.actions.push(action);
            action
        }

        fn sample(&mut self, screen: &RgbImage) -> CropAction {
            let snap = self.snapshot(screen);
            let obs = observe(&snap, self.src, self.sar);
            let action = self.tracker.observe(obs);
            self.actions.push(action);
            action
        }

        fn samples(&mut self, screen: &RgbImage, n: usize) {
            for _ in 0..n {
                self.sample(screen);
            }
        }

        fn sets(&self) -> usize {
            self.actions
                .iter()
                .filter(|a| matches!(a, CropAction::Set(_)))
                .count()
        }
    }

    fn letterboxed_uhd() -> RgbImage {
        // Mary and the Witch's Flower: 44/40 of 2160 -> 11/10 of 540.
        framed(SNAP_W, SNAP_H, 11, 10, 0, 0)
    }

    // ---- Detector: genuine bars are measured -------------------------------------------

    #[test]
    fn genuine_letterbox_is_measured() {
        let obs = observe(&letterboxed_uhd(), UHD, 1.0);
        let BarObservation::Bars(b) = obs else {
            panic!("expected bars, got {obs:?}");
        };
        assert_eq!((b.top, b.bottom, b.left, b.right), (44, 40, 0, 0));
        assert_eq!(b.crop_spec(), "0+44+0+40");
    }

    #[test]
    fn wide_scope_letterbox_is_measured() {
        // 2.39:1 inside 16:9: 69 of 540 rows each side.
        let obs = observe(&framed(SNAP_W, SNAP_H, 69, 69, 0, 0), HD, 1.0);
        let BarObservation::Bars(b) = obs else {
            panic!("expected bars, got {obs:?}");
        };
        assert_eq!((b.top, b.bottom), (138, 138));
        assert_eq!(b.crop_spec(), "0+138+0+138");
    }

    #[test]
    fn narrow_dvd_pillarbox_is_measured() {
        // Banner of Stars: 5 px left, 3 px right on a 720x480 anamorphic frame.
        let obs = observe(&framed(720, 480, 0, 0, 5, 3), DVD, DVD_16_9_SAR);
        let BarObservation::Bars(b) = obs else {
            panic!("expected bars, got {obs:?}");
        };
        assert_eq!((b.left, b.right, b.top, b.bottom), (4, 2, 0, 0));
        assert_eq!(b.crop_spec(), "4+0+2+0");
        assert!(b.active_aspect() > 1.45, "stays 16:9, nothing is stretched");
    }

    #[test]
    fn slivers_below_the_minimum_are_not_bars() {
        assert_eq!(
            observe(&framed(720, 480, 0, 0, 2, 1), DVD, DVD_16_9_SAR),
            BarObservation::NoBars
        );
    }

    #[test]
    fn full_frame_picture_has_no_bars() {
        assert_eq!(
            observe(&framed(SNAP_W, SNAP_H, 0, 0, 0, 0), HD, 1.0),
            BarObservation::NoBars
        );
    }

    #[test]
    fn large_4_3_pillarbox_keeps_its_side_bars() {
        // 4:3 picture inside a 16:9 frame: the user rule keeps the pillarbox.
        let frame = framed(SNAP_W, SNAP_H, 0, 0, 120, 120);
        assert_eq!(observe(&frame, HD, 1.0), BarObservation::NoBars);
    }

    #[test]
    fn dvd_4_3_with_side_slivers_stays_uncropped() {
        let frame = framed(720, 480, 0, 0, 5, 3);
        assert_eq!(observe(&frame, DVD, 8.0 / 9.0), BarObservation::NoBars);
    }

    // ---- F: black-dominant content is never read as bars -------------------------------

    #[test]
    fn centered_text_on_black_is_not_a_crop() {
        let frame = centered_text_on_black();
        let naive = scan_black_borders(&frame).expect("text frame is not fully black");
        assert!(
            naive.top > 200 && naive.left > 250,
            "a bare border scan would crop tightly around the text: {naive:?}"
        );
        assert_eq!(observe(&frame, HD, 1.0), BarObservation::Unknown);
        assert_eq!(observe(&frame, UHD, 1.0), BarObservation::Unknown);
    }

    #[test]
    fn credits_and_text_near_the_edges_are_not_a_crop() {
        let frame = credits_near_edges();
        assert_eq!(observe(&frame, HD, 1.0), BarObservation::Unknown);
    }

    #[test]
    fn sparse_bright_objects_on_black_are_not_a_crop() {
        let frame = sparse_objects_on_black();
        assert_eq!(observe(&frame, HD, 1.0), BarObservation::Unknown);
    }

    #[test]
    fn bright_box_centered_on_black_is_not_a_crop() {
        // The 426x280 box that used to be applied on the Banner DVD title cards.
        let mut f = all_black(720, 480);
        for y in 96..376 {
            for x in 144..570 {
                f.put_pixel(x, y, picture_px(x, y));
            }
        }
        assert_eq!(observe(&f, DVD, DVD_16_9_SAR), BarObservation::Unknown);
    }

    #[test]
    fn fully_black_frames_abstain() {
        assert_eq!(
            observe(&all_black(SNAP_W, SNAP_H), HD, 1.0),
            BarObservation::Unknown
        );
    }

    #[test]
    fn fades_never_produce_other_bars() {
        let genuine = framed(SNAP_W, SNAP_H, 11, 10, 0, 0);
        for step in 0..=20 {
            let level = 1.0 - step as f64 / 20.0;
            let frame = scaled(&genuine, level);
            match observe(&frame, UHD, 1.0) {
                BarObservation::Unknown => {}
                BarObservation::Bars(b) => assert_eq!(
                    (b.top, b.bottom, b.left, b.right),
                    (44, 40, 0, 0),
                    "fade level {level:.2} measured different bars"
                ),
                BarObservation::NoBars => panic!("fade level {level:.2} lost the bars"),
            }
        }
        // Full-frame picture fading out never turns into a crop either.
        let full = framed(SNAP_W, SNAP_H, 0, 0, 0, 0);
        for step in 0..=20 {
            let level = 1.0 - step as f64 / 20.0;
            let obs = observe(&scaled(&full, level), HD, 1.0);
            assert!(
                !matches!(obs, BarObservation::Bars(_)),
                "full-frame fade level {level:.2} produced {obs:?}"
            );
        }
    }

    #[test]
    fn grainy_dark_picture_is_not_cropped() {
        // Dim picture that reaches every edge: low luma everywhere, grain on top.
        let dark = RgbImage::from_fn(SNAP_W, SNAP_H, |x, y| {
            let v = 8 + ((x * 5 + y * 11) % 38) as u8;
            Rgb([v, v, v])
        });
        assert!(!matches!(observe(&dark, HD, 1.0), BarObservation::Bars(_)));
    }

    #[test]
    fn black_sky_above_a_lit_scene_is_not_a_crop() {
        // Dark scene without encoded bars: the top 18% is black sky, the rest is lit.
        let mut f = framed(SNAP_W, SNAP_H, 97, 0, 0, 0);
        let naive = scan_black_borders(&f).expect("lit scene");
        assert_eq!((naive.top, naive.bottom), (97, 0));
        assert_eq!(observe(&f, HD, 1.0), BarObservation::Unknown);

        // The same scene once the sky has some stars in it is still not letterboxing.
        paint_rect(&mut f, 400, 20, 3, 3, 250);
        assert_eq!(observe(&f, HD, 1.0), BarObservation::Unknown);
    }

    #[test]
    fn dark_scene_with_a_lit_centre_is_not_cropped() {
        // Night scene: only a central band is lit, the rest is black.
        let mut f = all_black(SNAP_W, SNAP_H);
        for y in 160..380 {
            for x in 0..SNAP_W {
                f.put_pixel(x, y, picture_px(x, y));
            }
        }
        let naive = scan_black_borders(&f).expect("lit band");
        assert_eq!((naive.top, naive.bottom), (160, 160));
        assert_eq!(observe(&f, HD, 1.0), BarObservation::Unknown);
    }

    #[test]
    fn one_sided_large_border_is_not_a_crop() {
        // Left third black (credits column placed right): not encoder bars.
        let frame = framed(SNAP_W, SNAP_H, 0, 0, 280, 0);
        assert_eq!(observe(&frame, HD, 1.0), BarObservation::Unknown);
    }

    #[test]
    fn bars_on_both_axes_are_not_a_crop() {
        let frame = framed(SNAP_W, SNAP_H, 60, 60, 120, 120);
        assert_eq!(observe(&frame, HD, 1.0), BarObservation::Unknown);
    }

    #[test]
    fn oversized_bars_are_not_clipped_to_the_maximum() {
        // A 40% left border used to be silently dropped while the right one was kept.
        let frame = framed(SNAP_W, SNAP_H, 0, 0, 384, 300);
        assert_eq!(observe(&frame, HD, 1.0), BarObservation::Unknown);
    }

    #[test]
    fn black_dominant_frame_touching_every_edge_is_not_full_frame_evidence() {
        // Mostly black but with a bright line along every edge: no bars, but also
        // not a picture, so it must not count towards "full frame confirmed".
        let mut f = all_black(SNAP_W, SNAP_H);
        paint_rect(&mut f, 0, 0, SNAP_W, 4, 200);
        paint_rect(&mut f, 0, SNAP_H - 4, SNAP_W, 4, 200);
        paint_rect(&mut f, 0, 0, 4, SNAP_H, 200);
        paint_rect(&mut f, SNAP_W - 4, 0, 4, SNAP_H, 200);
        assert_eq!(observe(&f, HD, 1.0), BarObservation::Unknown);
    }

    // ---- Tracker: applied crop behaviour ---------------------------------------------

    #[test]
    fn crop_is_applied_after_two_matching_samples() {
        let mut s = Session::new(UHD, 1.0);
        let screen = letterboxed_uhd();
        assert_eq!(s.sample(&screen), CropAction::None);
        let CropAction::Set(b) = s.sample(&screen) else {
            panic!("second matching sample must apply the crop");
        };
        assert_eq!(b.crop_spec(), "0+44+0+40");
        assert_eq!(s.tracker.applied(), Some(b));
    }

    #[test]
    fn confirmed_crop_is_the_least_aggressive_of_its_samples() {
        let mut t = CropTracker::default();
        let mk = |top, bottom| Bars {
            top,
            bottom,
            left: 0,
            right: 0,
            frame_width: 3840,
            frame_height: 2160,
            sar: 1.0,
        };
        assert_eq!(
            t.observe(BarObservation::Bars(mk(44, 40))),
            CropAction::None
        );
        assert_eq!(
            t.observe(BarObservation::Bars(mk(40, 40))),
            CropAction::Set(mk(40, 40))
        );
    }

    #[test]
    fn cropped_snapshots_never_clear_the_crop() {
        // The regression: after the crop is applied the snapshot no longer shows the
        // bars, and "no bars" used to clear the crop every ~4 s.
        let mut s = Session::new(UHD, 1.0);
        let screen = letterboxed_uhd();
        s.samples(&screen, 2);
        let applied = s.tracker.applied().expect("crop applied");
        let cropped = s.snapshot(&screen);
        assert_eq!(
            observe(&cropped, UHD, 1.0),
            BarObservation::NoBars,
            "the cropped snapshot shows no bars"
        );

        s.samples(&screen, 40);
        assert_eq!(s.sets(), 1, "the crop is applied exactly once");
        assert_eq!(s.tracker.applied(), Some(applied), "and never changes");
        assert!(s.actions[2..].iter().all(|a| *a == CropAction::Stable));
    }

    #[test]
    fn snapshots_that_still_show_the_bars_keep_the_crop_too() {
        let mut s = Session::new(UHD, 1.0);
        let screen = letterboxed_uhd();
        s.sample_uncropped(&screen);
        s.sample_uncropped(&screen);
        let applied = s.tracker.applied().expect("crop applied");
        for _ in 0..40 {
            assert_eq!(s.sample_uncropped(&screen), CropAction::Stable);
        }
        assert_eq!(s.tracker.applied(), Some(applied));
        assert_eq!(s.sets(), 1);
    }

    #[test]
    fn cropped_scene_then_black_title_card_keeps_the_crop() {
        let mut s = Session::new(UHD, 1.0);
        let scene = letterboxed_uhd();
        s.samples(&scene, 3);
        let applied = s.tracker.applied().expect("crop applied");

        // Title card: the screen is mostly black with centered text. What LibVLC
        // snapshots is the cropped region of it.
        let mut card = all_black(SNAP_W, SNAP_H);
        paint_text(&mut card, 300, 250, 360, 40);
        s.samples(&card, 20);
        assert_eq!(
            s.tracker.applied(),
            Some(applied),
            "no tightening, no clearing"
        );

        // And back to the scene: still the same single crop.
        s.samples(&scene, 5);
        assert_eq!(s.tracker.applied(), Some(applied));
        assert_eq!(s.sets(), 1);
    }

    #[test]
    fn credits_over_an_applied_crop_never_tighten_it() {
        let mut s = Session::new(UHD, 1.0);
        let scene = letterboxed_uhd();
        s.samples(&scene, 2);
        let applied = s.tracker.applied().expect("crop applied");

        s.samples(&credits_near_edges(), 10);
        s.samples(&sparse_objects_on_black(), 10);
        s.samples(&centered_text_on_black(), 10);
        s.samples(&all_black(SNAP_W, SNAP_H), 10);
        assert_eq!(s.tracker.applied(), Some(applied));
        assert_eq!(s.sets(), 1);
    }

    #[test]
    fn black_dominant_frames_never_establish_a_crop() {
        let mut s = Session::new(HD, 1.0);
        for frame in [
            centered_text_on_black(),
            credits_near_edges(),
            sparse_objects_on_black(),
            all_black(SNAP_W, SNAP_H),
        ] {
            s.samples(&frame, 12);
        }
        assert_eq!(s.tracker.applied(), None);
        assert_eq!(s.sets(), 0);
    }

    #[test]
    fn black_dominant_frames_do_not_settle_the_full_frame_decision() {
        // Ambiguous frames must not count as "full frame confirmed": the letterboxed
        // feature that follows still has to be cropped.
        let mut s = Session::new(UHD, 1.0);
        s.samples(&centered_text_on_black(), 10);
        assert!(s.actions.iter().all(|a| *a == CropAction::None));
        s.samples(&letterboxed_uhd(), 2);
        assert!(s.tracker.applied().is_some());
    }

    #[test]
    fn ambiguous_frames_between_confirmations_do_not_oscillate() {
        let mut s = Session::new(UHD, 1.0);
        let scene = letterboxed_uhd();
        let dark = centered_text_on_black();
        for _ in 0..10 {
            s.sample(&scene);
            s.sample(&dark);
            s.sample(&dark);
        }
        assert_eq!(s.sets(), 1, "one crop, never re-applied or cleared");
        assert!(s.tracker.applied().is_some());
    }

    #[test]
    fn repeated_ambiguous_frames_alone_change_nothing() {
        let mut s = Session::new(UHD, 1.0);
        for _ in 0..30 {
            s.sample(&credits_near_edges());
            s.sample(&all_black(SNAP_W, SNAP_H));
        }
        assert_eq!(s.tracker.applied(), None);
        assert!(s.actions.iter().all(|a| *a == CropAction::None));
    }

    #[test]
    fn narrow_dvd_pillarbox_is_applied_and_held() {
        let mut s = Session::new(DVD, DVD_16_9_SAR);
        let screen = framed(720, 480, 0, 0, 5, 3);
        s.samples(&screen, 2);
        let applied = s.tracker.applied().expect("crop applied");
        assert_eq!(applied.crop_spec(), "4+0+2+0");
        s.samples(&screen, 30);
        assert_eq!(s.tracker.applied(), Some(applied));
        assert_eq!(s.sets(), 1);
    }

    #[test]
    fn dvd_title_card_does_not_crop_before_the_feature() {
        // Banner of Stars: a 12 s card, then the feature with narrow side bars.
        let mut s = Session::new(DVD, DVD_16_9_SAR);
        let mut card = all_black(720, 480);
        paint_text(&mut card, 200, 220, 320, 40);
        s.samples(&card, 8);
        assert_eq!(s.tracker.applied(), None);
        s.samples(&framed(720, 480, 0, 0, 5, 3), 2);
        assert_eq!(
            s.tracker.applied().map(|b| b.crop_spec()),
            Some("4+0+2+0".into())
        );
    }

    #[test]
    fn media_change_starts_from_a_clean_slate() {
        let mut s = Session::new(UHD, 1.0);
        s.samples(&letterboxed_uhd(), 2);
        assert!(s.tracker.applied().is_some());

        s.tracker.reset();
        assert_eq!(s.tracker.applied(), None);

        // A half-confirmed candidate does not leak across the reset either.
        s.sample(&letterboxed_uhd());
        s.tracker.reset();
        assert_eq!(s.sample(&letterboxed_uhd()), CropAction::None);

        // New media with different framing is measured on its own.
        let mut s2 = Session::new(HD, 1.0);
        s2.samples(&framed(SNAP_W, SNAP_H, 69, 69, 0, 0), 2);
        assert_eq!(
            s2.tracker.applied().map(|b| b.crop_spec()),
            Some("0+138+0+138".into())
        );
    }

    #[test]
    fn full_frame_media_never_gets_a_crop() {
        let mut s = Session::new(HD, 1.0);
        s.samples(&framed(SNAP_W, SNAP_H, 0, 0, 0, 0), 12);
        assert_eq!(s.tracker.applied(), None);
        assert!(s.actions[1..].iter().all(|a| *a == CropAction::Stable));
    }

    #[test]
    fn differing_bars_inside_a_cropped_picture_do_not_tighten_it() {
        let mut t = CropTracker::default();
        let mk = |top, bottom, left, right| Bars {
            top,
            bottom,
            left,
            right,
            frame_width: 3840,
            frame_height: 2160,
            sar: 1.0,
        };
        let crop = mk(44, 40, 0, 0);
        t.observe(BarObservation::Bars(crop));
        t.observe(BarObservation::Bars(crop));
        assert_eq!(t.applied(), Some(crop));

        for _ in 0..10 {
            assert_eq!(
                t.observe(BarObservation::Bars(mk(0, 0, 400, 400))),
                CropAction::None
            );
            assert_eq!(
                t.observe(BarObservation::Bars(mk(300, 300, 0, 0))),
                CropAction::None
            );
        }
        assert_eq!(t.applied(), Some(crop));
    }

    #[test]
    fn stream_size_change_marks_a_crop_stale() {
        let b = Bars {
            top: 44,
            bottom: 40,
            left: 0,
            right: 0,
            frame_width: 3840,
            frame_height: 2160,
            sar: 1.0,
        };
        assert!(!b.stream_changed(3840, 2160));
        assert!(b.stream_changed(1920, 1080));
        assert!(!b.stream_changed(0, 0), "an unknown size is not a change");
    }

    #[test]
    fn close_to_separates_narrow_bars_from_no_bars() {
        let mk = |left| Bars {
            top: 0,
            bottom: 0,
            left,
            right: 0,
            frame_width: 720,
            frame_height: 480,
            sar: DVD_16_9_SAR,
        };
        assert!(mk(4).close_to(&mk(6)));
        assert!(mk(0).close_to(&mk(2)));
        assert!(!mk(0).close_to(&mk(4)));
        assert!(!mk(2).close_to(&mk(8)));
    }
}
