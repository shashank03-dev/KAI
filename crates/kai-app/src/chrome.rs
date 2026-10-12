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
        // The reason stays one hover away instead of a bare "No model reachable".
        let tip = match &self.conn {
            Conn::Offline(reason) => {
                format!("{}. Click to test again.", reason.trim_end_matches('.'))
            }
            _ => "Model connection. Click to test again.".to_string(),
        };
        let run = self.active_state().unwrap_or_else(|| self.run.clone());
        let latency = run
            .read(cx)
            .last_model_ms
            .map(|ms| format!("last step {ms} ms"))
            .unwrap_or_else(|| "no steps yet".into());
        let queued = self.queued_count(cx);
        let (stop_keys, stop_label) = match crate::StopHotkey::label(cx) {
            Some(k) => (Some(KbdCombo::new(&k)), "stops Kai"),
            None => (None, "Stop: top-left corner or Ctrl+."),
        };
        let switcher = crate::switcher::switcher("status-model", self, cx);
        let this = cx.entity();
        StatusBar::new()
            .left(ConnectionStatus::new("conn", state))
            .left(
                StatusBarItem::new("conn-label")
                    .label(label)
                    .tooltip(tip)
                    .on_click(move |_, window, cx| {
                        this.update(cx, |app, cx| app.check_connection(window, cx))
                    }),
            )
            .left(switcher)
            .right(
                StatusBarItem::new("queue")
                    .icon(IconName::ListOrdered)
                    .label(match (queued, self.queue_held) {
                        (0, _) => "queue empty".to_string(),
                        (n, true) => format!("{n} queued · paused"),
                        (n, false) => format!("{n} queued"),
                    }),
            )
            .right(
                StatusBarItem::new("latency")
                    .icon(IconName::Gauge)
                    .label(latency),
            )
            .right({
                let item = StatusBarItem::new("stop-key").label(stop_label);
                match stop_keys {
                    Some(k) => item.leading(k),
                    None => item,
                }
            })
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
        // Run controls only while a run has the screen, and only the one that applies.
        let paused = self.active_state().map(|s| s.read(cx).paused);
        let mut kai = vec![cmd("new", "New task", IconName::Plus, Some("ctrl-n"))];
        if let Some(paused) = paused {
            kai.push(cmd(
                "stop",
                "Stop Kai",
                IconName::CircleStop,
                Some("ctrl-."),
            ));
            kai.push(if paused {
                cmd("resume", "Resume the running task", IconName::Play, None)
            } else {
                cmd("pause", "Pause the running task", IconName::Pause, None)
            });
        }
        kai.extend([
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
        ]);
        CommandPalette::new("palette", move |_, cx| {
            closer.update(cx, |app, cx| {
                app.palette = false;
                cx.notify();
            })
        })
        .group("Kai", kai)
        .group(
            "Go to",
            [
                cmd("go:run", "Run", IconName::Play, None),
                cmd(
                    "go:models",
                    "Models (run on this computer)",
                    IconName::Cpu,
                    None,
                ),
                cmd("go:skills", "Skills", IconName::Puzzle, None),
                cmd("go:history", "History", IconName::History, None),
                cmd("go:bench", "Benchmarks", IconName::ChartColumn, None),
                cmd("go:settings", "Settings", IconName::Settings, None),
            ],
        )
        .group(
            "Switch model",
            kai_core::settings::MODELS
                .iter()
                .map(|m| cmd(&format!("model:{}", m.id), m.label, IconName::Cpu, None))
                .collect::<Vec<_>>(),
        )
        .group(
            "Local models",
            crate::switcher::installed_local(self, cx)
                .into_iter()
                .map(|(id, label)| {
                    cmd(
                        &format!("local:{id}"),
                        &format!("Use {label} on this computer"),
                        IconName::HardDrive,
                        None,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .group(
            "Switch backend",
            crate::switcher::BACKENDS
                .iter()
                .map(|b| {
                    cmd(
                        &format!("backend:{}", crate::switcher::backend_key(*b)),
                        crate::runs::backend_label(*b),
                        IconName::Server,
                        None,
                    )
                })
                .collect::<Vec<_>>(),
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
                    "new" => app.select(None, cx),
                    "pause" | "resume" => {
                        if let Some(id) = app.active {
                            app.set_paused(id, value.as_ref() == "pause", cx)
                        }
                    }
                    v if v.starts_with("local:") => app.local_use(&v[6..], cx),
                    v if v.starts_with("model:") => crate::switcher::set_model(app, &v[6..], cx),
                    v if v.starts_with("backend:") => {
                        let b = crate::switcher::backend_from_key(&v[8..]);
                        crate::switcher::set_backend(app, b, cx);
                        app.check_connection(window, cx);
                    }
                    "stop" => app.stop(cx),
                    "theme" => app.toggle_theme(cx),
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
                            app.select(None, cx);
                        }
                    }
                    _ => {}
                }
                cx.notify();
            })
        })
    }
}
