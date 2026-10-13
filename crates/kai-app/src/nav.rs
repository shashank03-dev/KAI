//! The window's pages and keyboard shortcuts: the actions, their chords and
//! the page each one opens. The main window (`app.rs`) handles them.

use gpui::{App, KeyBinding, actions};

actions!(
    kai,
    [
        OpenPalette,
        ShowShortcuts,
        StopRun,
        NewTask,
        ToggleTheme,
        GoRun,
        GoModels,
        GoSkills,
        GoHistory,
        GoBench,
        GoSettings,
        NextTask,
        PrevTask,
        TogglePause,
        RetryRun
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-k", OpenPalette, Some("KaiRoot")),
        KeyBinding::new("ctrl-/", ShowShortcuts, Some("KaiRoot")),
        KeyBinding::new("f1", ShowShortcuts, Some("KaiRoot")),
        KeyBinding::new("ctrl-.", StopRun, Some("KaiRoot")),
        KeyBinding::new("ctrl-n", NewTask, Some("KaiRoot")),
        KeyBinding::new("ctrl-shift-l", ToggleTheme, Some("KaiRoot")),
        // Keyboard-first, like Linear and Superhuman: every page one chord
        // away, tasks walkable without the mouse.
        KeyBinding::new("ctrl-1", GoRun, Some("KaiRoot")),
        KeyBinding::new("ctrl-2", GoModels, Some("KaiRoot")),
        KeyBinding::new("ctrl-3", GoSkills, Some("KaiRoot")),
        KeyBinding::new("ctrl-4", GoHistory, Some("KaiRoot")),
        KeyBinding::new("ctrl-5", GoBench, Some("KaiRoot")),
        KeyBinding::new("ctrl-,", GoSettings, Some("KaiRoot")),
        KeyBinding::new("alt-down", NextTask, Some("KaiRoot")),
        KeyBinding::new("alt-up", PrevTask, Some("KaiRoot")),
        KeyBinding::new("ctrl-p", TogglePause, Some("KaiRoot")),
        KeyBinding::new("ctrl-r", RetryRun, Some("KaiRoot")),
    ]);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Run,
    Models,
    Skills,
    History,
    Bench,
    Settings,
}

impl Page {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Page::Run => "run",
            Page::Models => "models",
            Page::Skills => "skills",
            Page::History => "history",
            Page::Bench => "bench",
            Page::Settings => "settings",
        }
    }

    pub(crate) fn from_key(k: &str) -> Option<Self> {
        [
            Page::Run,
            Page::Models,
            Page::Skills,
            Page::History,
            Page::Bench,
            Page::Settings,
        ]
        .into_iter()
        .find(|p| p.key() == k)
    }
}
