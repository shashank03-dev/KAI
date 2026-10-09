//! Kai's one accent, saffron, laid over Ely's palettes. Everything else is
//! Ely's. Status colours (green, amber, red) stay Ely's, so the accent never
//! carries meaning. Contrast is checked in the tests below.

use ely_gpui_component::theme::{Mode, Palette, Theme};
use gpui::{App, Hsla, rgb};

pub const SAFFRON_LIGHT: u32 = 0xb45309;
pub const SAFFRON_LIGHT_HOVER: u32 = 0x92400e;
pub const SAFFRON_DARK: u32 = 0xf59e0b;
pub const SAFFRON_DARK_HOVER: u32 = 0xfbbf24;

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn palette(mode: Mode) -> Palette {
    let mut p = match mode {
        Mode::Light => Palette::light(false),
        Mode::Dark => Palette::dark(false),
    };
    match mode {
        Mode::Light => {
            p.accent = c(SAFFRON_LIGHT);
            p.accent_hover = c(SAFFRON_LIGHT_HOVER);
            p.on_accent = c(0xffffff);
        }
        Mode::Dark => {
            p.accent = c(SAFFRON_DARK);
            p.accent_hover = c(SAFFRON_DARK_HOVER);
            p.on_accent = c(0x252525);
        }
    }
    p
}

pub fn apply(cx: &mut App) {
    Theme::set_palette(Mode::Light, Some(palette(Mode::Light)), cx);
    Theme::set_palette(Mode::Dark, Some(palette(Mode::Dark)), cx);
}

/// WCAG relative luminance contrast of two sRGB hex colours.
#[cfg(test)]
pub fn contrast(a: u32, b: u32) -> f32 {
    fn lum(hex: u32) -> f32 {
        let ch = |v: u32| {
            let s = v as f32 / 255.0;
            if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
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
    fn saffron_meets_wcag_aa_in_both_modes() {
        // Accent as text on the page background, and label text on the accent.
        assert!(contrast(SAFFRON_LIGHT, 0xfcfaf7) >= 4.5, "light accent on bg");
        assert!(contrast(0xffffff, SAFFRON_LIGHT) >= 4.5, "label on light accent");
        assert!(contrast(SAFFRON_DARK, 0x252525) >= 4.5, "dark accent on bg");
        assert!(contrast(0x252525, SAFFRON_DARK) >= 4.5, "label on dark accent");
    }
}
