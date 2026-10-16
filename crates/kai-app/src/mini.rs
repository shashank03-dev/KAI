//! The always-on-top mini window that shows a run while Kai works, small and in
//! a corner so it never covers what Kai clicks. It is kept out of every frame
//! Kai sends to the model.

use crate::agent::Agent;
use crate::app::KaiApp;
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
    App, AppContext, AsyncApp, ClickEvent, Context, Entity, FocusHandle, Global, IntoElement,
    ParentElement, Render, Styled, WeakEntity, Window, WindowHandle, div, px, size,
};
use kai_core::human::HumanReply;
use kai_core::{ApprovalChoice, Rect};

struct Open(Option<WindowHandle<MiniWindow>>);
impl Global for Open {}

/// Shows the active run. Its answers and Stop go through the main window's
/// [`KaiApp`], so both windows answer the same way.
pub struct MiniView {
    app: WeakEntity<KaiApp>,
    run: Entity<RunState>,
    focus: FocusHandle,
}

pub fn open(app: WeakEntity<KaiApp>, run: Entity<RunState>, cx: &mut App) {
    let view = cx.new(|cx| {
        cx.observe(&run, |_, _, cx| cx.notify()).detach();
        MiniView {
            app,
            run,
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

/// Whether the mini window is still on screen (the person may close it).
pub fn is_open(cx: &mut AsyncApp) -> bool {
    cx.update(|cx| {
        cx.try_global::<Open>()
            .and_then(|o| o.0)
            .is_some_and(|h| h.update(cx, |_, _, _| ()).is_ok())
    })
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
            // Screen captures are in device pixels, except on a Mac where
            // Kai scales them to points (`kai_os::frame::to_display`).
            let s = if cfg!(target_os = "macos") {
                1.0
            } else {
                window.scale_factor()
            };
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

/// A click handler that acts through the main window's app (gone only
/// while Kai quits, when nothing is left to answer).
fn via_app(
    app: &WeakEntity<KaiApp>,
    f: fn(&mut KaiApp, &mut Context<KaiApp>),
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let app = app.clone();
    move |_, _, cx| {
        let _ = app.update(cx, f);
    }
}

impl Render for MiniView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let g = crate::ui::Glass::of(cx);
        let r = self.run.read(cx);
        let waiting = r.phase == Phase::Waiting;
        // An ember edge while Kai works, so the corner window reads as live.
        let mut edge = g.ember;
        edge.a = if r.held { 0.25 } else { 0.6 };
        let step = t(
            cx,
            "run.step",
            &[
                ("n", &r.current_step().max(1).to_string()),
                ("max", &r.max_steps.to_string()),
            ],
        );
        let app = &self.app;
        // A human check: the user does it on screen (or types a code or an
        // answer in Kai's main window), then says so here.
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
                                .on_click(via_app(app, |a, cx| {
                                    a.answer_human(HumanReply::Resolved, cx)
                                })),
                        )
                        .child(
                            Button::new("mini-human-stop", "Stop")
                                .variant(ButtonVariant::Danger)
                                .on_click(via_app(app, |a, cx| {
                                    a.answer_human(HumanReply::Stop, cx)
                                })),
                        ),
                )
        });
        // Approve, Skip and Stop only while an approval is actually shown
        // (not in the moment `Waiting` precedes it), and one check at a time.
        let ready = r.approve_ready();
        if !ready {
            // Approve turns on once the grace is over (`APPROVE_GRACE`).
            window.request_animation_frame();
        }
        let answers = (r.approval.is_some() && human.is_none()).then(|| {
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("mini-approve", "Approve")
                        .variant(ButtonVariant::Primary)
                        .disabled(!ready)
                        .on_click(via_app(app, |a, cx| a.answer(ApprovalChoice::Approve, cx))),
                )
                .child(
                    Button::new("mini-skip", "Skip")
                        .variant(ButtonVariant::Secondary)
                        .on_click(via_app(app, |a, cx| a.answer(ApprovalChoice::Skip, cx))),
                )
                .child(
                    Button::new("mini-deny", "Stop")
                        .variant(ButtonVariant::Danger)
                        .on_click(via_app(app, |a, cx| a.answer(ApprovalChoice::Stop, cx))),
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
                theme.colors.surface
            })
            .border_1()
            .border_color(if waiting { theme.colors.warning } else { edge })
            .shadow(vec![g.sheen_shadow()])
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
            .child(crate::anim::progress(
                r.current_step().max(1) as f32 / r.max_steps.max(1) as f32,
                if waiting {
                    theme.colors.warning
                } else {
                    g.ember
                },
                "mini-progress",
                &g,
            ))
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Sm))
                    .line_clamp(2)
                    .child(if r.held {
                        "Paused. Kai waits before its next step.".to_string()
                    } else if let Some(p) = &r.approval {
                        // Team: another sub-agent's events can change the
                        // caption while this approval waits.
                        format!(
                            "Waiting for you: {}",
                            crate::words::describe(&p.decision, p.element.as_deref())
                        )
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
                let paused = r.paused;
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("mini-pause", if paused { "Resume" } else { "Pause" })
                            .variant(ButtonVariant::Secondary)
                            .on_click(via_app(app, KaiApp::toggle_pause)),
                    )
                    .child(
                        div().flex_1().child(
                            stop_button("mini-stop")
                                .full_width()
                                .on_click(via_app(app, KaiApp::stop)),
                        ),
                    )
            }))
    }
}
