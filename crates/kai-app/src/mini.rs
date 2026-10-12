//! The always-on-top mini window that shows a run while Kai works, small and in
//! a corner so it never covers what Kai clicks. It is kept out of every frame
//! Kai sends to the model.

use crate::AgentHandle;
use crate::agent::Agent;
use crate::control::RunControl;
use crate::i18n::t;
use crate::pages::running::agent_state;
use crate::state::{Phase, RunState};
use ely_gpui_component::{
    agent::AgentStatus,
    buttons::{Button, ButtonVariant},
    chat::{ThinkingIndicator, stop_button},
    primitives::FocusScope,
    shell::{Corner, MiniWindow},
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, FocusHandle, Global, IntoElement, ParentElement,
    Render, Styled, Window, WindowHandle, div, px, size,
};
use kai_core::human::HumanReply;
use kai_core::{ApprovalChoice, Rect};
use std::sync::Arc;

struct Open(Option<WindowHandle<MiniWindow>>);
impl Global for Open {}

pub struct MiniView {
    run: Entity<RunState>,
    control: Arc<RunControl>,
    focus: FocusHandle,
}

pub fn open(run: Entity<RunState>, control: Arc<RunControl>, cx: &mut App) {
    let view = cx.new(|cx| {
        cx.observe(&run, |_, _, cx| cx.notify()).detach();
        MiniView {
            run,
            control,
            focus: cx.focus_handle(),
        }
    });
    match MiniWindow::open(
        "Kai is working",
        size(px(340.), px(212.)),
        Some(Corner::TopRight),
        view.into(),
        cx,
    ) {
        Ok(h) => cx.set_global(Open(Some(h))),
        Err(e) => log::error!("mini window: {e:#}"),
    }
}

pub fn close(cx: &mut AsyncApp) {
    cx.update(|cx| {
        if let Some(h) = cx.try_global::<Open>().and_then(|o| o.0) {
            let _ = h.update(cx, |_, window, _| window.remove_window());
        }
        cx.set_global(Open(None));
    });
}

/// Keep Kai's own windows out of what the model sees: excluded from capture
/// by the OS on Windows, painted over before encoding on Linux.
pub fn exclude_windows(agent: &Agent, cx: &mut App) {
    let mut rects = Vec::new();
    for handle in cx.windows() {
        let _ = handle.update(cx, |_, window, _| {
            let b = window.bounds();
            let s = window.scale_factor();
            rects.push(Rect::new(
                f32::from(b.origin.x) * s,
                f32::from(b.origin.y) * s,
                f32::from(b.size.width) * s,
                f32::from(b.size.height) * s,
            ));
            #[cfg(target_os = "windows")]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Ok(h) = HasWindowHandle::window_handle(window)
                    && let RawWindowHandle::Win32(w) = h.as_raw()
                {
                    kai_os::exclude::from_capture(w.hwnd.get());
                }
            }
        });
    }
    crate::agent::exclude(&agent.perceptor, rects);
}

impl MiniView {
    fn answer(run: &Entity<RunState>, choice: ApprovalChoice, cx: &mut App) {
        if let Some(p) = run.update(cx, |r, cx| {
            r.phase = Phase::Running;
            cx.notify();
            r.approval.take()
        }) {
            let _ = p.reply.send(choice);
        }
    }

    /// Answer a human check (CAPTCHA, code, sign-in approval, question).
    fn answer_human(run: &Entity<RunState>, reply: HumanReply, cx: &mut App) {
        if let Some(h) = run.update(cx, |r, cx| {
            r.phase = Phase::Running;
            cx.notify();
            r.human.take()
        }) {
            let _ = h.reply.send(reply);
        }
    }
}

impl Render for MiniView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let r = self.run.read(cx);
        let waiting = r.phase == Phase::Waiting;
        let step = t(
            cx,
            "run.step",
            &[
                ("n", &r.current_step().max(1).to_string()),
                ("max", &r.max_steps.to_string()),
            ],
        );
        let (stop_run, a, s, d) = (
            self.run.clone(),
            self.run.clone(),
            self.run.clone(),
            self.run.clone(),
        );

        // A human check: the user does it on screen (or types a code or an
        // answer in Kai's main window), then says so here.
        let (h_done, h_stop) = (self.run.clone(), self.run.clone());
        let human = r.human.as_ref().map(|h| {
            let typed = h.check.accepts_code() || h.check.accepts_answer();
            let hint = if typed {
                format!("{} You can type it in Kai's main window.", h.check.prompt)
            } else {
                h.check.prompt.clone()
            };
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Xs))
                        .line_clamp(3)
                        .child(hint),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("mini-human-done", "I did it")
                                .variant(ButtonVariant::Primary)
                                .on_click(move |_, _, cx| {
                                    Self::answer_human(&h_done, HumanReply::Resolved, cx)
                                }),
                        )
                        .child(
                            Button::new("mini-human-stop", "Stop")
                                .variant(ButtonVariant::Danger)
                                .on_click(move |_, _, cx| {
                                    Self::answer_human(&h_stop, HumanReply::Stop, cx)
                                }),
                        ),
                )
        });
        let has_human = human.is_some();
        let answers = (waiting && !has_human).then(|| {
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("mini-approve", "Approve")
                        .variant(ButtonVariant::Primary)
                        .on_click(move |_, _, cx| Self::answer(&a, ApprovalChoice::Approve, cx)),
                )
                .child(
                    Button::new("mini-skip", "Skip")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, _, cx| Self::answer(&s, ApprovalChoice::Skip, cx)),
                )
                .child(
                    Button::new("mini-deny", "Stop")
                        .variant(ButtonVariant::Danger)
                        .on_click(move |_, _, cx| Self::answer(&d, ApprovalChoice::Stop, cx)),
                )
        });

        FocusScope::new(&self.focus)
            .root()
            .size_full()
            .p_3()
            .pt_4()
            .flex()
            .flex_col()
            .gap_2()
            .bg(if waiting {
                theme.colors.warning_subtle
            } else {
                theme.colors.bg
            })
            .border_1()
            .border_color(if waiting {
                theme.colors.warning
            } else {
                theme.colors.border
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(AgentStatus::new("mini-status", agent_state(r)))
                    .child(
                        div()
                            .text_size(theme.text_size(TextSize::Xs))
                            .text_color(theme.colors.fg_muted)
                            .child(step),
                    ),
            )
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Sm))
                    .line_clamp(2)
                    .child(if r.held {
                        "Paused. Kai waits before its next step.".to_string()
                    } else {
                        r.caption.clone()
                    }),
            )
            .children(
                r.thinking.then(|| {
                    ThinkingIndicator::new("mini-thinking").label(t(cx, "run.thinking", &[]))
                }),
            )
            .child(div().flex_1())
            .children(human)
            .children(answers)
            .children((!waiting).then(|| {
                let (control, run) = (self.control.clone(), self.run.clone());
                let paused = r.paused;
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("mini-pause", if paused { "Resume" } else { "Pause" })
                            .variant(ButtonVariant::Secondary)
                            .on_click(move |_, _, cx| {
                                control.set_paused(!paused);
                                run.update(cx, |r, cx| {
                                    r.paused = !paused;
                                    if paused {
                                        r.held = false;
                                    }
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(stop_button("mini-stop").full_width().on_click(
                                move |_, _, cx| {
                                    cx.global::<AgentHandle>().0.stop();
                                    Self::answer(&stop_run, ApprovalChoice::Stop, cx);
                                },
                            )),
                    )
            }))
    }
}
