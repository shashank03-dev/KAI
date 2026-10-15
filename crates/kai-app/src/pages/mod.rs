//! The pages behind the activity bar, and the state their fields keep.

pub mod bench;
pub mod capture;
pub mod capture_models;
pub mod done;
pub mod history;
pub mod home;
pub mod memory;
pub mod model_settings;
pub mod models;
pub mod models_card;
pub mod models_store;
pub mod panes;
pub mod running;
pub mod sessions;
pub mod settings;
pub mod skills;

use crate::app::KaiApp;
use crate::state::Phase;
use ely_gpui_component::{
    forms::TextInput,
    overlays::Dialog,
    settings::{KeyboardShortcutsList, Shortcut},
};
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
    pub gateway_key: Entity<TextInput>,
    pub openai_key: Entity<TextInput>,
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
    /// The results file whose tasks the Benchmarks page shows.
    pub bench_pick: String,
    /// The backend being tested ("" for none) and the last result per backend.
    pub testing: String,
    pub test_results: Vec<(String, Result<String, String>)>,
    pub confirm_delete: bool,
    /// Settings › Memory › Clear all, waiting for a yes.
    pub confirm_clear_memory: bool,
    /// Models: the downloaded model waiting for a yes before it is deleted.
    pub confirm_remove_model: Option<String>,
    /// Settings › Memory: the "Add a lesson" field, the lessons, and skill
    /// suggestions from proven lessons.
    pub lesson_draft: Entity<TextInput>,
    pub lessons: Vec<kai_core::Lesson>,
    pub memory_suggestions: Vec<kai_core::memory::promote::Suggestion>,
    /// Models › More from Hugging Face: the search field and its results.
    pub store_query: Entity<TextInput>,
    pub store: crate::store::StoreState,
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
            gateway_key: masked(
                "Vercel AI Gateway key (stored in the OS keychain)",
                window,
                cx,
            ),
            openai_key: masked("OpenAI API key (stored in the OS keychain)", window, cx),
            skill_draft: cx.new(|cx| TextInput::new(window, cx).multi_line(12, 40)),
            answer: field("Type your answer", window, cx),
            disabled: settings.disabled_skills.iter().cloned().collect(),
            open_skill: None,
            drafting: false,
            settings_section: "model".into(),
            history: history::load(),
            open_trace: None,
            trace_range: None,
            bench: bench::load(),
            bench_running: false,
            bench_pick: String::new(),
            testing: String::new(),
            test_results: Vec::new(),
            confirm_delete: false,
            confirm_clear_memory: false,
            confirm_remove_model: None,
            lesson_draft: field("e.g. Save notes to ~/Notes", window, cx),
            lessons: Vec::new(),
            memory_suggestions: Vec::new(),
            store_query: field("Search Hugging Face, e.g. holo, fara, gemma", window, cx),
            store: Default::default(),
        }
    }

    /// Re-read the lessons and suggestions from disk.
    pub fn reload_memory(&mut self) {
        (self.lessons, self.memory_suggestions) = memory::load();
    }

    pub fn reload_history(&mut self) {
        self.history = history::load();
    }
}

/// Below this width, tasks and the screen use switchable views.
pub fn compact_run(window: &Window) -> bool {
    window.viewport_size().width < gpui::px(1100.)
}

/// Run: the sessions list, and beside it the composer for a new task, a
/// queued run, the live run, or a finished run's result.
pub fn run_page(
    app: &mut KaiApp,
    window: &mut Window,
    cx: &mut Context<KaiApp>,
) -> impl IntoElement {
    let phase = app.run.read(cx).phase;
    let content = match (app.selected, phase) {
        (None, _) | (_, Phase::Idle) => home::render(app, window, cx).into_any_element(),
        (_, Phase::Queued) => sessions::queued(app, cx),
        (_, Phase::Running | Phase::Waiting) => running::render(app, window, cx).into_any_element(),
        (_, Phase::Done) => done::render(app, window, cx).into_any_element(),
    };
    if app.runs.is_empty() {
        return content;
    }
    if compact_run(window) {
        let picker = cx.entity();
        let label = format!(
            "{} tasks ({})",
            if app.tasks_open { "Hide" } else { "Show" },
            app.runs.len()
        );
        let tasks = app.tasks_open.then(|| sessions::sidebar(app, window, cx));
        return div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div().flex_none().px_3().py_1().child(
                    ely_gpui_component::buttons::Button::new("tasks-toggle", label)
                        .variant(ely_gpui_component::buttons::ButtonVariant::Ghost)
                        .on_click(move |_, _, cx| {
                            picker.update(cx, |a, cx| {
                                a.tasks_open = !a.tasks_open;
                                cx.notify();
                            })
                        }),
                ),
            )
            .children(tasks)
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element();
    }
    div()
        .size_full()
        .flex()
        .child(sessions::sidebar(app, window, cx))
        .child(div().flex_1().min_w_0().h_full().child(content))
        .into_any_element()
}

pub fn shortcut_list() -> Vec<Shortcut> {
    let s = |group: &str, name: &str, keys: &str| Shortcut {
        group: group.to_string().into(),
        name: name.to_string().into(),
        keys: keys.to_string().into(),
    };
    vec![
        s("Anywhere", "Stop Kai", "ctrl-alt-escape"),
        s(
            "Anywhere",
            "Stop Kai (if the first is taken)",
            "ctrl-alt-end",
        ),
        s("In Kai", "Command palette", "secondary-k"),
        s("In Kai", "New task", "secondary-n"),
        s("In a run", "Send a message to Kai", "enter"),
        s("In Kai", "Stop the run", "secondary-."),
        s("In Kai", "Toggle light and dark", "secondary-shift-l"),
        s("Go to", "Run", "secondary-1"),
        s("Go to", "Models", "secondary-2"),
        s("Go to", "Skills", "secondary-3"),
        s("Go to", "History", "secondary-4"),
        s("Go to", "Benchmarks", "secondary-5"),
        s("Go to", "Settings", "secondary-,"),
        s("Tasks", "Next task", "alt-down"),
        s("Tasks", "Previous task", "alt-up"),
        s("Tasks", "Pause or resume the running task", "secondary-p"),
        s("Tasks", "Run the selected task again", "secondary-r"),
        s("In a run", "Jump to the latest step", "secondary-down"),
        s("In Kai", "Keyboard shortcuts", "secondary-/"),
        s("Approval", "Approve", "enter"),
        s("Approval", "Stop", "escape"),
    ]
}

/// `search` is passed in: reading the app entity while it renders panics.
pub fn shortcuts_dialog(app: Entity<KaiApp>, search: Entity<TextInput>) -> impl IntoElement {
    Dialog::new("shortcuts", "Keyboard shortcuts", move |_, cx| {
        app.update(cx, |a, cx| {
            a.shortcuts = false;
            cx.notify();
        })
    })
    .child(
        div()
            .w_full()
            .child(KeyboardShortcutsList::new(shortcut_list(), &search)),
    )
}
