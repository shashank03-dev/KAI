//! The live run: steps on the left, what Kai sees on the right.

use crate::app::KaiApp;
use crate::i18n::t;
use crate::state::{Phase, RunState, StepView};
use ely_gpui_component::{
    agent::{AgentPlan, AgentState, AgentStatus, BrowserPreview, ComputerUseViewer, ToolCallCard},
    chat::{CostEstimator, StepState, ThinkingIndicator, TokenCounter, stop_button},
    data_display::UsageBar,
    layout::{Panel, ScrollArea, SplitPane},
    theme::{ActiveTheme, TextSize},
};
use gpui::{Axis, Context, IntoElement, ParentElement, Styled, Window, div, point, px};
use std::time::Duration;

/// Context a step costs, roughly: the stable prompt plus one screenshot.
const TOKENS_PER_STEP: usize = 1_400;
const CONTEXT_WINDOW: f64 = 16_384.0;

pub fn agent_state(r: &RunState) -> AgentState {
    match r.phase {
        Phase::Waiting => AgentState::Waiting,
        Phase::Done if r.error.is_some() => AgentState::Failed,
        Phase::Done => match r.outcome() {
            Some(kai_core::Outcome::Success) => AgentState::Done,
            _ => AgentState::Failed,
        },
        _ if r.team && r.steps.is_empty() => AgentState::Planning,
        _ => AgentState::Working,
    }
}

fn card(ix: usize, s: &StepView) -> impl IntoElement {
    let mut c =
        ToolCallCard::new(("step", ix), s.title.clone(), s.state).summary(s.thought.clone());
    if let Some(tm) = s.timings {
        c = c.took(Duration::from_millis(tm.total_ms())).result(format!(
            "capture {} ms · tree {} ms · encode {} ms · model {} ms · act {} ms",
            tm.capture_ms, tm.tree_ms, tm.encode_ms, tm.model_ms, tm.act_ms
        ));
    }
    if !s.json.is_empty() {
        c = c.arguments(s.json.clone());
    }
    c
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let theme = cx.theme();
    let r = app.run.read(cx);
    let state = agent_state(r);
    let step_no = r.current_step().max(1);
    let stop = cx.entity();
    let pick = app.run.clone();

    let header = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_3()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(theme.colors.border)
        .child(AgentStatus::new("run-status", state))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(theme.text_size(TextSize::Md))
                .child(r.task.clone()),
        )
        .child(
            div()
                .text_size(theme.text_size(TextSize::Sm))
                .text_color(theme.colors.fg_muted)
                .child(t(
                    cx,
                    "run.step",
                    &[
                        ("n", &step_no.to_string()),
                        ("max", &r.max_steps.to_string()),
                    ],
                )),
        )
        .child(
            stop_button("run-stop").on_click(move |_, _, cx| stop.update(cx, |a, cx| a.stop(cx))),
        );

    let plan = r.plan.as_ref().map(|p| {
        let done = r.steps.len();
        AgentPlan::new(
            "plan",
            "Sub-agents",
            p.subtasks.iter().enumerate().map(|(i, s)| {
                let state = if i < done {
                    StepState::Done
                } else {
                    StepState::Working
                };
                (format!("{} · {:?}", s.goal, s.kind), state)
            }),
        )
    });

    let left = ScrollArea::new("steps").vertical().child(
        div()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .children(plan)
            .children(r.steps.iter().enumerate().map(|(i, s)| card(i, s)))
            .children(
                r.thinking
                    .then(|| ThinkingIndicator::new("thinking").label(t(cx, "run.thinking", &[]))),
            ),
    );

    let viewer = match r.shown.and_then(|i| r.frames.get(i)) {
        Some(frame) => {
            let mut v = ComputerUseViewer::new("viewer", frame.to_string_lossy().to_string())
                .action(r.caption.clone());
            if let Some(s) = r.screen {
                v = v.ratio(s.w / s.h.max(1.0));
            }
            if let (Some((x, y)), true) = (r.pointer, r.shown == Some(r.frames.len() - 1)) {
                v = v.pointer(point(x, y));
            }
            v.into_any_element()
        }
        None => div()
            .h(px(240.))
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.colors.fg_muted)
            .child("Kai's first look at your screen will appear here.")
            .into_any_element(),
    };
    let strip = (r.frames.len() > 1).then(|| {
        let frames: Vec<String> = r
            .frames
            .iter()
            .map(|f| f.to_string_lossy().to_string())
            .collect();
        BrowserPreview::new("frames", "Earlier frames", frames, r.shown.unwrap_or(0)).on_show(
            move |ix, _, cx| {
                pick.update(cx, |r, cx| {
                    r.shown = Some(ix);
                    cx.notify();
                })
            },
        )
    });

    let tokens = r.steps.len() * TOKENS_PER_STEP;
    let hours = r.elapsed.as_secs_f64() / 3600.0;
    let droplet = app.settings.droplet_hourly_usd as f64;
    let meters = Panel::new("Usage").child(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(TokenCounter::new(tokens).limit(CONTEXT_WINDOW as usize))
            .child(UsageBar::new(CONTEXT_WINDOW).part(
                "This step's context",
                (TOKENS_PER_STEP as f64).min(CONTEXT_WINDOW),
            ))
            .child(CostEstimator::new(
                "cost",
                tokens,
                r.steps.len() * 40,
                (droplet * hours / tokens.max(1) as f64 * 1_000_000.0, 0.0),
            ))
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Xs))
                    .text_color(theme.colors.fg_muted)
                    .child(format!(
                        "Droplet time so far: {:.1} min at ${droplet:.2}/h",
                        r.elapsed.as_secs_f64() / 60.0
                    )),
            ),
    );

    let right = ScrollArea::new("seen").vertical().child(
        div()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(viewer)
            .children(strip)
            .child(meters),
    );

    div().size_full().flex().flex_col().child(header).child(
        div().flex_1().min_h_0().child(
            SplitPane::new("run-split", Axis::Horizontal, px(280.))
                .sizes(&[0.42, 0.58])
                .pane(left)
                .pane(right),
        ),
    )
}
