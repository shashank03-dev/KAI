//! The live run: the transcript, terminal and sub-agents on the left with a
//! composer to steer the agent, what Kai sees on the right, and the run's
//! controls (pause, resume, stop) in the header.

use crate::app::{KaiApp, Page};
use crate::follow::{self, Frame};
use crate::i18n::t;
use crate::pages::panes;
use crate::state::{Phase, RunState};
use crate::ui::{self, Glass};
use ely_gpui_component::{
    agent::{AgentPlan, AgentState, AgentStatus, BrowserPreview, ComputerUseViewer},
    buttons::{Button, ButtonVariant},
    chat::{PromptInput, StepState, ThinkingIndicator, stop_button},
    data_display::{Badge, Tone},
    forms::Choice,
    layout::{Panel, ScrollArea, SplitPane},
    navigation::Tabs,
    primitives::IconName,
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    AnyElement, Axis, Context, FontWeight, IntoElement, ParentElement, ScrollHandle, Styled,
    Window, div, point, px,
};

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

/// The outcome checklist (`docs/VERIFICATION.md`): verified, waiting, or
/// failed when Kai last checked.
fn outcomes(r: &RunState) -> Option<AgentPlan> {
    use kai_core::ledger::Status;
    (!r.outcomes.is_empty()).then(|| {
        AgentPlan::new(
            "outcomes",
            "Checked before done",
            r.outcomes.iter().map(|o| {
                let state = match o.status {
                    Status::Verified => StepState::Done,
                    Status::Reverted => StepState::Failed,
                    Status::Pending => StepState::Waiting,
                };
                let how = o.probe.as_deref().unwrap_or("not checkable");
                (format!("{} · {how}", o.description), state)
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

/// The tab on show: Screen is a tab only in the compact layout.
fn shown_tab(app: &KaiApp, compact: bool) -> String {
    if !compact && app.run_tab == "screen" {
        "steps".into()
    } else {
        app.run_tab.clone()
    }
}

/// Entries a tab lists, for the views that follow the newest one.
fn entries(r: &RunState, tab: &str) -> Option<usize> {
    match tab {
        "steps" => Some(r.steps.len() + r.steers.len()),
        "terminal" => Some(r.outputs.len()),
        _ => None,
    }
}

impl KaiApp {
    /// Back to the newest step (or command): the view follows again.
    pub fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.filter(|_| self.page == Page::Run) else {
            return;
        };
        let r = self.run.read(cx);
        if entries(r, &self.run_tab).is_none() {
            self.run_tab = "steps".into();
        }
        let n = entries(r, &self.run_tab).unwrap_or_default();
        let (scroll, follow) = self.transcripts.view(id, &self.run_tab);
        follow.jump(n);
        scroll.scroll_to_bottom();
        cx.notify();
    }
}

/// The left pane's scroll handle, scrolled to the end while it follows, and
/// "Jump to latest" while the user reads older entries.
fn transcript_scroll(
    app: &mut KaiApp,
    id: u64,
    tab: &str,
    r: &RunState,
    cx: &Context<KaiApp>,
) -> (ScrollHandle, Option<AnyElement>) {
    let (scroll, follow) = app.transcripts.view(id, tab);
    let scroll = scroll.clone();
    let Some(entries) = entries(r, tab) else {
        return (scroll, None);
    };
    let frame = Frame {
        offset: scroll.offset().y.into(),
        max: scroll.max_offset().y.into(),
        entries,
    };
    if follow.frame(frame) {
        scroll.scroll_to_bottom();
        return (scroll, None);
    }
    let jumper = cx.entity();
    let jump = div()
        .absolute()
        .bottom(px(12.))
        .right(px(20.))
        .rounded_md()
        .shadow_md()
        .child(
            Button::new("jump-latest", follow::jump_label(follow.unseen(entries)))
                .variant(ButtonVariant::Secondary)
                .icon(IconName::ArrowDown)
                .on_click(move |_, _, cx| jumper.update(cx, |a, cx| a.jump_to_latest(cx))),
        )
        .into_any_element();
    (scroll, Some(jump))
}

/// The shown tab's content: transcript, terminal, screen or sub-agents.
fn panel(
    app: &KaiApp,
    r: &RunState,
    live: bool,
    compact: bool,
    cx: &Context<KaiApp>,
) -> AnyElement {
    match shown_tab(app, compact).as_str() {
        "terminal" => panes::terminal(r, cx),
        "team" => panes::team_tree(r, cx),
        "screen" => seen(app, r, cx),
        _ => div()
            .flex()
            .flex_col()
            .gap_2()
            .children(plan(r))
            .children(outcomes(r))
            .children(panes::transcript(r))
            .children(
                (live && r.thinking && !r.held)
                    .then(|| ThinkingIndicator::new("thinking").label(t(cx, "run.thinking", &[]))),
            )
            .into_any_element(),
    }
}

/// The left pane's tab strip. It stays above the scrolled panel, so the
/// tabs remain in reach while the transcript follows its newest step.
fn strip(app: &KaiApp, r: &RunState, compact: bool, cx: &Context<KaiApp>) -> AnyElement {
    let picker = cx.entity();
    let mut choices = vec![
        Choice::new("steps", format!("Steps ({})", r.steps.len())),
        Choice::new("terminal", format!("Terminal ({})", r.outputs.len())),
    ];
    if compact {
        choices.push(Choice::new("screen", "Screen"));
    }
    if r.team {
        choices.push(Choice::new(
            "team",
            format!("Sub-agents ({})", r.team_view.subs.len()),
        ));
    }
    Tabs::new("run-tabs", choices, shown_tab(app, compact))
        .on_change(move |v, _, cx| {
            picker.update(cx, |a, cx| {
                a.run_tab = v.to_string();
                cx.notify();
            })
        })
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

    let note = panes::usage_note(
        app.settings.backend,
        app.settings.droplet_hourly_usd as f64,
        r.elapsed,
    );
    let meters = Panel::new("Usage").child(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(panes::latency(r, cx))
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Xs))
                    .text_color(theme.colors.fg_muted)
                    .child(note),
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

pub fn render(app: &mut KaiApp, window: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let compact = super::compact_run(window);
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
    let (scroll, jump) = transcript_scroll(app, id, &shown_tab(app, compact), r, cx);
    let done = step_no as f32 / r.max_steps.max(1) as f32;
    let line_color = if r.paused || r.phase == Phase::Waiting {
        cx.theme().colors.warning
    } else {
        g.ember
    };
    let header = div()
        .flex_none()
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
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_none()
                .px_4()
                .pt_3()
                .child(strip(app, r, compact, cx)),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .relative()
                .child(
                    ScrollArea::new("steps")
                        .size_full()
                        .vertical()
                        .track_scroll(&scroll)
                        .child(div().p_4().child(panel(app, r, true, compact, cx))),
                )
                .children(jump),
        )
        .child(
            div()
                .flex_none()
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
                        (
                            &[ui::MOD_KEY, "P"],
                            if r.paused { "resume" } else { "pause" },
                        ),
                        (&[ui::MOD_KEY, "."], "stop"),
                    ],
                    cx,
                )),
        );
    let right = ScrollArea::new("seen")
        .size_full()
        .vertical()
        .child(seen(app, r, cx));

    let body = if compact {
        left.into_any_element()
    } else {
        SplitPane::new("run-split", Axis::Horizontal, px(280.))
            .sizes(&[0.46, 0.54])
            .pane(left)
            .pane(right)
            .into_any_element()
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(header)
        .child(div().flex_1().min_h_0().child(body))
}
