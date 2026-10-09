//! After a run: the result in plain words, the numbers, and what to do next.

use crate::app::KaiApp;
use crate::i18n::t;
use crate::pages::running;
use crate::state::Phase;
use ely_gpui_component::{
    agent::{ComputerUseViewer, HumanInputRequest, ToolCallCard},
    buttons::{Button, ButtonVariant},
    chat::FollowUpSuggestions,
    data_display::Statistic,
    documents::MarkdownRenderer,
    feedback::{ErrorView, ResultView},
    layout::ScrollArea,
    motion::Confetti,
    primitives::IconName,
    theme::{ActiveTheme, TextSize},
};
use gpui::{Context, IntoElement, ParentElement, Styled, Window, div};
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
    let theme = cx.theme();
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

    let result = if ok {
        ResultView::success("result", t(cx, "done.success", &[])).into_any_element()
    } else if let Some(e) = &r.error {
        ErrorView::new(
            "result",
            IconName::TriangleAlert,
            t(cx, "done.failure", &[]),
        )
        .detail(e.clone())
        .into_any_element()
    } else {
        ResultView::failure("result", t(cx, "done.failure", &[])).into_any_element()
    };

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

    let stats = div()
        .flex()
        .flex_wrap()
        .gap_6()
        .child(Statistic::new("steps", "Steps", steps as f64))
        .child(
            Statistic::new("time", "Total time", secs)
                .decimals(1)
                .suffix(" s"),
        )
        .child(
            Statistic::new("per-step", "Seconds per step", per_step)
                .decimals(2)
                .suffix(" s"),
        );

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
                    let task = again_task.clone();
                    again.update(cx, |a, cx| a.start_run(Some(task), window, cx))
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
                .on_click(move |_, _, cx| {
                    fresh.update(cx, |a, cx| {
                        a.run.update(cx, |r, cx| {
                            r.phase = Phase::Idle;
                            cx.notify();
                        });
                        cx.notify()
                    })
                }),
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

    ScrollArea::new("done").vertical().child(
        div()
            .relative()
            .p_6()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(ely_gpui_component::agent::AgentStatus::new(
                        "done-status",
                        running::agent_state(r),
                    ))
                    .child(div().text_size(theme.text_size(TextSize::Md)).child(task)),
            )
            .child(result)
            .child(
                div()
                    .text_color(theme.colors.fg_muted)
                    .child(reason(outcome)),
            )
            .children(ask)
            .child(stats)
            .children(report)
            .child(actions)
            .children(follow_ups)
            .children(failed)
            .children(ok.then(|| Confetti::new("confetti", 1))),
    )
}
