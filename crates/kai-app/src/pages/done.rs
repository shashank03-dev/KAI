//! After a run: the result in plain words, the numbers, and what to do next.

use crate::app::KaiApp;
use crate::i18n::t;
use crate::pages::running;
use crate::ui::{self, Glass};
use ely_gpui_component::{
    agent::{ComputerUseViewer, HumanInputRequest, ToolCallCard},
    buttons::{Button, ButtonVariant},
    chat::FollowUpSuggestions,
    documents::MarkdownRenderer,
    feedback::ErrorView,
    layout::ScrollArea,
    motion::Confetti,
    primitives::{Icon, IconName},
    theme::{ActiveTheme, IconSize},
};
use gpui::{Context, FontWeight, IntoElement, ParentElement, Styled, Window, div, px};
use kai_core::{Action, Outcome};

fn reason(outcome: Option<Outcome>) -> &'static str {
    match outcome {
        Some(Outcome::Success) => "Kai finished the task.",
        Some(Outcome::Stopped) => "You stopped Kai.",
        Some(Outcome::StepLimit) => {
            "Kai hit the step limit before finishing. Raise it in Settings › Safety, or break the task into smaller parts."
        }
        Some(Outcome::TimeLimit) => {
            "Kai hit the time limit before finishing. Raise it in Settings › Safety."
        }
        Some(Outcome::AskedUser) => "Kai needs your input to go on.",
        Some(Outcome::Failed) | None => "Kai couldn't make progress on this screen.",
    }
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let r = app.run.read(cx);
    let outcome = r.outcome();
    let ok = outcome == Some(Outcome::Success) && r.error.is_none();
    let steps = r
        .trace
        .as_ref()
        .map(|t| t.steps.len())
        .unwrap_or(r.steps.len());
    let secs = r.elapsed.as_secs_f64();
    let per_step = if steps > 0 { secs / steps as f64 } else { 0.0 };
    let task = r.task.clone();

    let g = Glass::of(cx);
    let run_key = app.selected.unwrap_or_default();
    let colors = &cx.theme().colors;
    let stopped = outcome == Some(Outcome::Stopped) && r.error.is_none();
    let (tone, icon) = if ok {
        (colors.success, IconName::Check)
    } else if stopped {
        (colors.warning, IconName::CircleStop)
    } else {
        (colors.danger, IconName::X)
    };
    // The result as a hero: a glass orb lit in the outcome's colour.
    let mut tint = tone;
    tint.a = 0.14;
    let hero = div()
        .flex()
        .items_center()
        .gap(px(20.))
        .child(crate::anim::pop(
            div()
                .size(px(64.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(tint)
                .border_1()
                .border_color(tone)
                .shadow(vec![g.sheen_shadow(), g.glow(tone, 28.)])
                .child(Icon::new(icon).size(IconSize::Lg).color(tone)),
            64.,
            gpui::SharedString::from(format!("result-pop-{}", app.selected.unwrap_or_default())),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .flex_1()
                .min_w_0()
                .child(ui::eyebrow(
                    if ok {
                        "Finished".to_string()
                    } else {
                        outcome
                            .map(crate::state::outcome_text)
                            .unwrap_or("Ended")
                            .to_string()
                    },
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(26.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(g.text)
                        .child(if ok {
                            t(cx, "done.success", &[])
                        } else {
                            t(cx, "done.failure", &[])
                        }),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(g.dim)
                        .child(reason(outcome)),
                ),
        );
    let result = r.error.as_ref().map(|e| {
        ErrorView::new("result", IconName::TriangleAlert, "What went wrong").detail(e.clone())
    });

    // When Kai asked something, the last step's text is the question.
    let question = r
        .trace
        .as_ref()
        .and_then(|tr| tr.steps.last())
        .filter(|s| s.decision.action == Action::AskUser)
        .map(|s| {
            s.decision
                .text
                .clone()
                .filter(|q| !q.is_empty())
                .unwrap_or_else(|| s.decision.thought.clone())
        });
    let ask = question.map(|q| {
        let (again, task) = (cx.entity(), task.clone());
        let qq = q.clone();
        HumanInputRequest::new("ask", q, &app.pages.answer, move |answer, window, cx| {
            let next = format!("{task}\n\nYou asked: {qq}\nThe user answered: {answer}");
            again.update(cx, |a, cx| a.start_run(Some(next), window, cx))
        })
    });

    // On failure, show the step that failed and the frame it saw.
    let failed = (!ok).then(|| {
        let step = r.steps.last().map(|s| {
            ToolCallCard::new("failed-step", s.title.clone(), s.state)
                .summary(s.thought.clone())
                .arguments(s.json.clone())
        });
        let frame = r.frames.last().map(|f| {
            ComputerUseViewer::new("failed-frame", f.to_string_lossy().to_string())
                .action(r.caption.clone())
        });
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(step)
            .children(frame)
    });

    let report = r
        .report
        .clone()
        .map(|md| MarkdownRenderer::new("report", md));
    let (again, save, trace, fresh) = (cx.entity(), cx.entity(), cx.entity(), cx.entity());
    let again_task = task.clone();
    let follow_task = task.clone();
    let follow = cx.entity();

    let tile = |label: &str, value: String| {
        ui::card(&g)
            .flex_1()
            .min_w(px(140.))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::eyebrow(label.to_string(), cx))
            .child(
                div()
                    .font_family(cx.theme().mono_family.clone())
                    .text_size(px(24.))
                    .text_color(g.text)
                    .child(value),
            )
    };
    let stats = div()
        .flex()
        .flex_wrap()
        .gap(px(10.))
        .child(tile("Steps", steps.to_string()))
        .child(tile("Total time", format!("{secs:.1}s")))
        .child(tile("Per step", format!("{per_step:.2}s")));

    let actions = div()
        .flex()
        .flex_wrap()
        .gap_2()
        .child(
            Button::new("save-skill", t(cx, "done.save_skill", &[]))
                .variant(ButtonVariant::Primary)
                .icon(IconName::Save)
                .disabled(!ok)
                .on_click(move |_, _, cx| save.update(cx, crate::pages::skills::open_draft)),
        )
        .child(
            Button::new("again", t(cx, "done.again", &[]))
                .variant(ButtonVariant::Secondary)
                .icon(IconName::Play)
                .on_click(move |_, window, cx| {
                    again.update(cx, |a, cx| match a.selected {
                        Some(id) => a.retry(id, window, cx),
                        None => a.start_run(Some(again_task.clone()), window, cx),
                    })
                }),
        )
        .child(
            Button::new("trace", t(cx, "done.trace", &[]))
                .variant(ButtonVariant::Secondary)
                .icon(IconName::History)
                .on_click(move |_, _, cx| {
                    trace.update(cx, |a, cx| {
                        a.pages.reload_history();
                        a.pages.open_trace = a.pages.history.first().map(|e| e.dir.clone());
                        a.go(crate::app::Page::History, cx)
                    })
                }),
        )
        .child(
            Button::new("new", "New task")
                .variant(ButtonVariant::Ghost)
                .icon(IconName::Plus)
                .on_click(move |_, _, cx| fresh.update(cx, |a, cx| a.select(None, cx))),
        );

    let follow_ups = ok.then(|| {
        FollowUpSuggestions::new(
            "follow",
            [
                "Do it again for another folder",
                "Explain what you did",
                "Undo the last change",
            ],
            move |ix, window, cx| {
                let next = match ix {
                    0 => format!(
                        "Do the same as before for a different folder I'll name: {follow_task}"
                    ),
                    1 => format!("Explain in plain words what you did for: {follow_task}"),
                    _ => format!("Undo the last change you made for: {follow_task}"),
                };
                follow.update(cx, |a, cx| a.start_run(Some(next), window, cx))
            },
        )
    });

    ScrollArea::new("done").size_full().vertical().child(
        div()
            .relative()
            .px(px(32.))
            .py(px(28.))
            .max_w(px(920.))
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(ely_gpui_component::agent::AgentStatus::new(
                        "done-status",
                        running::agent_state(r),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .text_color(g.dim)
                            .truncate()
                            .child(task),
                    ),
            )
            .child(hero)
            .children(result)
            .children(ask)
            .child(crate::anim::enter(
                stats,
                gpui::SharedString::from(format!("done-stats-{run_key}")),
                2,
            ))
            .children(report)
            .child(crate::anim::enter(
                actions,
                gpui::SharedString::from(format!("done-actions-{run_key}")),
                3,
            ))
            .child(ui::hints(
                &[
                    (&[ui::MOD_KEY, "R"], "run again"),
                    (&[ui::MOD_KEY, "N"], "new task"),
                    (&[ui::ALT_KEY, "↑↓"], "other tasks"),
                ],
                cx,
            ))
            .children(follow_ups)
            .children(failed)
            .children((!r.outputs.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(ui::eyebrow("Terminal", cx))
                    .child(crate::pages::panes::terminal(r, cx))
            }))
            .children((!r.team_view.subs.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(ui::eyebrow("Sub-agents", cx))
                    .child(crate::pages::panes::team_tree(r, cx))
            }))
            .children(ok.then(|| Confetti::new("confetti", 1))),
    )
}
