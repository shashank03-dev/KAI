//! The pages behind the activity bar, and the state their fields keep.

pub mod bench;
pub mod capture;
pub mod done;
pub mod history;
pub mod home;
pub mod running;
pub mod settings;
pub mod skills;

use crate::app::KaiApp;
use crate::state::Phase;
use ely_gpui_component::{forms::TextInput, overlays::Dialog, settings::{KeyboardShortcutsList, Shortcut}};
use gpui::{AppContext, Context, Entity, IntoElement, ParentElement, Styled, Window, div};
use kai_core::settings::Settings;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Fields and lists the pages keep between frames.
pub struct PageState {
    pub skill_search: Entity<TextInput>,
    pub history_search: Entity<TextInput>,
    pub shortcut_search: Entity<TextInput>,
    pub endpoint: Entity<TextInput>,
    pub vllm_key: Entity<TextInput>,
    pub gemini_key: Entity<TextInput>,
    pub skill_draft: Entity<TextInput>,
    pub answer: Entity<TextInput>,
    pub disabled: BTreeSet<String>,
    pub open_skill: Option<String>,
    pub drafting: bool,
    pub settings_section: String,
    pub history: Vec<history::Entry>,
    pub open_trace: Option<PathBuf>,
    pub trace_range: Option<(f64, f64)>,
    pub bench: Vec<bench::Results>,
    pub bench_running: bool,
    pub testing: bool,
    pub test_result: Option<Result<String, String>>,
    pub confirm_delete: bool,
}

impl PageState {
    pub fn new(settings: &Settings, window: &mut Window, cx: &mut Context<KaiApp>) -> Self {
        let field = |placeholder: &str, window: &mut Window, cx: &mut Context<KaiApp>| {
            let p = placeholder.to_string();
            cx.new(|cx| TextInput::new(window, cx).placeholder(p))
        };
        let endpoint = field("https://<droplet>.sslip.io/v1", window, cx);
        let ep = settings.vllm_endpoint.clone();
        endpoint.update(cx, |f, cx| f.set_text(ep, cx));
        let masked = |label: &str, window: &mut Window, cx: &mut Context<KaiApp>| {
            let l = label.to_string();
            cx.new(|cx| TextInput::new(window, cx).placeholder(l).masked())
        };
        Self {
            skill_search: field("Search skills", window, cx),
            history_search: field("Search past runs", window, cx),
            shortcut_search: field("Search shortcuts", window, cx),
            endpoint,
            vllm_key: masked("vLLM API key (stored in the OS keychain)", window, cx),
            gemini_key: masked("Gemini API key (stored in the OS keychain)", window, cx),
            skill_draft: cx.new(|cx| TextInput::new(window, cx).multi_line(12, 40)),
            answer: field("Type your answer", window, cx),
            disabled: BTreeSet::new(),
            open_skill: None,
            drafting: false,
            settings_section: "model".into(),
            history: history::load(),
            open_trace: None,
            trace_range: None,
            bench: bench::load(),
            bench_running: false,
            testing: false,
            test_result: None,
            confirm_delete: false,
        }
    }

    pub fn reload_history(&mut self) {
        self.history = history::load();
    }
}

/// Run: the composer when idle, the live run while working, the result after.
pub fn run_page(app: &mut KaiApp, window: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    match app.run.read(cx).phase {
        Phase::Idle => home::render(app, window, cx).into_any_element(),
        Phase::Running | Phase::Waiting => running::render(app, window, cx).into_any_element(),
        Phase::Done => done::render(app, window, cx).into_any_element(),
    }
}

pub fn shortcut_list() -> Vec<Shortcut> {
    let s = |group: &str, name: &str, keys: &str| Shortcut { group: group.to_string().into(), name: name.to_string().into(), keys: keys.to_string().into() };
    vec![
        s("Anywhere", "Stop Kai", "ctrl-alt-escape"),
        s("Anywhere", "Stop Kai (if the first is taken)", "ctrl-alt-end"),
        s("Anywhere", "Quick launcher", "ctrl-shift-space"),
        s("In Kai", "Command palette", "ctrl-k"),
        s("In Kai", "New task", "ctrl-n"),
        s("In Kai", "Stop the run", "ctrl-."),
        s("In Kai", "Toggle light and dark", "ctrl-shift-l"),
        s("In Kai", "Keyboard shortcuts", "ctrl-/"),
        s("Approval", "Approve", "enter"),
        s("Approval", "Stop", "escape"),
    ]
}

pub fn shortcuts_dialog(app: Entity<KaiApp>, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let search = cx.entity().read(cx).pages.shortcut_search.clone();
    Dialog::new("shortcuts", "Keyboard shortcuts", move |_, cx| {
        app.update(cx, |a, cx| {
            a.shortcuts = false;
            cx.notify();
        })
    })
    .child(div().w_full().child(KeyboardShortcutsList::new(shortcut_list(), &search)))
}
