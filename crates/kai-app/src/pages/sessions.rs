//! The sessions list beside the run view: every run, queued, running or done,
//! newest at the bottom like a queue, with a button for a new task.

use crate::app::KaiApp;
use crate::state::{Phase, RunState};
use crate::ui::{self, Glass, R_SM};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::InlineMessage,
    layout::ScrollArea,
    primitives::{Icon, IconName, Severity},
    theme::{ActiveTheme, ControlSize, IconSize, TextSize},
};
use gpui::{
    AnyElement, Context, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};

/// The row's icon, colour and second line.
fn status(r: &RunState, place: usize, cx: &Context<KaiApp>) -> (IconName, Hsla, String) {
    let c = &cx.theme().colors;
    match r.phase {
        Phase::Idle => (IconName::Circle, c.fg_muted, String::new()),
        Phase::Queued => (
            IconName::Hourglass,
            c.fg_muted,
            if place == 0 {
                "Queued · next".to_string()
            } else {
                format!("Queued · {place} ahead")
            },
        ),
        Phase::Running if r.held => (IconName::Pause, c.warning, "Paused".into()),
        Phase::Running => (
            IconName::LoaderCircle,
            c.success,
            format!("Running · step {}", r.current_step().max(1)),
        ),
        Phase::Waiting => (IconName::Hand, c.warning, "Waiting for you".into()),
        Phase::Done if r.error.is_some() => (
            IconName::CircleX,
            c.danger,
            r.error.clone().unwrap_or_default(),
        ),
        Phase::Done => match r.outcome() {
            Some(kai_core::Outcome::Success) => (
                IconName::CircleCheck,
                c.success,
                format!("Done · {} steps", r.steps.len()),
            ),
            Some(o @ (kai_core::Outcome::Stopped | kai_core::Outcome::AskedUser)) => (
                IconName::CircleStop,
                c.warning,
                crate::state::outcome_text(o).to_string(),
            ),
            Some(o) => (
                IconName::CircleX,
                c.danger,
                crate::state::outcome_text(o).to_string(),
            ),
            None => (IconName::CircleStop, c.fg_muted, "Ended".into()),
        },
    }
}

/// Runs before `id` that are queued or have the screen.
fn ahead(app: &KaiApp, id: u64, cx: &Context<KaiApp>) -> usize {
    app.runs
        .iter()
        .take_while(|e| e.id != id)
        .filter(|e| {
            let r = e.state.read(cx);
            r.phase == Phase::Queued || r.is_live()
        })
        .count()
}

pub fn sidebar(app: &KaiApp, _: &mut Window, cx: &Context<KaiApp>) -> AnyElement {
    let g = Glass::of(cx);
    let fresh = cx.entity();
    let rows: Vec<AnyElement> = app
        .runs
        .iter()
        .map(|e| {
            let r = e.state.read(cx);
            let (icon, color, line) = status(r, ahead(app, e.id, cx), cx);
            let live = r.is_live() && !r.held;
            let picker = cx.entity();
            let id = e.id;
            let selected = app.selected == Some(id);
            let (hover, ember) = (g.card_hover, g.ember);
            let row = div()
                .id(("session", id as usize))
                .relative()
                .px(px(10.))
                .py(px(9.))
                .flex()
                .items_start()
                .gap(px(10.))
                .cursor_pointer()
                .rounded(px(R_SM + 2.))
                .border_1()
                .on_click(move |_, _, cx| picker.update(cx, |a, cx| a.select(Some(id), cx)));
            let row = if selected {
                row.bg(g.raised)
                    .border_color(g.line_strong)
                    .shadow(vec![g.sheen_shadow()])
            } else {
                row.border_color(gpui::transparent_black())
                    .hover(move |s| s.bg(hover))
            };
            row.child(div().pt(px(2.)).flex_none().child(if live {
                div()
                    .pt(px(4.))
                    .child(ui::dot(color, true, &g))
                    .into_any_element()
            } else {
                Icon::new(icon)
                    .size(IconSize::Sm)
                    .color(color)
                    .into_any_element()
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(if selected { g.text } else { g.dim })
                            .truncate()
                            .child(r.task.clone()),
                    )
                    .child(
                        ui::mono(
                            if r.team {
                                format!("{line} · team")
                            } else {
                                line
                            },
                            if live { ember } else { g.faint },
                            cx,
                        )
                        .text_size(px(11.))
                        .truncate(),
                    ),
            )
            .into_any_element()
        })
        .collect();
    let any_done = app
        .runs
        .iter()
        .any(|e| e.state.read(cx).phase == Phase::Done);
    let held = app.queue_held.then(|| {
        let resumer = cx.entity();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(InlineMessage::new(
                Severity::Warning,
                "Queue paused after a stop.",
            ))
            .child(
                Button::new("resume-queue", "Resume queue")
                    .variant(ButtonVariant::Secondary)
                    .icon(IconName::Play)
                    .on_click(move |_, window, cx| {
                        resumer.update(cx, |a, cx| a.resume_queue(window, cx))
                    }),
            )
    });
    let clear = any_done.then(|| {
        let clearer = cx.entity();
        Button::new("clear-finished", "Clear finished")
            .variant(ButtonVariant::Ghost)
            .icon(IconName::Trash2)
            .on_click(move |_, _, cx| clearer.update(cx, |a, cx| a.clear_finished(cx)))
    });
    div()
        .w(px(256.))
        .flex_none()
        .h_full()
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(g.line)
        .bg(g.pane)
        .child(
            div()
                .px(px(14.))
                .pt(px(14.))
                .pb(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .child(ui::eyebrow(format!("Tasks · {}", app.runs.len()), cx))
                .child(
                    Button::new("new-task", "New")
                        .variant(if app.selected.is_none() {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Primary
                        })
                        .size(ControlSize::Sm)
                        .icon(IconName::Plus)
                        .on_click(move |_, _, cx| fresh.update(cx, |a, cx| a.select(None, cx))),
                ),
        )
        .children(held.map(|h| div().px_3().pb_3().child(h)))
        .child(
            div().flex_1().min_h_0().child(
                ScrollArea::new("sessions").vertical().child(
                    div()
                        .px(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .children(rows),
                ),
            ),
        )
        .children(clear.map(|c| div().p_2().border_t_1().border_color(g.line).child(c)))
        .into_any_element()
}

/// A run waiting for the screen.
pub fn queued(app: &KaiApp, cx: &Context<KaiApp>) -> AnyElement {
    let theme = cx.theme();
    let id = app.selected.unwrap_or_default();
    let r = app.run.read(cx);
    let ahead = ahead(app, id, cx);
    let canceller = cx.entity();
    div()
        .size_full()
        .p_6()
        .flex()
        .flex_col()
        .gap_4()
        .child(div().text_size(theme.text_size(TextSize::Lg)).child(r.task.clone()))
        .child(InlineMessage::new(
            Severity::Info,
            format!(
                "Queued. Kai runs one task at a time because tasks share your mouse and keyboard. {}",
                match (ahead, app.queue_held) {
                    (_, true) => "The queue is paused after a stop; resume it in the task list.".to_string(),
                    (0, _) => "This one starts next.".to_string(),
                    (1, _) => "1 task is ahead of it.".to_string(),
                    (n, _) => format!("{n} tasks are ahead of it."),
                }
            ),
        ))
        .child(
            div()
                .text_size(theme.text_size(TextSize::Sm))
                .text_color(theme.colors.fg_muted)
                .child(format!("Will use {}", r.model)),
        )
        .child(
            div().flex().child(
                Button::new("cancel-queued", "Cancel")
                    .variant(ButtonVariant::Secondary)
                    .icon(IconName::X)
                    .on_click(move |_, _, cx| canceller.update(cx, |a, cx| a.stop_run(id, cx))),
            ),
        )
        .into_any_element()
}
