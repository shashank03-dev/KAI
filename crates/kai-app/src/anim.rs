//! Kai's motion: short, eased, and only where it explains a change.
//!
//! Timings follow what fast desktop apps converge on (Warp blocks enter in
//! 150 ms with a 4 px rise; UI motion stays under ~300 ms; siblings stagger by
//! 30–50 ms; ease-out for entrances, linear only for continuous motion).
//! Nothing loops while Kai is idle: the only repeating animation is the live
//! dot's breath, shown only while a run is going. gpui renders every one of
//! these in its end state when the OS asks to reduce motion.

use crate::ui::Glass;
use gpui::{
    Animation, AnimationElement, AnimationExt, Div, ElementId, Hsla, IntoElement, ParentElement,
    SpringAnimation, SpringConfig, Styled, div, ease_out_quint, pulsating_between, px, relative,
    sampled_easing,
};
use std::time::Duration;

/// Entrances: cards, blocks, rows.
pub const BASE: Duration = Duration::from_millis(200);
/// Whole views.
pub const VIEW: Duration = Duration::from_millis(180);
/// Between siblings entering together.
pub const STAGGER: Duration = Duration::from_millis(35);
/// How far an entering element rises.
const RISE: f32 = 6.;

/// Fade in and rise into place; `order` staggers siblings (capped, so a long
/// list never makes the last item wait).
pub fn enter<E>(el: E, id: impl Into<ElementId>, order: usize) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    let hold = (STAGGER * order.min(10) as u32).max(Duration::from_millis(1));
    el.with_animations(
        id,
        vec![
            Animation::new(hold),
            Animation::new(BASE).with_easing(ease_out_quint()),
        ],
        |el, phase, v| {
            if phase == 0 {
                el.opacity(0.).relative().top(px(RISE))
            } else {
                el.opacity(v).relative().top(px(RISE * (1. - v)))
            }
        },
    )
}

/// A whole view fading in when it replaces another (no rise: views fill the
/// window, and a moving edge would show the canvas behind).
pub fn view<E>(el: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    el.with_animation(
        id,
        Animation::new(VIEW).with_easing(ease_out_quint()),
        |el, v| el.opacity(0.35 + 0.65 * v),
    )
}

/// A status dot that breathes while something is alive: a steady core and a
/// halo whose glow rises and falls (1.6 s, like a resting pulse).
pub fn live_dot(color: Hsla, id: impl Into<ElementId>, g: &Glass) -> Div {
    div()
        .relative()
        .size(px(8.))
        .flex_none()
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded_full()
                .bg(color)
                .shadow(vec![g.glow(color, 12.)])
                .with_animation(
                    id,
                    Animation::new(Duration::from_millis(1600))
                        .repeat()
                        .with_easing(pulsating_between(0.25, 1.)),
                    |el, v| el.opacity(v),
                ),
        )
        .child(div().absolute().inset_0().rounded_full().bg(color))
}

/// A thin progress line whose fill glides to `done` on a spring, so steps
/// read as momentum instead of jumps.
pub fn progress(done: f32, color: Hsla, id: impl Into<ElementId>, g: &Glass) -> Div {
    div().w_full().h(px(2.)).rounded_full().bg(g.line).child(
        div()
            .h_full()
            .rounded_full()
            .bg(color)
            .shadow(vec![g.glow(color, 6.)])
            .with_spring(
                id,
                SpringAnimation::new(SpringConfig::new(170., 26., 1.)).to(done.clamp(0., 1.)),
                |el, v| el.w(relative(v.clamp(0., 1.))),
            ),
    )
}

/// A springy arrival for a result: grows from 70 % with a little overshoot.
/// `size` is the final size; the box keeps it, so nothing around moves.
pub fn pop(child: Div, size: f32, id: impl Into<ElementId>) -> Div {
    let (duration, easing) = sampled_easing(SpringConfig::new(260., 15., 1.), 0.001);
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(child.with_animation(
            id,
            Animation::new(duration).with_easing(easing),
            move |el, v| {
                let s = size * (0.7 + 0.3 * v);
                el.size(px(s)).opacity(v.clamp(0., 1.))
            },
        ))
}
