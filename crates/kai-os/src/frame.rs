//! Screenshot post-processing: mask Kai's own windows, downscale to 1280px on
//! the long side with SIMD resize, hash, and JPEG-encode at ~75.

use fast_image_resize::{
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image as FirImage,
};
use image::{RgbImage, RgbaImage, codecs::jpeg::JpegEncoder};
use kai_core::Rect;

/// Long side of the image sent to the model.
pub const MODEL_LONG_SIDE: u32 = 1280;
pub const JPEG_QUALITY: u8 = 75;

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

/// Downscale so the long side is at most [`MODEL_LONG_SIDE`]. Bilinear via
/// fast_image_resize's SIMD paths: a 4K frame takes a few ms on a low-end CPU.
pub fn downscale(src: &RgbaImage) -> Frame {
    let (w, h) = src.dimensions();
    let long = w.max(h).max(1);
    if long <= MODEL_LONG_SIDE {
        return Frame {
            image: rgba_to_rgb(src),
            scale: 1.0,
        };
    }
    let scale = long as f32 / MODEL_LONG_SIDE as f32;
    let dw = ((w as f32 / scale).round() as u32).max(1);
    let dh = ((h as f32 / scale).round() as u32).max(1);

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

fn rgba_to_rgb(src: &RgbaImage) -> RgbImage {
    let (w, h) = src.dimensions();
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for px in src.pixels() {
        out.extend_from_slice(&px.0[..3]);
    }
    RgbImage::from_raw(w, h, out).expect("sized above")
}

pub fn encode_jpeg(img: &RgbImage) -> Vec<u8> {
    let mut buf = Vec::with_capacity((img.width() * img.height() / 6) as usize);
    let mut enc = JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
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

/// Frames within this Hamming distance count as unchanged.
pub const SAME_SCREEN_BITS: u32 = 3;

pub fn same_screen(a: u64, b: u64) -> bool {
    (a ^ b).count_ones() <= SAME_SCREEN_BITS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32, flip: bool) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, _| {
            let v = ((x * 255) / w.max(1)) as u8;
            let v = if flip { 255 - v } else { v };
            image::Rgba([v, v, v, 255])
        })
    }

    #[test]
    fn downscales_4k_to_1280_long_side() {
        let f = downscale(&gradient(3840, 2160, false));
        assert_eq!(f.image.width(), 1280);
        assert_eq!(f.image.height(), 720);
        assert!((f.scale - 3.0).abs() < 1e-3);
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
