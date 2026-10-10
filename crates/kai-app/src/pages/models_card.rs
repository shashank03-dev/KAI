//! One model on the Models page: what it is, how well it fits this computer
//! and why, and the one or two things you can do with it now. The Best card
//! is the visually primary one with a single "Download and use" button.

use crate::app::KaiApp;
use crate::local::{LocalState, bytes_text, fraction, stage_text};
use crate::local_api::{Fit, ModelEntry, Ranked};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    data_display::{Badge, Tone},
    feedback::ErrorView,
    motion::ProgressBar,
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, TextSize},
};
use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, ParentElement, SharedString, Styled, div,
};

pub fn fit_badge(fit: Fit) -> Badge {
    match fit {
        Fit::Best => Badge::new("Best for this computer").tone(Tone::Accent),
        Fit::Good => Badge::new("Good").tone(Tone::Success),
        Fit::Slow => Badge::new("Slow").tone(Tone::Warning),
        Fit::TooBig => Badge::new("Too big").tone(Tone::Danger),
    }
}

fn num(b: f32) -> String {
    if b.fract().abs() < 0.05 {
        format!("{b:.0}B")
    } else {
        format!("{b:.1}B")
    }
}

/// "Google · 8B (4B active) · 5.0 GB download · Apache-2.0".
pub fn meta(e: &ModelEntry) -> String {
    let params = match e.active_params_b {
        Some(a) => format!("{} ({} active)", num(e.params_b), num(a)),
        None => num(e.params_b),
    };
    format!(
        "{} · {params} · {:.1} GB download · {}",
        e.maker, e.download_gb, e.license
    )
}

fn computer_use(e: &ModelEntry) -> Badge {
    let tone = match e.computer_use.as_str() {
        "recommended" => Tone::Success,
        "experimental" => Tone::Warning,
        _ => Tone::Neutral,
    };
    Badge::new(format!("Computer use: {}", e.computer_use)).tone(tone)
}

fn id(prefix: &str, model: &str) -> SharedString {
    format!("{prefix}-{model}").into()
}

/// The download/start progress with its Cancel button.
fn progress(l: &LocalState, me: &Entity<KaiApp>, cx: &Context<KaiApp>) -> Option<AnyElement> {
    let job = l.job.as_ref()?;
    let theme = cx.theme();
    let f = (!job.starting).then(|| fraction(&job.progress)).flatten();
    let bar = match f {
        Some(f) => ProgressBar::new(id("bar", &job.id), f),
        None => ProgressBar::indeterminate(id("bar", &job.id)),
    };
    let me = me.clone();
    Some(
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap_3()
                    .text_size(theme.text_size(TextSize::Sm))
                    .child(div().child(stage_text(&job.progress, job.starting)))
                    .child(
                        div()
                            .text_color(theme.colors.fg_muted)
                            .child(if job.starting {
                                String::new()
                            } else {
                                bytes_text(&job.progress)
                            }),
                    ),
            )
            .child(bar)
            .child(
                div().flex().justify_end().child(
                    Button::new(id("cancel", &job.id), "Cancel")
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .icon(IconName::X)
                        .on_click(move |_, _, cx| me.update(cx, |a, cx| a.local_cancel(cx))),
                ),
            )
            .into_any_element(),
    )
}

fn failure(l: &LocalState, me: &Entity<KaiApp>, model: &str) -> Option<AnyElement> {
    let f = l.failure.as_ref().filter(|f| f.id == model)?;
    let what = if f.steps.is_empty() {
        "Kai couldn't remove the model"
    } else if f.steps.contains(&crate::local::Step::Install) {
        "The download didn't finish"
    } else {
        "The local server didn't start"
    };
    let me = me.clone();
    let retry = (!f.steps.is_empty()).then(|| {
        Button::new(id("retry", model), "Try again")
            .variant(ButtonVariant::Primary)
            .icon(IconName::RotateCcw)
            .on_click(move |_, _, cx| me.update(cx, |a, cx| a.local_retry(cx)))
    });
    let mut view = ErrorView::new(id("err", model), IconName::TriangleAlert, what)
        .body("Nothing was changed. Check your connection and free disk space, then try again.")
        .detail(f.message.clone());
    if let Some(r) = retry {
        view = view.action(r);
    }
    Some(view.into_any_element())
}

fn actions(r: &Ranked, l: &LocalState, in_use: bool, me: &Entity<KaiApp>) -> AnyElement {
    let e = &r.entry;
    let (busy, best) = (l.job.is_some(), r.fit == Fit::Best);
    let row = div().flex().flex_wrap().items_center().gap_2();
    if r.fit == Fit::TooBig {
        return row
            .child(
                Button::new(id("too-big", &e.id), "Too big for this computer")
                    .icon(IconName::Ban)
                    .disabled(true),
            )
            .into_any_element();
    }
    let click = |f: fn(&mut KaiApp, &str, &mut Context<KaiApp>)| {
        let (me, model) = (me.clone(), e.id.clone());
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
            me.update(cx, |a, cx| f(a, &model, cx))
        }
    };
    if !l.is_installed(&e.id) {
        let b = if best {
            Button::new(id("get-use", &e.id), "Download and use")
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Lg)
                .on_click(click(|a, m, cx| a.local_get_and_use(m, cx)))
        } else {
            Button::new(id("get", &e.id), "Download").on_click(click(|a, m, cx| {
                a.local_job(m, vec![crate::local::Step::Install], cx)
            }))
        };
        return row
            .child(b.icon(IconName::Download).disabled(busy))
            .into_any_element();
    }
    let running = l.running(&e.id);
    let use_btn = (!in_use).then(|| {
        Button::new(id("use", &e.id), "Use")
            .variant(if best {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            })
            .icon(IconName::Check)
            .disabled(busy)
            .on_click(click(|a, m, cx| a.local_use(m, cx)))
    });
    let start_stop = if running {
        Button::new(id("stop", &e.id), "Stop")
            .icon(IconName::Square)
            .variant(ButtonVariant::Outline)
            .disabled(busy)
            .on_click(click(|a, _, cx| a.local_stop(cx)))
    } else {
        Button::new(id("start", &e.id), "Start")
            .icon(IconName::Play)
            .variant(ButtonVariant::Outline)
            .disabled(busy)
            .on_click(click(|a, m, cx| {
                a.local_job(m, vec![crate::local::Step::Start], cx)
            }))
    };
    row.children(use_btn)
        .children(in_use.then(|| Badge::new("Kai uses this model").tone(Tone::Success)))
        .child(start_stop)
        .child(
            Button::new(id("remove", &e.id), "Remove")
                .icon(IconName::Trash2)
                .variant(ButtonVariant::Ghost)
                .disabled(busy || running)
                .on_click(click(|a, m, cx| a.local_remove(m, cx))),
        )
        .into_any_element()
}

pub fn card(r: &Ranked, l: &LocalState, in_use: bool, cx: &Context<KaiApp>) -> AnyElement {
    let me = cx.entity();
    let theme = cx.theme();
    let e = &r.entry;
    let best = r.fit == Fit::Best;
    let (reason_icon, reason_color) = match r.fit {
        Fit::Best | Fit::Good => (IconName::CircleCheck, theme.colors.success),
        Fit::Slow => (IconName::Gauge, theme.colors.warning),
        Fit::TooBig => (IconName::Ban, theme.colors.danger),
    };
    let mine = l.job.as_ref().is_some_and(|j| j.id == e.id);
    let body = if mine {
        progress(l, &me, cx)
    } else {
        failure(l, &me, &e.id)
    }
    .unwrap_or_else(|| actions(r, l, in_use, &me));
    let muted = if r.fit == Fit::TooBig {
        theme.colors.fg_subtle
    } else {
        theme.colors.fg
    };
    div()
        .p(if best {
            gpui::rems(1.5)
        } else {
            gpui::rems(1.0)
        })
        .flex()
        .flex_col()
        .gap_2()
        .rounded_lg()
        .border(if best { gpui::px(2.) } else { gpui::px(1.) })
        .border_color(if best {
            theme.colors.accent
        } else {
            theme.colors.border
        })
        .bg(theme.colors.surface)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(muted)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(theme.text_size(if best { TextSize::Xl } else { TextSize::Md }))
                        .child(e.label.clone()),
                )
                .child(fit_badge(r.fit)),
        )
        .child(
            div()
                .text_size(theme.text_size(TextSize::Sm))
                .text_color(theme.colors.fg_muted)
                .child(meta(e)),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(computer_use(e))
                .children(e.vision.then(|| Badge::new("Sees images").tone(Tone::Info)))
                .children(l.is_installed(&e.id).then(|| Badge::new("Downloaded")))
                .children(
                    l.running(&e.id)
                        .then(|| Badge::new("Running").tone(Tone::Success).dot()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(theme.text_size(TextSize::Sm))
                .child(Icon::new(reason_icon).color(reason_color))
                .child(div().flex_1().min_w_0().child(r.reason.clone())),
        )
        .child(div().pt_1().child(body))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_api::load_catalog;

    #[test]
    fn meta_names_size_and_licence() {
        let e = &load_catalog().models[0];
        let m = meta(e);
        assert!(m.contains("active"), "{m}");
        assert!(m.contains("GB download"));
        assert!(m.ends_with(&e.license));
        assert_eq!(num(7.0), "7B");
        assert_eq!(num(5.1), "5.1B");
    }
}
