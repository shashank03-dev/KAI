//! The window chrome around the pages: the status bar and the command palette.

use crate::app::{Conn, KaiApp, Page};
use ely_gpui_component::{
    feedback::ConnectionStatus,
    navigation::{Command, CommandPalette},
    primitives::IconName,
    shell::{Connectivity, StatusBar, StatusBarItem},
    typography::KbdCombo,
};
use gpui::{Context, IntoElement};

impl KaiApp {
    pub(crate) fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (state, label) = match &self.conn {
            Conn::Unknown => (
                Connectivity::Reconnecting,
                "Connecting to Gemma…".to_string(),
            ),
            Conn::Online(s) => (Connectivity::Online, s.clone()),
            Conn::Fallback(s) => (Connectivity::Reconnecting, format!("Gemini fallback · {s}")),
            Conn::Offline(_) => (Connectivity::Offline, "No model reachable".to_string()),
        };
        let run = self.run.read(cx);
        let latency = run
            .last_model_ms
            .map(|ms| format!("last step {ms} ms"))
            .unwrap_or_else(|| "no steps yet".into());
        let model = self.settings.fast_model.clone();
        let this = cx.entity();
        StatusBar::new()
            .left(ConnectionStatus::new("conn", state))
            .left(
                StatusBarItem::new("conn-label")
                    .label(label)
                    .tooltip("Model connection. Click to test again.")
                    .on_click(move |_, window, cx| {
                        this.update(cx, |app, cx| app.check_connection(window, cx))
                    }),
            )
            .left(StatusBarItem::new("model").icon(IconName::Cpu).label(model))
            .right(
                StatusBarItem::new("latency")
                    .icon(IconName::Gauge)
                    .label(latency),
            )
            .right(
                StatusBarItem::new("stop-key")
                    .leading(KbdCombo::new("ctrl-alt-escape"))
                    .label("stops Kai"),
            )
    }

    pub(crate) fn palette(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (closer, runner) = (cx.entity(), cx.entity());
        let cmd = |value: &str, label: &str, icon: IconName, keys: Option<&str>| {
            let mut c = Command::new(value.to_string(), label.to_string()).icon(icon);
            if let Some(k) = keys {
                c = c.keys(k.to_string());
            }
            c
        };
        CommandPalette::new("palette", move |_, cx| {
            closer.update(cx, |app, cx| {
                app.palette = false;
                cx.notify();
            })
        })
        .group(
            "Kai",
            [
                cmd("new", "New task", IconName::Plus, Some("ctrl-n")),
                cmd("stop", "Stop Kai", IconName::CircleStop, Some("ctrl-.")),
                cmd(
                    "theme",
                    "Toggle light / dark",
                    IconName::Moon,
                    Some("ctrl-shift-l"),
                ),
                cmd(
                    "shortcuts",
                    "Keyboard shortcuts",
                    IconName::Keyboard,
                    Some("ctrl-/"),
                ),
            ],
        )
        .group(
            "Go to",
            [
                cmd("go:run", "Run", IconName::Play, None),
                cmd("go:skills", "Skills", IconName::Puzzle, None),
                cmd("go:history", "History", IconName::History, None),
                cmd("go:bench", "Benchmarks", IconName::ChartColumn, None),
                cmd("go:settings", "Settings", IconName::Settings, None),
            ],
        )
        .group(
            "Run a skill",
            self.skills
                .iter()
                .map(|s| {
                    cmd(
                        &format!("skill:{}", s.front.name),
                        &s.front.name,
                        IconName::Sparkles,
                        None,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .on_run(move |value, window, cx| {
            runner.update(cx, |app, cx| {
                app.palette = false;
                match value.as_ref() {
                    "new" => app.go(Page::Run, cx),
                    "stop" => app.stop(cx),
                    "theme" => Self::toggle_theme(cx),
                    "shortcuts" => app.shortcuts = true,
                    v if v.starts_with("go:") => {
                        if let Some(p) = Page::from_key(&v[3..]) {
                            app.go(p, cx)
                        }
                    }
                    v if v.starts_with("skill:") => {
                        let name = v[6..].to_string();
                        if let Some(s) = app.skills.iter().find(|s| s.front.name == name) {
                            let hint = format!("Use the {} skill: ", s.front.name);
                            app.draft.update(cx, |d, cx| d.set_text(hint, cx));
                            app.go(Page::Run, cx);
                            let _ = window;
                        }
                    }
                    _ => {}
                }
                cx.notify();
            })
        })
    }
}
