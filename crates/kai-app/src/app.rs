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
    App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, Styled, Window, actions, div,
};
use kai_core::Skill;
use kai_core::settings::Settings;
use std::path::PathBuf;

actions!(
    kai,
    [OpenPalette, ShowShortcuts, StopRun, NewTask, ToggleTheme]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-k", OpenPalette, Some("KaiRoot")),
        KeyBinding::new("ctrl-/", ShowShortcuts, Some("KaiRoot")),
        KeyBinding::new("f1", ShowShortcuts, Some("KaiRoot")),
        KeyBinding::new("ctrl-.", StopRun, Some("KaiRoot")),
        KeyBinding::new("ctrl-n", NewTask, Some("KaiRoot")),
        KeyBinding::new("ctrl-shift-l", ToggleTheme, Some("KaiRoot")),
    ]);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Run,
    Skills,
    History,
    Bench,
    Settings,
}

impl Page {
    fn key(self) -> &'static str {
        match self {
            Page::Run => "run",
            Page::Skills => "skills",
            Page::History => "history",
            Page::Bench => "bench",
            Page::Settings => "settings",
        }
    }

    pub(crate) fn from_key(k: &str) -> Option<Self> {
        [
            Page::Run,
            Page::Skills,
            Page::History,
            Page::Bench,
            Page::Settings,
        ]
        .into_iter()
        .find(|p| p.key() == k)
    }
}

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
    /// Which pane the run view shows: "steps", "terminal" or "team".
    pub run_tab: String,
    pub settings: Settings,
    pub skills: Vec<Skill>,
    pub toaster: Entity<Toaster>,
    pub palette: bool,
    pub shortcuts: bool,
    pub team: bool,
    pub conn: Conn,
    pub onboarding: Option<Entity<Onboarding>>,
    pub pages: pages::PageState,
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
        let onboarding = (!settings.onboarded).then(|| {
            let this = cx.entity().downgrade();
            cx.new(|cx| Onboarding::new(settings.clone(), this, window, cx))
        });
        let steer_draft = cx.new(|cx| {
            TextInput::new(window, cx)
                .multi_line(1, 4)
                .placeholder("Tell Kai something mid-run, e.g. \"use Firefox, not Chrome\"")
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
            run_tab: "steps".into(),
            team: settings.team_mode,
            pages: pages::PageState::new(&settings, window, cx),
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
        self.check_connection(window, cx);
        if let Some(dir) = self.capture.clone() {
            crate::pages::capture::start(dir, window, cx);
        }
    }

    pub fn check_connection(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let rx = cx
            .global::<AgentHandle>()
            .0
            .test_connection(self.settings.clone(), kai_os::secrets::keys());
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

    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.palette = false;
        cx.notify();
    }

    pub(crate) fn toggle_theme(cx: &mut App) {
        let next = if cx.theme().is_dark() {
            Mode::Light
        } else {
            Mode::Dark
        };
        Theme::set_mode(next, cx);
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
        let theme_btn = IconButton::new(
            "theme",
            if cx.theme().is_dark() {
                IconName::Sun
            } else {
                IconName::Moon
            },
        )
        .size(ControlSize::Sm)
        .on_click(|_, _, cx| Self::toggle_theme(cx));
        let title = TitleBar::new("title")
            .title(t(cx, "app.name", &[]))
            .action(
                IconButton::new("palette-btn", IconName::Command)
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
            Page::Skills => pages::skills::render(self, window, cx).into_any_element(),
            Page::History => pages::history::render(self, window, cx).into_any_element(),
            Page::Bench => pages::bench::render(self, window, cx).into_any_element(),
            Page::Settings => pages::settings::render(self, window, cx).into_any_element(),
        };
        let me = cx.entity();
        let approval = self.active_state().and_then(|state| {
            let r = state.read(cx);
            r.approval
                .as_ref()
                .map(|p| approval::dialog(p, me.clone(), r).into_any_element())
        });
        let shortcuts = self
            .shortcuts
            .then(|| pages::shortcuts_dialog(me.clone(), window, cx).into_any_element());
        let palette = self.palette.then(|| self.palette(cx).into_any_element());
        let status = self.status_bar(cx).into_any_element();

        let (on_palette, on_stop, on_new, on_short) =
            (cx.entity(), cx.entity(), cx.entity(), cx.entity());
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
            .on_action(|_: &ToggleTheme, _, cx| Self::toggle_theme(cx))
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
                                .child(div().size_full().min_h_0().child(body)),
                        ),
                    )
                    .children(palette)
                    .children(approval)
                    .children(shortcuts)
                    .child(ToastViewport::new("toasts", &self.toaster)),
            )
            .into_any_element()
    }
}
