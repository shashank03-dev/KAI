//! Kai Glass: a monochrome graphite palette laid over Ely's. The primary
//! colour is ink (white on dark, black on light), so actions read as the
//! brightest thing on screen without a hue. Kai's one hue, ember, is kept for
//! what is alive (the brand mark, a running run, focus glow) and is drawn by
//! `ui.rs`, never as an Ely accent. Status colours (green, amber, red) stay
//! Ely's, so colour still carries meaning. Contrast is checked in the tests.

use ely_gpui_component::theme::{Mode, Palette, Theme};
use gpui::{App, Hsla, rgb};

// Dark: graphite canvas, raised graphite surfaces, hairline borders.
pub const DARK_BG: u32 = 0x09090b;
pub const DARK_SURFACE: u32 = 0x131316;
pub const DARK_BORDER: u32 = 0x232328;
pub const DARK_FG: u32 = 0xededef;
pub const DARK_FG_MUTED: u32 = 0xa1a1aa;
pub const DARK_FG_SUBTLE: u32 = 0x6b6b74;
pub const DARK_INK: u32 = 0xfafafa;
pub const DARK_INK_HOVER: u32 = 0xd4d4d8;

// Light: porcelain canvas, white surfaces, soft graphite borders.
pub const LIGHT_BG: u32 = 0xf4f4f5;
pub const LIGHT_SURFACE: u32 = 0xffffff;
pub const LIGHT_BORDER: u32 = 0xe2e2e6;
pub const LIGHT_FG: u32 = 0x09090b;
pub const LIGHT_FG_MUTED: u32 = 0x52525b;
pub const LIGHT_FG_SUBTLE: u32 = 0x8b8b94;
pub const LIGHT_INK: u32 = 0x09090b;
pub const LIGHT_INK_HOVER: u32 = 0x27272a;

/// Ember: Kai's signal hue (the old saffron, kept as the brand).
pub const EMBER_DARK: u32 = 0xf5a524;
pub const EMBER_LIGHT: u32 = 0xc2410c;

/// How see-through the window canvas is when the glass material is on.
pub const GLASS_ALPHA: f32 = 0.78;

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn palette(mode: Mode, glass: bool) -> Palette {
    let mut p = match mode {
        Mode::Light => Palette::light(false),
        Mode::Dark => Palette::dark(false),
    };
    let canvas = |hex: u32| {
        let mut h = c(hex);
        if glass {
            h.a = GLASS_ALPHA;
        }
        h
    };
    match mode {
        Mode::Light => {
            p.bg = canvas(LIGHT_BG);
            p.surface = c(LIGHT_SURFACE);
            p.border = c(LIGHT_BORDER);
            p.fg = c(LIGHT_FG);
            p.fg_muted = c(LIGHT_FG_MUTED);
            p.fg_subtle = c(LIGHT_FG_SUBTLE);
            p.accent = c(LIGHT_INK);
            p.accent_hover = c(LIGHT_INK_HOVER);
            p.on_accent = c(0xffffff);
        }
        Mode::Dark => {
            p.bg = canvas(DARK_BG);
            p.surface = c(DARK_SURFACE);
            p.border = c(DARK_BORDER);
            p.fg = c(DARK_FG);
            p.fg_muted = c(DARK_FG_MUTED);
            p.fg_subtle = c(DARK_FG_SUBTLE);
            p.accent = c(DARK_INK);
            p.accent_hover = c(DARK_INK_HOVER);
            p.on_accent = c(DARK_BG);
        }
    }
    p
}

/// Install both palettes; call again when the glass setting changes.
pub fn apply(glass: bool, cx: &mut App) {
    Theme::set_palette(Mode::Light, Some(palette(Mode::Light, glass)), cx);
    Theme::set_palette(Mode::Dark, Some(palette(Mode::Dark, glass)), cx);
}

/// WCAG relative luminance contrast of two sRGB hex colours.
#[cfg(test)]
pub fn contrast(a: u32, b: u32) -> f32 {
    fn lum(hex: u32) -> f32 {
        let ch = |v: u32| {
            let s = v as f32 / 255.0;
            if s <= 0.03928 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(hex >> 16 & 0xff) + 0.7152 * ch(hex >> 8 & 0xff) + 0.0722 * ch(hex & 0xff)
    }
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_meets_wcag_aa_on_canvas_and_surfaces() {
        for (fg, bgs) in [
            (DARK_FG, [DARK_BG, DARK_SURFACE]),
            (DARK_FG_MUTED, [DARK_BG, DARK_SURFACE]),
            (LIGHT_FG, [LIGHT_BG, LIGHT_SURFACE]),
            (LIGHT_FG_MUTED, [LIGHT_BG, LIGHT_SURFACE]),
        ] {
            for bg in bgs {
                assert!(contrast(fg, bg) >= 4.5, "{fg:06x} on {bg:06x}");
            }
        }
    }

    #[test]
    fn ink_buttons_and_ember_read_well() {
        assert!(contrast(DARK_BG, DARK_INK) >= 4.5, "label on dark ink");
        assert!(contrast(0xffffff, LIGHT_INK) >= 4.5, "label on light ink");
        // Ember is used as text for live labels, so it must pass as text.
        assert!(contrast(EMBER_DARK, DARK_BG) >= 4.5, "ember on dark");
        assert!(contrast(EMBER_LIGHT, LIGHT_BG) >= 4.5, "ember on light");
    }

    #[test]
    fn subtle_text_is_at_least_large_text_contrast() {
        assert!(contrast(DARK_FG_SUBTLE, DARK_BG) >= 3.0);
        assert!(contrast(LIGHT_FG_SUBTLE, LIGHT_BG) >= 3.0);
    }
}
