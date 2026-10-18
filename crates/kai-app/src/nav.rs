//! The window's pages and keyboard shortcuts: the actions, their chords and
//! the page each one opens. The main window (`app.rs`) handles them.

use gpui::{App, KeyBinding, Menu, MenuItem, actions};

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
        RetryRun,
        JumpToLatest,
        Quit,
        Hide,
        HideOthers,
        Minimize
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-k", OpenPalette, Some("KaiRoot")),
        KeyBinding::new("secondary-/", ShowShortcuts, Some("KaiRoot")),
        KeyBinding::new("f1", ShowShortcuts, Some("KaiRoot")),
        KeyBinding::new("secondary-.", StopRun, Some("KaiRoot")),
        KeyBinding::new("secondary-n", NewTask, Some("KaiRoot")),
        KeyBinding::new("secondary-shift-l", ToggleTheme, Some("KaiRoot")),
        // `secondary` is ⌘ on a Mac and Ctrl elsewhere.
        // Keyboard-first, like Linear and Superhuman: every page one chord
        // away, tasks walkable without the mouse.
        KeyBinding::new("secondary-1", GoRun, Some("KaiRoot")),
        KeyBinding::new("secondary-2", GoModels, Some("KaiRoot")),
        KeyBinding::new("secondary-3", GoSkills, Some("KaiRoot")),
        KeyBinding::new("secondary-4", GoHistory, Some("KaiRoot")),
        KeyBinding::new("secondary-5", GoBench, Some("KaiRoot")),
        KeyBinding::new("secondary-,", GoSettings, Some("KaiRoot")),
        KeyBinding::new("alt-down", NextTask, Some("KaiRoot")),
        KeyBinding::new("alt-up", PrevTask, Some("KaiRoot")),
        KeyBinding::new("secondary-p", TogglePause, Some("KaiRoot")),
        KeyBinding::new("secondary-r", RetryRun, Some("KaiRoot")),
        KeyBinding::new("secondary-down", JumpToLatest, Some("KaiRoot")),
    ]);
    // A Mac app is quit from its menu bar (⌘Q), not by closing its window,
    // and hides (⌘H) and minimizes (⌘M) like every other Mac app.
    if cfg!(target_os = "macos") {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &Hide, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &Minimize, cx| {
            if let Some(w) = cx.active_window() {
                let _ = w.update(cx, |_, window, _| window.minimize_window());
            }
        });
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
            KeyBinding::new("cmd-m", Minimize, None),
        ]);
        cx.set_menus([
            Menu::new("Kai").items([
                MenuItem::action("Hide Kai", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::separator(),
                MenuItem::action("Quit Kai", Quit),
            ]),
            Menu::new("Window").items([MenuItem::action("Minimize", Minimize)]),
        ]);
    }
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
