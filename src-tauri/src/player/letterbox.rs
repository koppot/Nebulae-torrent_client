//! Encoded black-bar detection and crop calculation.
//!
//! Follows VideoLAN LibVLC 3.0 crop geometry specifications and FFmpeg cropdetect
//! principles (normalized digital black limits, noise tolerance, and stable evidence).

use std::path::Path;

/// Maximum luma value considered digital black in video frames.
///
/// In digital video (ITU-R BT.709 and BT.2020), limited range black is 16.
/// Decoded frames converted to sRGB / tone-mapped often contain compression noise,
/// dithering, and grain up to 24.
pub const BLACK_LUMA_LIMIT: u8 = 24;

/// Smallest bar worth cropping as a fraction of the frame dimension (1%).
pub const MIN_BAR_FRACTION: f64 = 0.01;

/// Largest bar accepted per edge as a fraction of the frame dimension (35%).
pub const MAX_BAR_FRACTION: f64 = 0.35;

/// Consecutive matching observations required to confirm a crop geometry.
pub const DEFAULT_CONFIRMATIONS: u32 = 2;

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
            && self.top.abs_diff(other.top) <= 8
            && self.bottom.abs_diff(other.bottom) <= 8
            && self.left.abs_diff(other.left) <= 8
            && self.right.abs_diff(other.right) <= 8
            && (self.sar - other.sar).abs() < 0.01
    }
}

/// The result of inspecting a sampled frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarObservation {
    /// Frame has measurable black bars on one or more borders.
    Bars(Bars),
    /// Frame is full picture with no encoded bars exceeding the minimum threshold.
    NoBars,
    /// Frame is black edge-to-edge or indeterminate (fade, blank, intro).
    Unknown,
}

/// Actions produced by the crop state tracker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CropAction {
    /// Apply this new confirmed crop geometry.
    Set(Bars),
    /// Clear the active crop back to uncropped full-frame.
    Clear,
    /// Confirmed geometry is identical to the currently applied crop.
    Stable,
    /// Not enough consecutive evidence yet to take action.
    None,
}

/// Calculates perceptual luma using ITU-R BT.709 coefficients.
#[inline]
fn rgb_luma(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 54 + g as u32 * 183 + b as u32 * 19) >> 8) as u8
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

    let mut sample_top = 0usize;
    while sample_top < h as usize && is_row_black(sample_top as u32) {
        sample_top += 1;
    }

    let mut sample_bottom = 0usize;
    while sample_bottom < h as usize && is_row_black((h as usize - 1 - sample_bottom) as u32) {
        sample_bottom += 1;
    }

    // If frame is black edge-to-edge vertically, it is a fade or blank frame.
    if sample_top + sample_bottom >= h as usize {
        return BarObservation::Unknown;
    }

    // Scan vertical columns within active picture rows
    let y_eval_start = (sample_top as u32).max((h as f64 * 0.05).round() as u32);
    let y_eval_end = ((h as usize - sample_bottom) as u32).min((h as f64 * 0.95).round() as u32);
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

    let mut sample_left = 0usize;
    while sample_left < w as usize && is_col_black(sample_left as u32) {
        sample_left += 1;
    }

    let mut sample_right = 0usize;
    while sample_right < w as usize && is_col_black((w as usize - 1 - sample_right) as u32) {
        sample_right += 1;
    }

    if sample_left + sample_right >= w as usize {
        return BarObservation::Unknown;
    }

    // Map sample coordinates to full source dimensions
    let top_px = ((sample_top as f64 / h as f64) * frame_height as f64).round() as u32;
    let bottom_px = ((sample_bottom as f64 / h as f64) * frame_height as f64).round() as u32;
    let left_px = ((sample_left as f64 / w as f64) * frame_width as f64).round() as u32;
    let right_px = ((sample_right as f64 / w as f64) * frame_width as f64).round() as u32;

    // Apply thresholds and round=2 (even pixel dimensions for YUV alignment)
    let top = if (top_px as f64 / frame_height as f64) >= MIN_BAR_FRACTION
        && (top_px as f64 / frame_height as f64) <= MAX_BAR_FRACTION
    {
        (top_px / 2) * 2
    } else {
        0
    };

    let bottom = if (bottom_px as f64 / frame_height as f64) >= MIN_BAR_FRACTION
        && (bottom_px as f64 / frame_height as f64) <= MAX_BAR_FRACTION
    {
        (bottom_px / 2) * 2
    } else {
        0
    };

    let mut left = if (left_px as f64 / frame_width as f64) >= MIN_BAR_FRACTION
        && (left_px as f64 / frame_width as f64) <= MAX_BAR_FRACTION
    {
        (left_px / 2) * 2
    } else {
        0
    };

    let mut right = if (right_px as f64 / frame_width as f64) >= MIN_BAR_FRACTION
        && (right_px as f64 / frame_width as f64) <= MAX_BAR_FRACTION
    {
        (right_px / 2) * 2
    } else {
        0
    };

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

/// Tracks observations over time and requires consecutive confirmations
/// before transitioning crop state (FFmpeg reset_count=0 stability principle).
#[derive(Debug, Clone)]
pub struct CropTracker {
    last_observation: Option<BarObservation>,
    confirmed_count: u32,
    confirmations_required: u32,
    applied: Option<Bars>,
}

impl CropTracker {
    pub fn new(confirmations_required: u32) -> Self {
        Self {
            last_observation: None,
            confirmed_count: 0,
            confirmations_required,
            applied: None,
        }
    }

    pub fn reset(&mut self) {
        self.last_observation = None;
        self.confirmed_count = 0;
        self.applied = None;
    }

    pub fn applied(&self) -> Option<Bars> {
        self.applied
    }

    pub fn observe(&mut self, obs: BarObservation) -> CropAction {
        match obs {
            BarObservation::Unknown => CropAction::None,
            BarObservation::NoBars => {
                if let Some(BarObservation::NoBars) = self.last_observation {
                    self.confirmed_count += 1;
                    if self.confirmed_count >= self.confirmations_required {
                        if self.applied.is_some() {
                            self.applied = None;
                            return CropAction::Clear;
                        }
                        return CropAction::Stable;
                    }
                } else {
                    self.last_observation = Some(BarObservation::NoBars);
                    self.confirmed_count = 1;
                }
                CropAction::None
            }
            BarObservation::Bars(new_bars) => {
                if let Some(BarObservation::Bars(prev_bars)) = self.last_observation {
                    if prev_bars.close_to(&new_bars) {
                        self.confirmed_count += 1;
                        if self.confirmed_count >= self.confirmations_required {
                            if self
                                .applied
                                .as_ref()
                                .map(|a| a.close_to(&new_bars))
                                .unwrap_or(false)
                            {
                                return CropAction::Stable;
                            }
                            self.applied = Some(new_bars);
                            return CropAction::Set(new_bars);
                        }
                    } else {
                        self.last_observation = Some(BarObservation::Bars(new_bars));
                        self.confirmed_count = 1;
                    }
                } else {
                    self.last_observation = Some(BarObservation::Bars(new_bars));
                    self.confirmed_count = 1;
                }
                CropAction::None
            }
        }
    }
}

impl Default for CropTracker {
    fn default() -> Self {
        Self::new(DEFAULT_CONFIRMATIONS)
    }
}
