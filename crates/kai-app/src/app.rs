//! The main window: Ely's AppShell with the title bar, activity bar and status
//! bar around the current page, plus the palette, toasts and the run flow.

use crate::{
    AgentHandle, approval, i18n::t, onboarding::Onboarding, pages, runs::RunEntry, state::RunState,
};
use ely_gpui_component::{
    buttons::IconButton,
    feedback::{Toast, ToastViewport, Toaster},
    forms::TextInput,
    layout::AppShell,
    primitives::{FocusScope, IconName, Severity},
    shell::{ActivityBar, ActivityItem, ResizeBorder, TitleBar},
    theme::{ActiveTheme, ControlSize, Mode, Theme},
};
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Window, div,
};
use kai_core::Skill;
use kai_core::settings::Settings;
use std::path::PathBuf;

pub use crate::nav::*;

/// Is a model reachable, and which one serves.
#[derive(Clone, Debug, PartialEq)]
pub enum Conn {
    Unknown,
    Online(String),
    Fallback(String),
    Offline(String),
}

pub struct KaiApp {
    pub focus: FocusHandle,
    pub page: Page,
    /// The run on screen: the selected run, or `idle` for a new task.
    pub run: Entity<RunState>,
    pub idle: Entity<RunState>,
    pub runs: Vec<RunEntry>,
    pub selected: Option<u64>,
    /// The run that has the screen.
    pub active: Option<u64>,
    /// The user stopped Kai: queued runs wait until they resume the queue.
    pub queue_held: bool,
    pub next_id: u64,
    pub draft: Entity<TextInput>,
    pub steer_draft: Entity<TextInput>,
    /// The code or answer typed into the human-check dialog.
    pub human_draft: Entity<TextInput>,
    /// Which pane the run view shows: "steps", "terminal" or "team".
    pub run_tab: String,
    pub tasks_open: bool,
    /// Where each run's Steps and Terminal views are scrolled to.
    pub transcripts: crate::follow::Transcripts,
    pub settings: Settings,
    pub skills: Vec<Skill>,
    pub toaster: Entity<Toaster>,
    pub palette: bool,
    pub shortcuts: bool,
    pub team: bool,
    pub conn: Conn,
    pub onboarding: Option<Entity<Onboarding>>,
    pub pages: pages::PageState,
    /// Local open models: hardware, ranking, the download/start job.
    pub local: Entity<crate::local::LocalState>,
    pub capture: Option<PathBuf>,
}

impl KaiApp {
    pub fn new(
        settings: Settings,
        capture: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let draft = cx.new(|cx| {
            TextInput::new(window, cx)
                .multi_line(1, 6)
                .placeholder(t(cx, "home.placeholder", &[]))
        });
        let local = cx.new(|_| crate::local::LocalState::new(&settings.local_model));
        crate::local::stop_on_quit(&local, cx);
        cx.on_app_quit(|app, cx| {
            app.quit_runs(cx);
            async {}
        })
        .detach();
        let onboarding = (!settings.onboarded).then(|| {
            let (this, local) = (cx.entity().downgrade(), local.clone());
            cx.new(|cx| Onboarding::new(settings.clone(), this, local, window, cx))
        });
        let steer_draft = cx.new(|cx| {
            TextInput::new(window, cx)
                .multi_line(1, 4)
                .placeholder("Message Kai during this run…")
        });
        let human_draft = cx.new(|cx| {
            TextInput::new(window, cx)
                .multi_line(1, 2)
                .placeholder("The code, or your answer")
        });
        let idle = cx.new(|_| RunState::default());
        Self {
            focus,
            page: Page::Run,
            run: idle.clone(),
            idle,
            runs: Vec::new(),
            selected: None,
            active: None,
            queue_held: false,
            next_id: 0,
            draft,
            steer_draft,
            human_draft,
            run_tab: "steps".into(),
            tasks_open: false,
            transcripts: Default::default(),
            team: settings.team_mode,
            pages: pages::PageState::new(&settings, window, cx),
            local,
            settings,
            skills: load_skills(),
            toaster: cx.new(|_| Toaster::default()),
            palette: false,
            shortcuts: false,
            conn: Conn::Unknown,
            onboarding,
            capture,
        }
    }

    /// Runs once the window is up: warm the connection and report it.
    pub fn after_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dir) = self.capture.clone() {
            crate::pages::capture::start(dir, window, cx);
        } else {
            self.check_connection(window, cx);
            self.detect_local(cx);
        }
    }

    pub fn check_connection(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.recheck_connection(cx);
    }

    pub fn recheck_connection(&mut self, cx: &mut Context<Self>) {
        if self.settings.gui_only
            || matches!(
                self.settings.backend,
                kai_core::settings::BackendChoice::Codex
                    | kai_core::settings::BackendChoice::Claude
            )
        {
            // Catalog/account inspection must not silently consume inference.
            self.conn = Conn::Unknown;
            cx.notify();
            return;
        }
        let rx = cx
            .global::<AgentHandle>()
            .0
            .test_connection(self.settings.clone(), Default::default());
        cx.spawn(async move |this, cx| {
            let result = rx
                .await
                .unwrap_or_else(|_| Err("Kai's agent stopped.".into()));
            let _ = this.update(cx, |app, cx| {
                app.conn = match result {
                    Ok((took, backend))
                        if backend.contains("Gemini") && app.settings.vllm_endpoint.is_empty() =>
                    {
                        Conn::Online(format!("{backend} · {} ms", took.as_millis()))
                    }
                    Ok((took, backend)) if backend.contains("Gemini") => {
                        Conn::Fallback(format!("{backend} · {} ms", took.as_millis()))
                    }
                    Ok((took, backend)) => {
                        Conn::Online(format!("{backend} · {} ms", took.as_millis()))
                    }
                    Err(e) => Conn::Offline(e),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub fn toast(&self, toast: Toast, cx: &mut App) {
        self.toaster.update(cx, |t, cx| t.push(toast, cx));
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.settings.save() {
            self.toast(
                Toast::new("Settings not saved")
                    .body(e.to_string())
                    .severity(Severity::Danger),
                cx,
            );
        }
        cx.notify();
    }

    pub fn enabled_skills(&self) -> Vec<Skill> {
        self.skills
            .iter()
            .filter(|s| !self.pages.disabled.contains(&s.front.name))
            .cloned()
            .collect()
    }

    /// Walk the task list: the next (or previous) run, wrapping at neither end.
    pub fn step_task(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(last) = self.runs.len().checked_sub(1) else {
            return;
        };
        let at = self
            .selected
            .and_then(|id| self.runs.iter().position(|r| r.id == id));
        let next = match at {
            None if delta > 0 => 0,
            None => last,
            Some(i) => (i as i64 + delta as i64).clamp(0, last as i64) as usize,
        };
        let id = self.runs[next].id;
        self.select(Some(id), cx);
    }

    /// Pause or resume the run that has the screen.
    pub fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.active {
            let paused = self.entry(id).is_some_and(|e| e.state.read(cx).paused);
            self.set_paused(id, !paused, cx);
        }
    }

    /// Run the selected finished task again.
    pub fn retry_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let done = self
            .selected
            .and_then(|id| self.entry(id))
            .is_some_and(|e| e.state.read(cx).phase == crate::state::Phase::Done);
        if let (true, Some(id)) = (done, self.selected) {
            self.retry(id, window, cx);
        }
    }

    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.palette = false;
        cx.notify();
    }

    /// Flip light/dark and remember it, so Settings › Appearance and the
    /// next start agree with what is on screen.
    pub(crate) fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        let (next, name) = if cx.theme().is_dark() {
            (Mode::Light, "light")
        } else {
            (Mode::Dark, "dark")
        };
        Theme::set_mode(next, cx);
        self.settings.theme = name.into();
        self.save_settings(cx);
    }
}

fn load_skills() -> Vec<Skill> {
    let mut v = Vec::new();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        Some(PathBuf::from("skills")),
        exe_dir.as_ref().map(|d| d.join("skills")),
        exe_dir.as_ref().map(|d| d.join("../../skills")),
        // Inside the macOS app bundle: Kai.app/Contents/Resources/skills.
        exe_dir.as_ref().map(|d| d.join("../Resources/skills")),
    ];
    if let Some(dir) = candidates.into_iter().flatten().find(|p| p.is_dir()) {
        v.extend(kai_core::skill::load_dir(&dir, true));
    }
    v.extend(kai_core::skill::load_dir(&Settings::skills_dir(), false));
    v
}

impl Render for KaiApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, font) = (theme.colors.bg, theme.colors.fg, theme.font_family.clone());

        if let Some(onboarding) = self.onboarding.clone() {
            return FocusScope::new(&self.focus)
                .root()
                .size_full()
                .bg(bg)
                .text_color(fg)
                .font_family(font)
                .child(
                    ResizeBorder::new().child(
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .child(TitleBar::new("title").title(t(cx, "app.name", &[])))
                            .child(div().flex_1().min_h_0().child(onboarding)),
                    ),
                )
                .into_any_element();
        }

        let nav = cx.entity();
        let palette_btn = cx.entity();
        let theme_toggler = cx.entity();
        let theme_btn = IconButton::new(
            "theme",
            if cx.theme().is_dark() {
                IconName::Sun
            } else {
                IconName::Moon
            },
        )
        .size(ControlSize::Sm)
        .tooltip("Toggle light and dark theme")
        .on_click(move |_, _, cx| theme_toggler.update(cx, |a, cx| a.toggle_theme(cx)));
        let title = TitleBar::new("title")
            .title(t(cx, "app.name", &[]))
            .action(
                IconButton::new("palette-btn", IconName::Command)
                    .tooltip("Open command palette")
                    .size(ControlSize::Sm)
                    .on_click(move |_, _, cx| {
                        palette_btn.update(cx, |app, cx| {
                            app.palette = true;
                            cx.notify();
                        })
                    }),
            )
            .action(theme_btn);
        let activity = ActivityBar::new("nav")
            .item(ActivityItem::new(
                "run",
                IconName::Play,
                t(cx, "nav.run", &[]),
            ))
            .item(ActivityItem::new(
                "models",
                IconName::Cpu,
                t(cx, "nav.models", &[]),
            ))
            .item(ActivityItem::new(
                "skills",
                IconName::Puzzle,
                t(cx, "nav.skills", &[]),
            ))
            .item(ActivityItem::new(
                "history",
                IconName::History,
                t(cx, "nav.history", &[]),
            ))
            .item(ActivityItem::new(
                "bench",
                IconName::ChartColumn,
                t(cx, "nav.bench", &[]),
            ))
            .footer(ActivityItem::new(
                "settings",
                IconName::Settings,
                t(cx, "nav.settings", &[]),
            ))
            .selected(self.page.key())
            .on_select(move |key, _, cx| {
                if let Some(p) = Page::from_key(key) {
                    nav.update(cx, |app, cx| app.go(p, cx));
                }
            });

        let body = match self.page {
            Page::Run => pages::run_page(self, window, cx).into_any_element(),
            Page::Models => pages::models::render(self, window, cx).into_any_element(),
            Page::Skills => pages::skills::render(self, window, cx).into_any_element(),
            Page::History => pages::history::render(self, window, cx).into_any_element(),
            Page::Bench => pages::bench::render(self, window, cx).into_any_element(),
            Page::Settings => pages::settings::render(self, window, cx).into_any_element(),
        };
        let me = cx.entity();
        // One decision on screen at a time: a check first (a code or a
        // sign-in does not wait long), then approvals; the rest queue.
        let decision = self.active_state().and_then(|state| {
            let r = state.read(cx);
            let more = r.decisions_waiting().saturating_sub(1);
            if let Some(h) = &r.human {
                let d = crate::human_dialog::dialog(h, me.clone(), &self.human_draft, more);
                return Some(d.into_any_element());
            }
            let p = r.approval.as_ref()?;
            if !r.approve_ready() {
                // Approve turns on once the grace is over (`APPROVE_GRACE`).
                window.request_animation_frame();
            }
            Some(approval::dialog(p, me.clone(), r, more).into_any_element())
        });
        let shortcuts = self.shortcuts.then(|| {
            pages::shortcuts_dialog(me.clone(), self.pages.shortcut_search.clone())
                .into_any_element()
        });
        let palette = self.palette.then(|| self.palette(cx).into_any_element());
        let status = self.status_bar(window, cx).into_any_element();

        let (on_palette, on_stop, on_new, on_short, on_theme) = (
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
        );
        div()
            .key_context("KaiRoot")
            .size_full()
            .on_action(move |_: &OpenPalette, _, cx| {
                on_palette.update(cx, |a, cx| {
                    a.palette = true;
                    cx.notify();
                })
            })
            .on_action(move |_: &StopRun, _, cx| on_stop.update(cx, |a, cx| a.stop(cx)))
            .on_action(move |_: &NewTask, _, cx| on_new.update(cx, |a, cx| a.select(None, cx)))
            .on_action(move |_: &ShowShortcuts, _, cx| {
                on_short.update(cx, |a, cx| {
                    a.shortcuts = true;
                    cx.notify();
                })
            })
            .on_action(move |_: &ToggleTheme, _, cx| {
                on_theme.update(cx, |a, cx| a.toggle_theme(cx))
            })
            .on_action(cx.listener(|a, _: &GoRun, _, cx| a.go(Page::Run, cx)))
            .on_action(cx.listener(|a, _: &GoModels, _, cx| a.go(Page::Models, cx)))
            .on_action(cx.listener(|a, _: &GoSkills, _, cx| a.go(Page::Skills, cx)))
            .on_action(cx.listener(|a, _: &GoHistory, _, cx| a.go(Page::History, cx)))
            .on_action(cx.listener(|a, _: &GoBench, _, cx| a.go(Page::Bench, cx)))
            .on_action(cx.listener(|a, _: &GoSettings, _, cx| a.go(Page::Settings, cx)))
            .on_action(cx.listener(|a, _: &NextTask, _, cx| a.step_task(1, cx)))
            .on_action(cx.listener(|a, _: &PrevTask, _, cx| a.step_task(-1, cx)))
            .on_action(cx.listener(|a, _: &TogglePause, _, cx| a.toggle_pause(cx)))
            .on_action(cx.listener(|a, _: &RetryRun, window, cx| a.retry_selected(window, cx)))
            .on_action(cx.listener(|a, _: &JumpToLatest, _, cx| a.jump_to_latest(cx)))
            .child(
                FocusScope::new(&self.focus)
                    .root()
                    .size_full()
                    .bg(bg)
                    .text_color(fg)
                    .font_family(font)
                    .child(
                        ResizeBorder::new().child(
                            AppShell::new()
                                .title_bar(title)
                                .sidebar(activity)
                                .status_bar(status)
                                // Each page (and each run) fades in as it
                                // replaces the last.
                                .child(crate::anim::view(
                                    div().size_full().min_h_0().child(body),
                                    gpui::SharedString::from(format!(
                                        "view-{}-{:?}",
                                        self.page.key(),
                                        self.selected
                                    )),
                                )),
                        ),
                    )
                    .children(palette)
                    .children(decision)
                    .children(shortcuts)
                    .child(ToastViewport::new("toasts", &self.toaster)),
            )
            .into_any_element()
    }
}
