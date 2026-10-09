//! A zoomed second look at part of the screen: pad the region, clamp it to
//! the display, upscale small regions ~2x, and draw a fine numbered sub-grid
//! (the same digits as Set-of-Marks) so the model can say where inside the
//! region something is.

use crate::marks;
use image::{RgbImage, RgbaImage};
use kai_core::{Grounding, Rect};

/// Smallest region shown, in screen pixels (before upscaling).
const MIN_SIDE: f32 = 160.0;
/// Long side of the zoomed image at most this many pixels.
const MAX_OUT: f32 = 1280.0;
/// Regions with a long side below this are upscaled (up to 2x).
const UPSCALE_BELOW: f32 = 640.0;
/// Target number of sub-grid cells along the long side, and the smallest
/// cell in output pixels (so the number tags never swamp the content).
const CELLS_LONG: f32 = 8.0;
const MIN_CELL_OUT: f32 = 80.0;

/// What a zoom will show and how big.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomPlan {
    /// The region captured, in the same coordinates as `screen`.
    pub src: Rect,
    pub out_w: u32,
    pub out_h: u32,
    pub cols: u32,
    pub rows: u32,
}

impl ZoomPlan {
    /// Output pixels per screen pixel.
    pub fn factor(&self) -> f32 {
        self.out_w as f32 / self.src.w
    }

    /// Screen rect of sub-grid cell `n` (1-based, reading order).
    pub fn cell(&self, n: u32) -> Option<Rect> {
        if n == 0 || n > self.cols * self.rows {
            return None;
        }
        let (c, r) = ((n - 1) % self.cols, (n - 1) / self.cols);
        let (cw, ch) = (self.src.w / self.cols as f32, self.src.h / self.rows as f32);
        Some(Rect::new(
            self.src.x + c as f32 * cw,
            self.src.y + r as f32 * ch,
            cw,
            ch,
        ))
    }
}

/// Pad `rect` by a quarter of its long side (at least 24 px), grow it to at
/// least [`MIN_SIDE`], slide it inside `screen`, and size the output.
/// `None` when the region is not on the screen at all.
pub fn plan(rect: Rect, screen: Rect) -> Option<ZoomPlan> {
    let (sx1, sy1) = (screen.x + screen.w, screen.y + screen.h);
    let (cx, cy) = rect.center();
    let overlaps = rect.x < sx1
        && rect.y < sy1
        && rect.x + rect.w.max(1.0) > screen.x
        && rect.y + rect.h.max(1.0) > screen.y;
    if !overlaps || screen.w < 1.0 || screen.h < 1.0 || !cx.is_finite() || !cy.is_finite() {
        return None;
    }
    let pad = (rect.w.max(rect.h) * 0.25).max(24.0);
    let w = (rect.w.max(0.0) + 2.0 * pad).max(MIN_SIDE).min(screen.w);
    let h = (rect.h.max(0.0) + 2.0 * pad).max(MIN_SIDE).min(screen.h);
    let x = (cx - w / 2.0).clamp(screen.x, sx1 - w);
    let y = (cy - h / 2.0).clamp(screen.y, sy1 - h);
    let src = Rect::new(x.round(), y.round(), w.round(), h.round());

    let long = src.w.max(src.h);
    let factor = if long < UPSCALE_BELOW {
        (MAX_OUT / long).min(2.0)
    } else {
        (MAX_OUT / long).min(1.0)
    };
    let out_w = ((src.w * factor).round() as u32).max(1);
    let out_h = ((src.h * factor).round() as u32).max(1);
    let cell = (long / CELLS_LONG).max(MIN_CELL_OUT / factor);
    let cols = ((src.w / cell).round() as u32).clamp(1, 12);
    let rows = ((src.h / cell).round() as u32).clamp(1, 12);
    Some(ZoomPlan {
        src,
        out_w,
        out_h,
        cols,
        rows,
    })
}

/// Crop `plan.src` out of a capture whose top-left sits at `at` (global),
/// scale it and draw the sub-grid.
pub fn render(img: &RgbaImage, at: Rect, plan: &ZoomPlan) -> RgbImage {
    let (iw, ih) = img.dimensions();
    let x = ((plan.src.x - at.x).max(0.0) as u32).min(iw.saturating_sub(1));
    let y = ((plan.src.y - at.y).max(0.0) as u32).min(ih.saturating_sub(1));
    let w = (plan.src.w as u32).clamp(1, iw - x);
    let h = (plan.src.h as u32).clamp(1, ih - y);
    let part = image::imageops::crop_imm(img, x, y, w, h).to_image();
    let filter = if plan.out_w > w {
        image::imageops::FilterType::CatmullRom
    } else {
        image::imageops::FilterType::Triangle
    };
    let scaled = image::imageops::resize(&part, plan.out_w, plan.out_h, filter);
    let mut rgb = image::DynamicImage::ImageRgba8(scaled).to_rgb8();
    let grid = Grounding::Grid {
        cols: plan.cols,
        rows: plan.rows,
        screen: Rect::new(0.0, 0.0, plan.out_w as f32, plan.out_h as f32),
    };
    marks::draw(&mut rgb, &grid, 1.0);
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };

    #[test]
    fn small_target_is_padded_and_doubled() {
        let p = plan(Rect::new(500.0, 500.0, 40.0, 20.0), SCREEN).unwrap();
        // 40 wide + 2*24 pad = 88 → grown to the 160 minimum, centred.
        assert_eq!(p.src, Rect::new(440.0, 430.0, 160.0, 160.0));
        assert_eq!((p.out_w, p.out_h), (320, 320));
        // Cells stay at least 80 output pixels wide: 4x4, not 8x8.
        assert_eq!((p.cols, p.rows), (4, 4));
        assert!((p.factor() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn padding_is_a_quarter_of_the_long_side() {
        let p = plan(Rect::new(400.0, 300.0, 400.0, 200.0), SCREEN).unwrap();
        assert_eq!(p.src, Rect::new(300.0, 200.0, 600.0, 400.0));
        // 600 long side < 640 → upscaled, capped so the long side ≤ 1280.
        assert_eq!((p.out_w, p.out_h), (1200, 800));
        assert_eq!((p.cols, p.rows), (8, 5));
    }

    #[test]
    fn region_at_the_edge_slides_inside() {
        let p = plan(Rect::new(1900.0, 1070.0, 30.0, 30.0), SCREEN).unwrap();
        assert!(p.src.x + p.src.w <= SCREEN.w && p.src.y + p.src.h <= SCREEN.h);
        assert!(p.src.x >= 0.0 && p.src.y >= 0.0);
        let p = plan(Rect::new(-10.0, -10.0, 30.0, 30.0), SCREEN).unwrap();
        assert_eq!((p.src.x, p.src.y), (0.0, 0.0));
    }

    #[test]
    fn huge_region_is_clamped_to_screen_and_not_upscaled() {
        let p = plan(Rect::new(-500.0, -500.0, 4000.0, 3000.0), SCREEN).unwrap();
        assert_eq!(p.src, SCREEN);
        assert_eq!((p.out_w, p.out_h), (1280, 720));
    }

    #[test]
    fn off_screen_region_has_no_zoom() {
        assert!(plan(Rect::new(3000.0, 10.0, 50.0, 50.0), SCREEN).is_none());
    }

    #[test]
    fn second_monitor_offsets_are_respected() {
        let right = Rect::new(1920.0, 0.0, 1920.0, 1080.0);
        let p = plan(Rect::new(2000.0, 100.0, 40.0, 40.0), right).unwrap();
        assert!(p.src.x >= 1920.0);
        let img = RgbaImage::from_pixel(1920, 1080, image::Rgba([10, 20, 30, 255]));
        let out = render(&img, right, &p);
        assert_eq!(out.dimensions(), (p.out_w, p.out_h));
    }

    #[test]
    fn cells_map_back_to_screen() {
        let p = plan(Rect::new(400.0, 300.0, 400.0, 200.0), SCREEN).unwrap();
        assert_eq!(p.cell(1), Some(Rect::new(300.0, 200.0, 75.0, 80.0)));
        let last = p.cell(p.cols * p.rows).unwrap();
        assert!((last.x + last.w - 900.0).abs() < 1e-3);
        assert!(p.cell(0).is_none() && p.cell(p.cols * p.rows + 1).is_none());
    }

    #[test]
    fn render_draws_the_grid() {
        let p = plan(Rect::new(500.0, 500.0, 40.0, 20.0), SCREEN).unwrap();
        let img = RgbaImage::from_pixel(1920, 1080, image::Rgba([0, 0, 0, 255]));
        let out = render(&img, SCREEN, &p);
        let lit = out.pixels().filter(|px| px.0 != [0, 0, 0]).count();
        assert!(lit > 0);
    }
}
