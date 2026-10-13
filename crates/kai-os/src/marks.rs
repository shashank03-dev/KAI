//! Draw Set-of-Marks on the model's copy of the screenshot: a box around each
//! element and its number in a solid tag, or a numbered grid when there is no
//! accessibility tree. Digits come from a built-in 3x5 bitmap font, so drawing
//! needs no font file and costs well under a millisecond.

use image::{Rgb, RgbImage};
use kai_core::{Grounding, Rect};

const BOX: Rgb<u8> = Rgb([255, 0, 170]);
const TAG_BG: Rgb<u8> = Rgb([255, 0, 170]);
const TAG_FG: Rgb<u8> = Rgb([255, 255, 255]);
const GRID: Rgb<u8> = Rgb([0, 200, 255]);
/// Each font pixel is drawn as a square this many image pixels wide.
const GLYPH_SCALE: u32 = 2;

/// 3x5 digits, one row per byte, top row first, bit 2 = left column.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

/// Draw the grounding onto `img`. `scale` is screen pixels per image pixel.
pub fn draw(img: &mut RgbImage, grounding: &Grounding, scale: f32) {
    match grounding {
        Grounding::Marks(els) => {
            let frames: Vec<Rect> = els.iter().map(|e| to_frame(e.bounds, scale)).collect();
            for (i, e) in els.iter().enumerate() {
                let r = frames[i];
                outline(img, r, BOX, 2);
                // Above the box's top-left corner, so the label stays readable;
                // inside when the box touches the top edge, or when the space
                // above is another box: in a menu or list a tag above sits on
                // the row before and reads as its number (seen live: "Find
                // and Replace" taken for the next item, 19 times).
                let th = tag_height() as i64;
                let above = Rect::new(r.x, r.y - th as f32, tag_width(e.mark) as f32, th as f32);
                let crowded = frames
                    .iter()
                    .enumerate()
                    .any(|(j, o)| j != i && overlaps(*o, above));
                let y = if r.y as i64 >= th && !crowded {
                    r.y as i64 - th
                } else {
                    r.y as i64
                };
                tag(img, r.x as i64, y, e.mark);
            }
        }
        Grounding::Grid { cols, rows, screen } => {
            let s = to_frame(*screen, scale);
            let cw = s.w / *cols as f32;
            let ch = s.h / *rows as f32;
            for r in 0..*rows {
                for c in 0..*cols {
                    let cell = Rect::new(s.x + c as f32 * cw, s.y + r as f32 * ch, cw, ch);
                    outline(img, cell, GRID, 1);
                    tag(img, cell.x as i64 + 1, cell.y as i64 + 1, r * cols + c + 1);
                }
            }
        }
    }
}

/// The two rects share some area (touching edges do not count).
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn to_frame(r: Rect, scale: f32) -> Rect {
    let s = if scale > 0.0 { scale } else { 1.0 };
    Rect::new(r.x / s, r.y / s, r.w / s, r.h / s)
}

fn put(img: &mut RgbImage, x: i64, y: i64, c: Rgb<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, c);
    }
}

fn outline(img: &mut RgbImage, r: Rect, c: Rgb<u8>, thick: i64) {
    let (x0, y0) = (r.x as i64, r.y as i64);
    let (x1, y1) = ((r.x + r.w) as i64, (r.y + r.h) as i64);
    for t in 0..thick {
        for x in x0..=x1 {
            put(img, x, y0 + t, c);
            put(img, x, y1 - t, c);
        }
        for y in y0..=y1 {
            put(img, x0 + t, y, c);
            put(img, x1 - t, y, c);
        }
    }
}

/// Width in pixels of a number's tag (padding included).
pub fn tag_width(n: u32) -> u32 {
    let digits = n.to_string().len() as u32;
    digits * 4 * GLYPH_SCALE + GLYPH_SCALE
}

pub fn tag_height() -> u32 {
    7 * GLYPH_SCALE
}

fn tag(img: &mut RgbImage, x: i64, y: i64, n: u32) {
    let w = tag_width(n) as i64;
    let h = tag_height() as i64;
    for dy in 0..h {
        for dx in 0..w {
            put(img, x + dx, y + dy, TAG_BG);
        }
    }
    let g = GLYPH_SCALE as i64;
    for (i, ch) in n.to_string().bytes().enumerate() {
        let glyph = DIGITS[(ch - b'0') as usize];
        let gx = x + g + i as i64 * 4 * g;
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    for sy in 0..g {
                        for sx in 0..g {
                            put(img, gx + col * g + sx, y + g + row as i64 * g + sy, TAG_FG);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::{Element, Role};

    fn el(mark: u32, x: f32, y: f32) -> Element {
        Element {
            mark,
            role: Role::Button,
            name: "b".into(),
            bounds: Rect::new(x, y, 40.0, 20.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }
    }

    #[test]
    fn marks_draw_box_and_tag_at_scaled_position() {
        let mut img = RgbImage::new(200, 200);
        // Element at screen (100,100) with scale 2 lands at frame (50,50);
        // its tag sits just above the corner.
        draw(&mut img, &Grounding::Marks(vec![el(7, 100.0, 100.0)]), 2.0);
        assert_eq!(*img.get_pixel(51, 50 - tag_height() + 1), TAG_BG);
        // The first pixels inside the box stay clear for the label.
        assert_eq!(*img.get_pixel(55, 55), Rgb([0, 0, 0]));
        // Right edge of the box: (100+40)/2 = 70.
        assert_eq!(*img.get_pixel(70, 55), BOX);
        // Outside untouched.
        assert_eq!(*img.get_pixel(150, 150), Rgb([0, 0, 0]));
    }

    #[test]
    fn in_a_stacked_list_each_tag_sits_in_its_own_row() {
        // Two menu rows, 20 px each, one under the other: the lower one's
        // tag goes inside its own box, not onto the row above.
        let mut img = RgbImage::new(200, 200);
        let rows = vec![el(10, 20.0, 60.0), el(11, 20.0, 80.0)];
        draw(&mut img, &Grounding::Marks(rows), 1.0);
        // Row 10 has free space above it: its tag is there.
        assert_eq!(*img.get_pixel(21, 60 - tag_height() + 1), TAG_BG);
        // Row 11's tag starts inside row 11 (y 80..100)...
        assert_eq!(*img.get_pixel(21, 81), TAG_BG);
        // ...and nothing of it is drawn inside row 10 (y 60..80).
        assert_eq!(*img.get_pixel(23, 75), Rgb([0, 0, 0]));
    }

    #[test]
    fn digits_render_foreground_pixels() {
        let mut img = RgbImage::new(60, 40);
        // At the top edge the tag goes inside the box.
        draw(&mut img, &Grounding::Marks(vec![el(8, 0.0, 0.0)]), 1.0);
        let fg = img.pixels().filter(|p| **p == TAG_FG).count();
        // "8" lights 13 of 15 font cells, each GLYPH_SCALE^2 pixels.
        assert_eq!(fg as u32, 13 * GLYPH_SCALE * GLYPH_SCALE);
    }

    #[test]
    fn grid_draws_every_cell() {
        let mut img = RgbImage::new(100, 100);
        let g = Grounding::Grid {
            cols: 4,
            rows: 4,
            screen: Rect::new(0.0, 0.0, 100.0, 100.0),
        };
        draw(&mut img, &g, 1.0);
        assert_eq!(*img.get_pixel(25, 50), GRID);
    }

    #[test]
    fn off_screen_boxes_do_not_panic() {
        let mut img = RgbImage::new(10, 10);
        draw(
            &mut img,
            &Grounding::Marks(vec![el(123, -50.0, 500.0)]),
            1.0,
        );
    }

    #[test]
    fn tag_width_grows_with_digits() {
        assert!(tag_width(123) > tag_width(5));
    }
}
