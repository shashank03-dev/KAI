//! Where a model's coordinates live, per family, and how they become pixels
//! of the image it saw (`docs/RESEARCH_2026_10_OPENAI_PARITY.md` §5b). A
//! frame is data chosen per model, never inferred from a model's name.
use crate::computer::{ComputerAction, Target};
use serde::{Deserialize, Serialize};

/// Qwen2/2.5-VL's resize unit and pixel bounds (UI-TARS-1.5, OpenCUA, GTA1).
const QWEN25_FACTOR: u32 = 28;
const QWEN25_MIN_PIXELS: u64 = 100 * 28 * 28;
const QWEN25_MAX_PIXELS: u64 = 16384 * 28 * 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoordFrame {
    /// Numbered marks from the accessibility tree; `mark` instead of x,y
    /// (Gemma: no GUI-grounding training).
    Marks,
    /// Pixels of the screenshot (OpenAI, Fara).
    Pixels,
    /// 0..=1000 on both axes (Qwen3-VL, Holo2, Gemini).
    Norm1000,
    /// Pixels of Qwen2.5-VL's smart-resized image (UI-TARS-1.5, OpenCUA, GTA1).
    SmartResize,
}

impl CoordFrame {
    pub fn parse(name: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(name.trim().to_ascii_lowercase())).ok()
    }

    /// Pixel-trained models see the screen without marks drawn on it.
    pub fn unmarked(self) -> bool {
        self != Self::Marks
    }

    /// A model point to pixels of a `(w, h)` image; `None` when it lies
    /// outside the frame.
    pub fn to_image(self, x: f64, y: f64, (w, h): (u32, u32)) -> Option<(f64, f64)> {
        if !x.is_finite() || !y.is_finite() || x < 0. || y < 0. || w == 0 || h == 0 {
            return None;
        }
        let (fw, fh) = (w as f64, h as f64);
        let (px, py) = match self {
            Self::Marks | Self::Pixels => (x, y),
            Self::Norm1000 => {
                if x > 1000. || y > 1000. {
                    return None;
                }
                (x * fw / 1000., y * fh / 1000.)
            }
            Self::SmartResize => {
                let (rh, rw) =
                    smart_resize(h, w, QWEN25_FACTOR, QWEN25_MIN_PIXELS, QWEN25_MAX_PIXELS);
                (x * fw / rw as f64, y * fh / rh as f64)
            }
        };
        // The far edge (1000, or the last resized pixel) maps onto the last pixel.
        let clamp = |v: f64, side: f64| {
            if v >= side && v <= side + 1. {
                side - 1.
            } else {
                v
            }
        };
        let (px, py) = (clamp(px, fw), clamp(py, fh));
        (px < fw && py < fh).then_some((px, py))
    }

    /// The sentence that tells the model its coordinate frame.
    pub fn line(self) -> &'static str {
        match self {
            Self::Marks => {
                "Numbered boxes mark the interactive elements. Give `mark` (the box number) \
                 instead of x and y whenever the target has a number."
            }
            Self::Pixels => "Coordinates are pixels from the top-left of this screenshot.",
            Self::Norm1000 => {
                "Coordinates are 0 to 1000 on both axes: (0,0) is the top-left of the screenshot \
                 and (1000,1000) the bottom-right."
            }
            Self::SmartResize => "Coordinates are pixels of the image as you received it.",
        }
    }
}

/// Qwen2-VL's `smart_resize`: the `(height, width)` the processor turns an
/// `h`x`w` image into, sides multiples of `factor`, area within bounds.
pub fn smart_resize(h: u32, w: u32, factor: u32, min_pixels: u64, max_pixels: u64) -> (u32, u32) {
    let f = factor as f64;
    let (fh, fw) = (h as f64, w as f64);
    let round = |v: f64| ((v / f).round().max(1.) * f) as u32;
    let (mut rh, mut rw) = (round(fh), round(fw));
    let area = rh as u64 * rw as u64;
    if area > max_pixels {
        let beta = (fh * fw / max_pixels as f64).sqrt();
        rh = (((fh / beta) / f).floor().max(1.) * f) as u32;
        rw = (((fw / beta) / f).floor().max(1.) * f) as u32;
    } else if area < min_pixels {
        let beta = (min_pixels as f64 / (fh * fw)).sqrt();
        rh = (((fh * beta) / f).ceil() * f) as u32;
        rw = (((fw * beta) / f).ceil() * f) as u32;
    }
    (rh, rw)
}

/// Points and scroll deltas from the model's frame to the engine's: the
/// model may see a larger, unmarked frame than the one Kai observed.
pub fn rescale(action: &mut ComputerAction, from: (u32, u32), to: (u32, u32)) {
    if from == to || from.0 == 0 || from.1 == 0 {
        return;
    }
    let (sx, sy) = (to.0 as f64 / from.0 as f64, to.1 as f64 / from.1 as f64);
    let point = |t: &mut Target| {
        if let Target::Point { x, y, .. } = t {
            // Stay inside the engine's frame after rounding.
            *x = (*x * sx).min(to.0 as f64 - 1.);
            *y = (*y * sy).min(to.1 as f64 - 1.);
        }
    };
    match action {
        ComputerAction::Click { target, .. } | ComputerAction::Move { target, .. } => point(target),
        ComputerAction::Drag { points, .. } => points.iter_mut().for_each(point),
        ComputerAction::Scroll { target, dx, dy, .. } => {
            point(target);
            *dx = (*dx as f64 * sx).round() as i32;
            *dy = (*dy as f64 * sy).round() as i32;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer::ScrollUnit;

    #[test]
    fn rescales_points_and_scroll_to_the_engine_frame() {
        let mut a = ComputerAction::Scroll {
            target: Target::Point {
                observation: 1,
                x: 1599.,
                y: 450.,
            },
            dx: 0,
            dy: 300,
            unit: ScrollUnit::Pixels,
            modifiers: vec![],
        };
        rescale(&mut a, (1600, 900), (1067, 600));
        let ComputerAction::Scroll { target, dy, .. } = a else {
            unreachable!()
        };
        assert_eq!(dy, 200);
        let Target::Point { x, y, .. } = target else {
            unreachable!()
        };
        assert!(x < 1067. && (x - 1066.).abs() < 1.5);
        assert!((y - 300.).abs() < 0.01);
    }

    #[test]
    fn frames_parse_by_name() {
        assert_eq!(CoordFrame::parse("marks"), Some(CoordFrame::Marks));
        assert_eq!(CoordFrame::parse("norm1000"), Some(CoordFrame::Norm1000));
        assert_eq!(
            CoordFrame::parse("smart-resize"),
            Some(CoordFrame::SmartResize)
        );
        assert_eq!(CoordFrame::parse("guess"), None);
    }

    #[test]
    fn normalised_points_scale_to_the_image() {
        let f = CoordFrame::Norm1000;
        assert_eq!(f.to_image(500., 500., (1440, 900)), Some((720., 450.)));
        assert_eq!(f.to_image(1000., 1000., (1440, 900)), Some((1439., 899.)));
        assert_eq!(f.to_image(1001., 10., (1440, 900)), None);
        assert_eq!(f.to_image(-1., 10., (1440, 900)), None);
    }

    #[test]
    fn pixel_points_must_be_inside_the_image() {
        let f = CoordFrame::Pixels;
        assert_eq!(f.to_image(10., 20., (100, 50)), Some((10., 20.)));
        assert_eq!(f.to_image(100.5, 20., (100, 50)), Some((99., 20.)));
        assert_eq!(f.to_image(150., 20., (100, 50)), None);
    }

    #[test]
    fn smart_resize_matches_qwen_on_common_screens() {
        // Qwen2-VL's processor rounds each side to a multiple of 28.
        assert_eq!(
            smart_resize(1080, 1920, 28, QWEN25_MIN_PIXELS, QWEN25_MAX_PIXELS),
            (1092, 1932)
        );
        assert_eq!(
            smart_resize(768, 1366, 28, QWEN25_MIN_PIXELS, QWEN25_MAX_PIXELS),
            (756, 1372)
        );
        // Already a multiple of 28: identity, so clicks cannot drift.
        assert_eq!(
            smart_resize(896, 1428, 28, QWEN25_MIN_PIXELS, QWEN25_MAX_PIXELS),
            (896, 1428)
        );
        // Too large: scaled under the bound.
        let (h, w) = smart_resize(4000, 6000, 28, 3136, 1_000_000);
        assert!(h as u64 * w as u64 <= 1_000_000 && h % 28 == 0 && w % 28 == 0);
    }

    #[test]
    fn smart_resize_points_map_back_to_the_screenshot() {
        // UI-TARS saw 1932x1092 for a 1920x1080 screenshot.
        let (x, y) = CoordFrame::SmartResize
            .to_image(966., 546., (1920, 1080))
            .unwrap();
        assert!((x - 960.).abs() < 0.01 && (y - 540.).abs() < 0.01);
    }
}
