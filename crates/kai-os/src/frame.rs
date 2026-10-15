//! Screenshot post-processing: mask Kai's own windows, downscale with SIMD
//! resize to Gemma 4's default 280-soft-token budget (1070x602 for 16:9,
//! never more than 1280 px on the long side), hash, and JPEG-encode at ~75.

use fast_image_resize::{
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image as FirImage,
};
use image::{RgbImage, RgbaImage, codecs::jpeg::JpegEncoder};
use kai_core::Rect;

/// Longest side of the image sent to the model, at most.
pub const MODEL_LONG_SIDE: u32 = 1280;
/// Default pixel budget: Gemma 4's default 280 soft tokens of ~48x48 px.
/// A frame that fits is not resized again by the server, so the mark digits
/// are resampled once (by us) instead of twice.
pub const MODEL_MAX_PIXELS: u32 = 280 * 48 * 48;
pub const JPEG_QUALITY: u8 = 75;

/// How frames for the model are sized and encoded. Set from the environment
/// (`KAI_FRAME_MAX_SIDE`, `KAI_FRAME_MAX_PIXELS`, `KAI_JPEG_QUALITY`,
/// `KAI_WINDOW_SCOPE`) on first use, or by the host with [`set_config`], so benches can A/B frame sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameConfig {
    /// Longest side in pixels.
    pub max_side: u32,
    /// Pixel budget (w*h); 0 = none. Gemma 4 spends one soft token per
    /// ~48x48 px, so 280 tokens ≈ 645_120 px.
    pub max_pixels: u32,
    /// JPEG quality, 1..=100.
    pub quality: u8,
    /// Show the model only the focused app's windows, not the whole display
    /// (`KAI_WINDOW_SCOPE=1`; `crate::window`). Off by default.
    pub window_scope: bool,
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self {
            max_side: MODEL_LONG_SIDE,
            max_pixels: MODEL_MAX_PIXELS,
            quality: JPEG_QUALITY,
            window_scope: false,
        }
    }
}

impl FrameConfig {
    /// Defaults overridden by any valid `KAI_FRAME_*` / `KAI_JPEG_QUALITY`.
    pub fn from_env() -> Self {
        let num = |k: &str| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.trim().parse::<u32>().ok())
        };
        let d = Self::default();
        Self {
            max_side: num("KAI_FRAME_MAX_SIDE")
                .filter(|v| (256..=7680).contains(v))
                .unwrap_or(d.max_side),
            max_pixels: num("KAI_FRAME_MAX_PIXELS").unwrap_or(d.max_pixels),
            quality: num("KAI_JPEG_QUALITY")
                .filter(|v| (1..=100).contains(v))
                .map(|v| v as u8)
                .unwrap_or(d.quality),
            window_scope: std::env::var("KAI_WINDOW_SCOPE").is_ok_and(|v| v.trim() == "1"),
        }
    }

    /// Output size for a `w`x`h` source: never upscaled, aspect kept.
    pub fn target_size(&self, w: u32, h: u32) -> (u32, u32) {
        let (w, h) = (w.max(1), h.max(1));
        let mut scale = w.max(h) as f64 / self.max_side.max(1) as f64;
        if self.max_pixels > 0 {
            scale = scale.max((w as f64 * h as f64 / self.max_pixels as f64).sqrt());
        }
        if scale <= 1.0 {
            return (w, h);
        }
        let dw = (w as f64 / scale).floor().max(1.0) as u32;
        let dh = (h as f64 / scale).floor().max(1.0) as u32;
        (dw, dh)
    }
}

static CONFIG: std::sync::RwLock<Option<FrameConfig>> = std::sync::RwLock::new(None);

/// The frame settings in effect.
pub fn config() -> FrameConfig {
    if let Some(c) = CONFIG.read().ok().and_then(|c| *c) {
        return c;
    }
    let c = FrameConfig::from_env();
    if let Ok(mut slot) = CONFIG.write() {
        *slot = Some(c);
    }
    c
}

/// Replace the frame settings for every later observation.
pub fn set_config(c: FrameConfig) {
    if let Ok(mut slot) = CONFIG.write() {
        *slot = Some(c);
    }
}

/// A downscaled frame plus the factor that maps it back to screen pixels.
pub struct Frame {
    pub image: RgbImage,
    /// Screen pixels per frame pixel.
    pub scale: f32,
}

/// Paint over regions that must never reach the model (Kai's own windows where
/// the OS cannot exclude them from capture).
pub fn mask(img: &mut RgbaImage, rects: &[Rect]) {
    let (w, h) = img.dimensions();
    for r in rects {
        let x0 = r.x.max(0.0) as u32;
        let y0 = r.y.max(0.0) as u32;
        let x1 = ((r.x + r.w).max(0.0) as u32).min(w);
        let y1 = ((r.y + r.h).max(0.0) as u32).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, image::Rgba([24, 24, 24, 255]));
            }
        }
    }
}

/// Downscale to the configured size (see [`config`]).
pub fn downscale(src: &RgbaImage) -> Frame {
    downscale_with(src, &config())
}

/// Downscale so the frame fits `cfg`. Bilinear via fast_image_resize's SIMD
/// paths: a 4K frame takes a few ms on a low-end CPU.
pub fn downscale_with(src: &RgbaImage, cfg: &FrameConfig) -> Frame {
    let (w, h) = src.dimensions();
    let (dw, dh) = cfg.target_size(w, h);
    if (dw, dh) == (w, h) {
        return Frame {
            image: rgba_to_rgb(src),
            scale: 1.0,
        };
    }
    let scale = w as f32 / dw as f32;

    let rgb = rgba_to_rgb(src);
    let src_img = FirImage::from_vec_u8(w, h, rgb.into_raw(), PixelType::U8x3)
        .expect("buffer matches dimensions");
    let mut dst = FirImage::new(dw, dh, PixelType::U8x3);
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    Resizer::new()
        .resize(&src_img, &mut dst, &opts)
        .expect("same pixel type");
    let image = RgbImage::from_raw(dw, dh, dst.into_vec()).expect("buffer matches dimensions");
    Frame { image, scale }
}

/// Resize a capture to the display's size in the OS's input coordinates.
/// On a Retina Mac the capture has two pixels per point while clicks and the
/// accessibility tree use points; elsewhere the sizes match and the capture
/// comes back untouched.
pub fn to_display(src: RgbaImage, w: u32, h: u32) -> RgbaImage {
    if src.dimensions() == (w, h) || w == 0 || h == 0 {
        return src;
    }
    let (sw, sh) = src.dimensions();
    let src_img = FirImage::from_vec_u8(sw, sh, src.into_raw(), PixelType::U8x4)
        .expect("buffer matches dimensions");
    let mut dst = FirImage::new(w, h, PixelType::U8x4);
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    Resizer::new()
        .resize(&src_img, &mut dst, &opts)
        .expect("same pixel type");
    RgbaImage::from_raw(w, h, dst.into_vec()).expect("buffer matches dimensions")
}

fn rgba_to_rgb(src: &RgbaImage) -> RgbImage {
    let (w, h) = src.dimensions();
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for px in src.pixels() {
        out.extend_from_slice(&px.0[..3]);
    }
    RgbImage::from_raw(w, h, out).expect("sized above")
}

/// JPEG at the configured quality.
pub fn encode_jpeg(img: &RgbImage) -> Vec<u8> {
    encode_jpeg_q(img, config().quality)
}

pub fn encode_jpeg_q(img: &RgbImage, quality: u8) -> Vec<u8> {
    let mut buf = Vec::with_capacity((img.width() * img.height() / 6) as usize);
    let mut enc = JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100));
    enc.encode_image(img)
        .expect("encoding to memory cannot fail");
    buf
}

/// Crop a region (in frame pixels) and upscale it 2x for a zoomed second look.
pub fn crop(img: &RgbImage, r: Rect) -> RgbImage {
    let (w, h) = img.dimensions();
    let x = (r.x.max(0.0) as u32).min(w.saturating_sub(1));
    let y = (r.y.max(0.0) as u32).min(h.saturating_sub(1));
    let cw = (r.w.max(1.0) as u32).min(w - x);
    let ch = (r.h.max(1.0) as u32).min(h - y);
    let part = image::imageops::crop_imm(img, x, y, cw, ch).to_image();
    image::imageops::resize(&part, cw * 2, ch * 2, image::imageops::FilterType::Triangle)
}

/// dHash: 9x8 grayscale, one bit per horizontal gradient. Two frames that
/// differ by a few bits are "the same screen".
pub fn dhash(img: &RgbImage) -> u64 {
    let small = image::imageops::resize(img, 9, 8, image::imageops::FilterType::Triangle);
    let gray = |x: u32, y: u32| {
        let p = small.get_pixel(x, y).0;
        p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114
    };
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            hash <<= 1;
            if gray(x, y) > gray(x + 1, y) {
                hash |= 1;
            }
        }
    }
    hash
}

/// An exact fingerprint fine enough to see a typed word or a ticked box:
/// 240x135 grayscale at 32 levels, FNV-1a. Used for "did the last action
/// change anything?" (the coarse `dhash` misses small edits like typing into
/// a form field, which made the loop wrongly report "no progress").
pub fn content_hash(img: &RgbImage) -> u64 {
    let small = image::imageops::resize(img, 240, 135, image::imageops::FilterType::Triangle);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in small.pixels() {
        let g = (p.0[0] as u32 * 299 + p.0[1] as u32 * 587 + p.0[2] as u32 * 114) / 1000;
        h ^= (g >> 3) as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Frames within this Hamming distance count as unchanged.
pub const SAME_SCREEN_BITS: u32 = 3;

pub fn same_screen(a: u64, b: u64) -> bool {
    (a ^ b).count_ones() <= SAME_SCREEN_BITS
}

#[cfg(test)]
mod tests {

    #[test]
    fn content_hash_sees_a_small_edit() {
        let mut a = RgbImage::from_pixel(1920, 1080, image::Rgb([240, 240, 240]));
        let b = a.clone();
        assert_eq!(content_hash(&a), content_hash(&b));
        // A typed word: a 60x14 px dark block inside a form field.
        for y in 500..514 {
            for x in 800..860 {
                a.put_pixel(x, y, image::Rgb([30, 30, 30]));
            }
        }
        assert_ne!(content_hash(&a), content_hash(&b));
        assert!(same_screen(dhash(&a), dhash(&b)), "dhash alone misses it");
    }

    use super::*;

    fn gradient(w: u32, h: u32, flip: bool) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, _| {
            let v = ((x * 255) / w.max(1)) as u8;
            let v = if flip { 255 - v } else { v };
            image::Rgba([v, v, v, 255])
        })
    }

    #[test]
    fn default_fits_280_soft_tokens() {
        let d = FrameConfig::default();
        let f = downscale_with(&gradient(3840, 2160, false), &d);
        assert_eq!(f.image.dimensions(), (1070, 602));
        assert!((f.scale - 3840.0 / 1070.0).abs() < 1e-3);
        // Ultra-wide: the 1280 px long-side cap binds before the budget.
        assert_eq!(d.target_size(3840, 1080), (1280, 360));
    }

    #[test]
    fn config_sizes_by_side_and_pixel_budget() {
        let side = FrameConfig {
            max_side: 1024,
            max_pixels: 0,
            quality: 70,
            window_scope: false,
        };
        assert_eq!(side.target_size(1920, 1080), (1024, 576));
        let tokens = FrameConfig {
            max_side: 4096,
            max_pixels: 280 * 48 * 48,
            quality: 70,
            window_scope: false,
        };
        let (w, h) = tokens.target_size(1920, 1080);
        assert!(w * h <= 280 * 48 * 48, "{w}x{h}");
        assert_eq!((w, h), (1070, 602));
        let (w, h) = tokens.target_size(1920, 1200);
        assert!(w * h <= 280 * 48 * 48 && w == 1015, "{w}x{h}");
        assert_eq!(tokens.target_size(800, 600), (800, 600));
        let f = downscale_with(&gradient(1920, 1080, false), &side);
        assert_eq!(f.image.dimensions(), (1024, 576));
        assert!((f.scale - 1.875).abs() < 1e-3);
    }

    #[test]
    fn retina_capture_maps_to_display_points() {
        let px = gradient(2880, 1864, false);
        let pt = to_display(px, 1440, 932);
        assert_eq!(pt.dimensions(), (1440, 932));
        let same = to_display(gradient(1920, 1080, false), 1920, 1080);
        assert_eq!(same.dimensions(), (1920, 1080));
    }

    #[test]
    fn small_frames_are_not_upscaled() {
        let f = downscale(&gradient(800, 600, false));
        assert_eq!(f.image.dimensions(), (800, 600));
        assert_eq!(f.scale, 1.0);
    }

    #[test]
    fn hash_is_stable_and_discriminates() {
        let a = downscale(&gradient(640, 480, false)).image;
        let b = downscale(&gradient(640, 480, true)).image;
        assert!(same_screen(dhash(&a), dhash(&a)));
        assert!(!same_screen(dhash(&a), dhash(&b)));
    }

    #[test]
    fn mask_paints_over_own_window() {
        let mut img = RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]));
        mask(&mut img, &[Rect::new(10.0, 10.0, 20.0, 20.0)]);
        assert_eq!(img.get_pixel(15, 15).0, [24, 24, 24, 255]);
        assert_eq!(img.get_pixel(50, 50).0, [255, 255, 255, 255]);
    }

    #[test]
    fn jpeg_is_produced() {
        let f = downscale(&gradient(320, 200, false));
        let bytes = encode_jpeg(&f.image);
        assert_eq!(&bytes[..2], &[0xff, 0xd8]);
    }

    #[test]
    fn crop_is_doubled_and_clamped() {
        let img = RgbImage::new(100, 100);
        let c = crop(&img, Rect::new(90.0, 90.0, 50.0, 50.0));
        assert_eq!(c.dimensions(), (20, 20));
    }
}
