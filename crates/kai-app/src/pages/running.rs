//! The live run: the transcript, terminal and sub-agents on the left with a
//! composer to steer the agent, what Kai sees on the right, and the run's
//! controls (pause, resume, stop) in the header.

use crate::app::KaiApp;
use crate::i18n::t;
use crate::pages::panes;
use crate::state::{Phase, RunState};
use crate::ui::{self, Glass};
use ely_gpui_component::{
    agent::{AgentPlan, AgentState, AgentStatus, BrowserPreview, ComputerUseViewer},
    buttons::{Button, ButtonVariant},
    chat::{CostEstimator, PromptInput, StepState, ThinkingIndicator, TokenCounter, stop_button},
    data_display::{Badge, Tone, UsageBar},
    forms::Choice,
    layout::{Panel, ScrollArea, SplitPane},
    navigation::Tabs,
    primitives::IconName,
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    AnyElement, Axis, Context, FontWeight, IntoElement, ParentElement, Styled, Window, div, point,
    px,
};

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
        _ if r.paused => AgentState::Waiting,
        _ if r.team && r.steps.is_empty() => AgentState::Planning,
        _ => AgentState::Working,
    }
}

/// Pause / Resume and Stop for one run.
pub fn controls(id: u64, r: &RunState, cx: &Context<KaiApp>) -> impl IntoElement {
    let (pauser, stopper) = (cx.entity(), cx.entity());
    let paused = r.paused;
    let pause = Button::new(
        ("run-pause", id as usize),
        if paused { "Resume" } else { "Pause" },
    )
    .variant(ButtonVariant::Secondary)
    .icon(if paused {
        IconName::Play
    } else {
        IconName::Pause
    })
    .on_click(move |_, _, cx| pauser.update(cx, |a, cx| a.set_paused(id, !paused, cx)));
    div().flex().items_center().gap_2().child(pause).child(
        stop_button(("run-stop", id as usize))
            .on_click(move |_, _, cx| stopper.update(cx, |a, cx| a.stop_run(id, cx))),
    )
}

fn plan(r: &RunState) -> Option<AgentPlan> {
    r.plan.as_ref().map(|p| {
        AgentPlan::new(
            "plan",
            "Sub-agents",
            p.subtasks.iter().map(|s| {
                let state = r
                    .team_view
                    .subs
                    .iter()
                    .find(|v| v.id == s.id)
                    .map(|v| match v.state {
                        crate::team::SubState::Waiting => StepState::Waiting,
                        crate::team::SubState::Running => StepState::Working,
                        crate::team::SubState::Finished => StepState::Done,
                        crate::team::SubState::Ended(kai_core::Outcome::Success) => StepState::Done,
                        crate::team::SubState::Ended(_) => StepState::Failed,
                    })
                    .unwrap_or(StepState::Waiting);
                let kind = match s.kind {
                    kai_core::orchestrator::Kind::Gui => "screen",
                    kai_core::orchestrator::Kind::Shell => "terminal",
                };
                (format!("{} · {kind}", s.goal), state)
            }),
        )
    })
}

/// The steering composer under the transcript.
fn steer_box(id: u64, app: &KaiApp, _r: &RunState, cx: &Context<KaiApp>) -> AnyElement {
    // Team runs too: whichever sub-agent steps next reads the message.
    let sender = cx.entity();
    PromptInput::new(("steer", id as usize), &app.steer_draft, move |_, cx| {
        sender.update(cx, |a, cx| a.steer(id, cx))
    })
    .into_any_element()
}

/// The left pane's tabs: transcript, terminal, sub-agents.
pub fn tabs(app: &KaiApp, r: &RunState, live: bool, cx: &Context<KaiApp>) -> AnyElement {
    let picker = cx.entity();
    let tab = app.run_tab.clone();
    let panel = match tab.as_str() {
        "terminal" => panes::terminal(r, cx),
        "team" => panes::team_tree(r, cx),
        _ => div()
            .flex()
            .flex_col()
            .gap_2()
            .children(plan(r))
            .children(panes::transcript(r))
            .children(
                (live && r.thinking && !r.held)
                    .then(|| ThinkingIndicator::new("thinking").label(t(cx, "run.thinking", &[]))),
            )
            .into_any_element(),
    };
    let mut choices = vec![
        Choice::new("steps", format!("Steps ({})", r.steps.len())),
        Choice::new("terminal", format!("Terminal ({})", r.outputs.len())),
    ];
    if r.team {
        choices.push(Choice::new(
            "team",
            format!("Sub-agents ({})", r.team_view.subs.len()),
        ));
    }
    Tabs::new("run-tabs", choices, tab)
        .on_change(move |v, _, cx| {
            picker.update(cx, |a, cx| {
                a.run_tab = v.to_string();
                cx.notify();
            })
        })
        .panel(div().pt_3().child(panel))
        .into_any_element()
}

/// What Kai sees, the frame strip, and the usage meters.
pub fn seen(app: &KaiApp, r: &RunState, cx: &Context<KaiApp>) -> AnyElement {
    let theme = cx.theme();
    let pick = app.run.clone();
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
        None => {
            let g = Glass::of(cx);
            ui::card(&g)
                .h(px(260.))
                .flex()
                .flex_col()
                .gap_2()
                .items_center()
                .justify_center()
                .child(ui::eyebrow("Kai's view", cx))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme.colors.fg_muted)
                        .child("Kai's first look at your screen will appear here."),
                )
                .into_any_element()
        }
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
            .children(panes::latency(r, cx))
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
                        "Tokens are estimated ({TOKENS_PER_STEP} per step). Droplet time so far: {:.1} min at ${droplet:.2}/h",
                        r.elapsed.as_secs_f64() / 60.0
                    )),
            ),
    );
    div()
        .p_4()
        .flex()
        .flex_col()
        .gap_3()
        .child(viewer)
        .children(strip)
        .child(meters)
        .into_any_element()
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let id = app.selected.unwrap_or_default();
    let run = app.run.clone();
    let r = run.read(cx);
    let state = agent_state(r);
    let step_no = r.current_step().max(1);
    let status = if r.held {
        Badge::new("Paused").tone(Tone::Warning).into_any_element()
    } else if r.paused {
        Badge::new("Pausing after this step")
            .tone(Tone::Warning)
            .into_any_element()
    } else {
        AgentStatus::new("run-status", state).into_any_element()
    };
    let step = t(
        cx,
        "run.step",
        &[
            ("n", &step_no.to_string()),
            ("max", &r.max_steps.to_string()),
        ],
    );
    let g = Glass::of(cx);
    let done = step_no as f32 / r.max_steps.max(1) as f32;
    let line_color = if r.paused || r.phase == Phase::Waiting {
        cx.theme().colors.warning
    } else {
        g.ember
    };
    let header = div()
        .flex()
        .flex_col()
        .bg(g.pane)
        .border_b_1()
        .border_color(g.line)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_3()
                .px(px(20.))
                .py(px(14.))
                .child(status)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(g.text)
                                .truncate()
                                .child(r.task.clone()),
                        )
                        .child(ui::mono(format!("{step} · {}", r.model), g.faint, cx)),
                )
                .child(controls(id, r, cx)),
        )
        .child(crate::anim::progress(
            done,
            line_color,
            gpui::SharedString::from(format!("run-progress-{id}")),
            &g,
        ));

    let left = div()
        .size_full()
        .flex()
        .flex_col()
        .child(
            div().flex_1().min_h_0().child(
                ScrollArea::new("steps")
                    .vertical()
                    .child(div().p_4().child(tabs(app, r, true, cx))),
            ),
        )
        .child(
            div()
                .p_3()
                .border_t_1()
                .border_color(g.line)
                .bg(g.pane)
                .flex()
                .flex_col()
                .gap_2()
                .child(steer_box(id, app, r, cx))
                .child(ui::hints(
                    &[
                        (&["↵"], "send to Kai"),
                        (&["Ctrl", "P"], if r.paused { "resume" } else { "pause" }),
                        (&["Ctrl", "."], "stop"),
                    ],
                    cx,
                )),
        );
    let right = ScrollArea::new("seen").vertical().child(seen(app, r, cx));

    div().size_full().flex().flex_col().child(header).child(
        div().flex_1().min_h_0().child(
            SplitPane::new("run-split", Axis::Horizontal, px(280.))
                .sizes(&[0.46, 0.54])
                .pane(left)
                .pane(right),
        ),
    )
}
