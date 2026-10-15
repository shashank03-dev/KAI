//! Kai Glass, the design layer: tokens for layered glass on a graphite (or
//! porcelain) canvas, and the few composed pieces every page shares. Built on
//! plain gpui so it never fights Ely; Ely's own components pick up the palette
//! from `brand.rs`.
//!
//! The rules: depth comes from translucent layers, hairline borders and a
//! one-pixel top highlight (the "sheen"), not from heavy shadows. Ink is the
//! primary colour; ember marks only what is alive. Small labels are set in the
//! mono face, upper case, so structure reads like code.

use crate::brand;
use ely_gpui_component::theme::ActiveTheme;
use gpui::{
    AnyElement, App, BoxShadow, Div, FontWeight, Hsla, IntoElement, ParentElement, SharedString,
    Styled, div, hsla, linear_color_stop, linear_gradient, px, rgb,
};

/// Corner radii, smallest to largest.
pub const R_SM: f32 = 8.;
pub const R_MD: f32 = 12.;
pub const R_LG: f32 = 16.;

#[derive(Clone, Copy, Debug)]
pub struct Glass {
    pub dark: bool,
    /// Glass layer for columns beside the content (task list, sheets).
    pub pane: Hsla,
    /// Glass layer for cards on the canvas.
    pub card: Hsla,
    pub card_hover: Hsla,
    /// The most raised layer: the composer, the selected row.
    pub raised: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    /// The one-pixel highlight along a glass layer's top edge.
    pub sheen: Hsla,
    pub text: Hsla,
    pub dim: Hsla,
    pub faint: Hsla,
    /// Ember: what is alive.
    pub ember: Hsla,
    pub ember_soft: Hsla,
    pub shadow: Hsla,
}

fn hex(v: u32) -> Hsla {
    rgb(v).into()
}

fn white(a: f32) -> Hsla {
    hsla(0., 0., 1., a)
}

fn black(a: f32) -> Hsla {
    hsla(0., 0., 0., a)
}

impl Glass {
    pub fn of(cx: &App) -> Self {
        if cx.theme().is_dark() {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn dark() -> Self {
        let mut ember_soft = hex(brand::EMBER_DARK);
        ember_soft.a = 0.16;
        Self {
            dark: true,
            pane: white(0.025),
            card: white(0.04),
            card_hover: white(0.07),
            raised: white(0.06),
            line: white(0.08),
            line_strong: white(0.16),
            sheen: white(0.07),
            text: hex(brand::DARK_FG),
            dim: hex(brand::DARK_FG_MUTED),
            faint: hex(brand::DARK_FG_SUBTLE),
            ember: hex(brand::EMBER_DARK),
            ember_soft,
            shadow: black(0.45),
        }
    }

    pub fn light() -> Self {
        let mut ember_soft = hex(brand::EMBER_LIGHT);
        ember_soft.a = 0.12;
        Self {
            dark: false,
            pane: white(0.55),
            card: white(0.7),
            card_hover: white(0.95),
            raised: white(0.9),
            line: black(0.08),
            line_strong: black(0.16),
            sheen: white(0.9),
            text: hex(brand::LIGHT_FG),
            dim: hex(brand::LIGHT_FG_MUTED),
            faint: hex(brand::LIGHT_FG_SUBTLE),
            ember: hex(brand::EMBER_LIGHT),
            ember_soft,
            shadow: black(0.12),
        }
    }

    /// The top-edge highlight every glass layer carries.
    pub fn sheen_shadow(&self) -> BoxShadow {
        BoxShadow::new(px(0.), px(1.), self.sheen).inset()
    }

    /// A soft drop shadow for the most raised layers.
    pub fn lift(&self) -> BoxShadow {
        BoxShadow::new(px(0.), px(18.), self.shadow)
            .blur_radius(px(40.))
            .spread_radius(px(-12.))
    }

    /// A halo in `color` for live things (dots, orbs, the composer's focus).
    pub fn glow(&self, color: Hsla, radius: f32) -> BoxShadow {
        let mut c = color;
        c.a = if self.dark { 0.45 } else { 0.3 };
        BoxShadow::new(px(0.), px(0.), c).blur_radius(px(radius))
    }
}

/// A glass card: translucent fill, hairline border, top sheen.
pub fn card(g: &Glass) -> Div {
    div()
        .bg(g.card)
        .border_1()
        .border_color(g.line)
        .rounded(px(R_MD))
        .shadow(vec![g.sheen_shadow()])
}

/// A small upper-case label in the mono face: section names, counters, meta.
pub fn eyebrow(text: impl Into<SharedString>, cx: &App) -> Div {
    let g = Glass::of(cx);
    let theme = cx.theme();
    let text: SharedString = text.into();
    div()
        .font_family(theme.mono_family.clone())
        .text_size(px(11.))
        .text_color(g.faint)
        .child(SharedString::from(text.to_uppercase()))
}

/// Mono meta text at normal case ("step 3 of 40 · 864 ms").
pub fn mono(text: impl Into<SharedString>, color: Hsla, cx: &App) -> Div {
    div()
        .font_family(cx.theme().mono_family.clone())
        .text_size(px(12.))
        .text_color(color)
        .child(text.into())
}

/// The key that app shortcuts use: ⌘ on a Mac, Ctrl elsewhere (gpui's
/// `secondary`).
pub const MOD_KEY: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl"
};

/// The Alt key as its keyboard labels it: ⌥ on a Mac.
pub const ALT_KEY: &str = if cfg!(target_os = "macos") {
    "⌥"
} else {
    "Alt"
};

/// How a key in a gpui chord (`secondary-k`, `ctrl-alt-escape`) reads on
/// this OS's keyboard: ⌘ ⌥ ⌃ on a Mac, words elsewhere.
pub fn key_label(key: &str) -> String {
    let mac = cfg!(target_os = "macos");
    match key {
        "secondary" => MOD_KEY.into(),
        "alt" => ALT_KEY.into(),
        "ctrl" if mac => "⌃".into(),
        "cmd" | "super" | "platform" if mac => "⌘".into(),
        _ => {
            let mut cap = key.to_string();
            if let Some(first) = cap.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            cap
        }
    }
}

/// A key cap: "Ctrl", "K", "↵".
pub fn kbd(key: impl Into<SharedString>, cx: &App) -> Div {
    let g = Glass::of(cx);
    div()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(5.))
        .border_1()
        .border_color(g.line_strong)
        .bg(g.card)
        .font_family(cx.theme().mono_family.clone())
        .text_size(px(11.))
        .text_color(g.dim)
        .child(key.into())
}

/// A status dot; `live` adds a halo.
pub fn dot(color: Hsla, live: bool, g: &Glass) -> Div {
    let d = div().size(px(8.)).flex_none().rounded_full().bg(color);
    if live {
        d.shadow(vec![g.glow(color, 10.)])
    } else {
        d
    }
}

/// Kai's mark: the hand in a glass orb with an ember halo.
pub fn orb(size: f32, g: &Glass) -> Div {
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(g.raised)
        .border_1()
        .border_color(g.line_strong)
        .shadow(vec![g.sheen_shadow(), g.glow(g.ember, size * 0.6)])
        .child(
            gpui::svg()
                .path(crate::assets::HAND)
                .size(px(size * 0.5))
                .text_color(g.ember),
        )
}

/// A soft light leak along the top of a view: ember fading into the canvas.
/// Static (no animation), so it costs nothing while idle.
pub fn aurora(height: f32, g: &Glass) -> Div {
    let mut top = g.ember;
    top.a = if g.dark { 0.10 } else { 0.07 };
    let mut clear = top;
    clear.a = 0.;
    div()
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(height))
        .bg(linear_gradient(
            180.,
            linear_color_stop(top, 0.),
            linear_color_stop(clear, 1.),
        ))
}

/// The page frame shared by Models, Skills, History and Benchmarks: an
/// eyebrow, a large title, a one-line subtitle and an optional action, over
/// a readable column.
pub fn page(
    section: &str,
    title: &str,
    subtitle: impl Into<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> Div {
    let g = Glass::of(cx);
    div().w_full().flex().flex_col().gap(px(28.)).child(
        div()
            .flex()
            .items_end()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    // Grow as well as shrink: inside a scroll area a
                    // shrink-only column collapses to zero width.
                    .flex_1()
                    .min_w_0()
                    .child(eyebrow(section.to_string(), cx))
                    .child(
                        div()
                            .text_size(px(30.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(g.text)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(g.dim)
                            .child(subtitle.into()),
                    ),
            )
            .children(action),
    )
}

/// The padded, width-capped column every full page sits in.
pub fn body(max: f32, content: impl IntoElement) -> Div {
    div()
        .w_full()
        .flex()
        .justify_center()
        .px(px(40.))
        .pt(px(36.))
        .pb(px(48.))
        .child(div().w_full().max_w(px(max)).child(content))
}

/// A row of shortcut hints, shown where the action is (Raycast-style), so
/// keys are learned by seeing them: `[(&["Ctrl", "R"], "run again")]`.
pub fn hints(items: &[(&[&str], &str)], cx: &App) -> Div {
    let g = Glass::of(cx);
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_4()
        .children(items.iter().map(|(caps, what)| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .children(caps.iter().map(|c| kbd(c.to_string(), cx)))
                .child(
                    div()
                        .pl_1()
                        .text_size(px(12.))
                        .text_color(g.faint)
                        .child(what.to_string()),
                )
        }))
}

/// An empty state that points somewhere: the mark, what is missing, why it
/// will fill, and the one action that fills it.
pub fn empty(title: &str, body: &str, action: impl IntoElement, cx: &App) -> Div {
    let g = Glass::of(cx);
    div()
        .w_full()
        .py(px(56.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(14.))
        .child(orb(48., &g))
        .child(
            div()
                .text_size(px(16.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(g.text)
                .child(title.to_string()),
        )
        .child(
            div()
                .max_w(px(420.))
                .text_center()
                .text_size(px(13.))
                .text_color(g.dim)
                .child(body.to_string()),
        )
        .child(div().pt_2().child(action))
}

// Exercise the exact vendored wheel policy in the workspace test command.
#[cfg(test)]
#[path = "../../../vendor/ely-gpui-component/src/layout/scroll_policy.rs"]
mod scroll_policy_contract;
