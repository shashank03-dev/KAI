//! Settings: model and server, safety, appearance, privacy, shortcuts, about.

use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, SegmentedControl},
    feedback::{InlineMessage, Toast},
    forms::Switch,
    layout::ScrollArea,
    overlays::ConfirmDialog,
    primitives::{IconName, Severity},
    settings::{Appearance, KeyboardShortcutsList, SettingsLayout, SettingsSection, ThemeSelector},
    theme::{ActiveTheme, Mode, TextSize, Theme},
    typography::KbdCombo,
};
use gpui::{Context, IntoElement, ParentElement, Styled, Window, div};
use kai_core::risk::ApprovalLevel;
use kai_core::settings::Settings;

pub(crate) fn row(
    label: &str,
    hint: &str,
    control: impl IntoElement,
    cx: &Context<KaiApp>,
) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .py_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().child(label.to_string()))
        .child(
            div()
                .text_size(theme.text_size(TextSize::Sm))
                .text_color(theme.colors.fg_muted)
                .child(hint.to_string()),
        )
        .child(control)
}

/// The stop shortcut actually registered (it can differ from the chosen one
/// when another app holds it), or why there is none.
fn stop_row(cx: &Context<KaiApp>) -> impl IntoElement {
    match crate::StopHotkey::label(cx) {
        Some(keys) => row(
            "Stop shortcut",
            "Works from any app. Pushing the pointer into the top-left corner of the screen also stops Kai.",
            div().flex().child(KbdCombo::new(&keys)),
            cx,
        )
        .into_any_element(),
        None => row(
            "Stop shortcut",
            "Another app holds the shortcut, so it isn't registered. Stop Kai with the top-left corner, Ctrl+. in Kai, or Stop in the mini window.",
            InlineMessage::new(Severity::Warning, "No global stop shortcut this session."),
            cx,
        )
        .into_any_element(),
    }
}

fn safety(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let (level, steps, team) = (cx.entity(), cx.entity(), cx.entity());
    let sel = match app.settings.approval {
        ApprovalLevel::Every => "every",
        ApprovalLevel::HighRiskOnly => "high",
        ApprovalLevel::Autopilot => "auto",
    };
    SettingsSection::new("Safety")
        .description("Payments and passwords always ask, whatever you choose here.")
        .row(row("Ask before", "Risky means sending, buying, deleting, submitting, changing settings, or typing into a password field.", SegmentedControl::new("level", sel)
            .segment("every", "Every action", None).segment("high", "Risky actions", None).segment("auto", "Autopilot", None)
            .on_change(move |v, _, cx| level.update(cx, |a, cx| {
                a.settings.approval = match v.as_ref() { "every" => ApprovalLevel::Every, "auto" => ApprovalLevel::Autopilot, _ => ApprovalLevel::HighRiskOnly };
                a.save_settings(cx);
            })), cx))
        .row(row("Limits", &format!("At most {} steps and {} minutes per task.", app.settings.max_steps, app.settings.max_minutes), SegmentedControl::new("limits", app.settings.max_steps.to_string())
            .segment("20", "20 steps", None).segment("40", "40 steps", None).segment("80", "80 steps", None)
            .on_change(move |v, _, cx| steps.update(cx, |a, cx| { a.settings.max_steps = v.parse().unwrap_or(40); a.save_settings(cx); })), cx))
        .row(stop_row(cx))
        .row(Switch::new("use-executor", app.settings.use_executor).label("Fast executor: a local decision model (jevos) carries out Gemma's plan — experimental").on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.use_executor = v; a.save_settings(cx); }) }))
        .row(Switch::new("verify-outcomes", app.settings.verify_outcomes).label("Check the result before \"done\": Kai probes the outcomes Gemma named (files, window title, address bar) — experimental").on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.verify_outcomes = v; a.save_settings(cx); }) }))
        .row(Switch::new("macro-history", app.settings.macro_history).label("Remember older steps as a short summary instead of only the last three — experimental").on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.macro_history = v; a.save_settings(cx); }) }))
        .row(Switch::new("memo", app.settings.memo).label("Let the model keep a short memo of what it read, for long tasks — experimental").on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.memo = v; a.save_settings(cx); }) }))
        .row(Switch::new("task-checks", app.settings.task_checks).label("Check the file or folder a task asks for before accepting done — experimental").on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.task_checks = v; a.save_settings(cx); }) }))
        .row(Switch::new("procedures", app.settings.use_procedures).label("Replay remembered procedures for tasks Kai has done before, with no model calls — experimental").on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.use_procedures = v; a.save_settings(cx); }) }))
        .row(Switch::new("best-of", app.settings.best_of_plans).label("Plan long tasks three ways first and follow the plan they agree on (3 extra model calls) — experimental").on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.best_of_plans = v; a.save_settings(cx); }) }))
        .row(Switch::new("recover", app.settings.recover).label("When nothing changes, work out why and try another way once before asking — experimental").on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.recover = v; a.save_settings(cx); }) }))
        .row(Switch::new("use-browser", app.settings.use_browser).label(if cfg!(target_os = "linux") { "Do tasks in Kai's own browser: pages are read from the browser itself and Kai works in the background, without your mouse — experimental" } else { "Do tasks in Kai's own browser (Linux only for now) — experimental" }).disabled(!cfg!(target_os = "linux")).on_change({ let e = cx.entity(); move |v, _, cx| e.update(cx, |a, cx| { a.settings.use_browser = v; a.save_settings(cx); }) }))
        .row(Switch::new("team-default", app.settings.team_mode).label("Use sub-agents by default").on_change(move |v, _, cx| team.update(cx, |a, cx| { a.settings.team_mode = v; a.team = v; a.save_settings(cx); })))
}

fn appearance(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let me = cx.entity();
    let glass_me = cx.entity();
    let glass = Switch::new("glass", app.settings.glass)
        .label("Glass window: let the desktop show through, softly blurred (Mica on Windows 11)")
        .on_change(move |on, window, cx| {
            crate::brand::apply(on, cx);
            window.set_background_appearance(crate::material(on));
            glass_me.update(cx, |a, cx| {
                a.settings.glass = on;
                a.save_settings(cx);
            })
        });
    let current = match app.settings.theme.as_str() {
        "dark" => Appearance::Dark,
        "light" => Appearance::Light,
        _ => Appearance::System,
    };
    SettingsSection::new("Appearance")
        .description("Kai Glass: graphite or porcelain, one ember light for what is alive.")
        .row(glass)
        .row(row(
            "If text is hard to read",
            "Without a compositor that blurs, the desktop shows through unblurred. Turn glass off for a solid window.",
            div(),
            cx,
        ))
        .row(ThemeSelector::new(
        "theme-sel",
        current,
        move |a, _, cx| {
            let mode = match a {
                Appearance::Dark => Mode::Dark,
                Appearance::Light => Mode::Light,
                Appearance::System => crate::system_mode(cx),
            };
            Theme::set_mode(mode, cx);
            me.update(cx, |app, cx| {
                app.settings.theme = match a {
                    Appearance::Dark => "dark",
                    Appearance::Light => "light",
                    Appearance::System => "system",
                }
                .into();
                app.save_settings(cx);
            })
        },
    ))
}

fn privacy(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let asker = cx.entity();
    let rec = cx.entity();
    SettingsSection::new("Privacy")
        .description(
            "Kai sends screenshots only to your model endpoint. Traces never leave this computer.",
        )
        .row(row(
            "Traces folder",
            &Settings::traces_dir().display().to_string(),
            div(),
            cx,
        ))
        .row(row(
            "Saved runs",
            &format!("{} runs on disk.", app.pages.history.len()),
            Button::new("delete-all", "Delete all history")
                .variant(ButtonVariant::Danger)
                .on_click(move |_, _, cx| {
                    asker.update(cx, |a, cx| {
                        a.pages.confirm_delete = true;
                        cx.notify();
                    })
                }),
            cx,
        ))
        .row(
            Switch::new("record-traces", app.settings.record_traces)
                .label("Keep runs as training data (prompts, replies and screenshots, saved only on this computer)")
                .on_change(move |v, _, cx| {
                    rec.update(cx, |a, cx| {
                        a.settings.record_traces = v;
                        a.save_settings(cx);
                    })
                }),
        )
        .row(row(
            "Training data folder",
            &Settings::runs_dir().display().to_string(),
            div(),
            cx,
        ))
        .row(
            Switch::new("learn", app.settings.learn)
                .label("Learn from mistakes on this computer (short lessons, kept only here; see Settings › Memory)")
                .on_change({
                    let e = cx.entity();
                    move |v, _, cx| e.update(cx, |a, cx| { a.settings.learn = v; a.save_settings(cx); })
                }),
        )
        .row(
            Switch::new("reflect", app.settings.reflect)
                .label("Reflect after failures (one extra model call)")
                .on_change({
                    let e = cx.entity();
                    move |v, _, cx| e.update(cx, |a, cx| { a.settings.reflect = v; a.save_settings(cx); })
                }),
        )
}

fn about(cx: &mut Context<KaiApp>) -> SettingsSection {
    SettingsSection::new("About")
        .row(row("Kai", concat!("Version ", env!("CARGO_PKG_VERSION"), " · MIT licence"), div(), cx))
        .row(row("Credits", "Gemma 4 by Google DeepMind (Apache 2.0). UI by Ely GPUI Components (MIT) on GPUI. Icons by Lucide (ISC). Fonts: Inter, JetBrains Mono, IBM Plex Sans (OFL).", div(), cx))
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let section = app.pages.settings_section.clone();
    let page = match section.as_str() {
        "safety" => safety(app, cx).into_any_element(),
        "appearance" => appearance(app, cx).into_any_element(),
        "privacy" => privacy(app, cx).into_any_element(),
        "memory" => crate::pages::memory::section(app, cx).into_any_element(),
        "shortcuts" => SettingsSection::new("Shortcuts")
            .row(KeyboardShortcutsList::new(
                crate::pages::shortcut_list(),
                &app.pages.shortcut_search,
            ))
            .into_any_element(),
        "about" => about(cx).into_any_element(),
        _ => crate::pages::model_settings::model(app, cx).into_any_element(),
    };
    let picker = cx.entity();
    let confirm_memory = app.pages.confirm_clear_memory.then(|| {
        let (closer, doer) = (cx.entity(), cx.entity());
        ConfirmDialog::new(
            "confirm-clear-memory",
            "Forget everything Kai learned?",
            "Every lesson on this computer is deleted. Kai will have to learn them again. This can't be undone.",
            move |_, cx| {
                closer.update(cx, |a, cx| {
                    a.pages.confirm_clear_memory = false;
                    cx.notify();
                })
            },
        )
        .confirm("Clear all")
        .destructive()
        .on_confirm(move |_, cx| {
            doer.update(cx, |a, cx| {
                a.pages.confirm_clear_memory = false;
                crate::pages::memory::clear_all(a, cx);
            })
        })
    });
    let confirm = app.pages.confirm_delete.then(|| {
        let (closer, doer) = (cx.entity(), cx.entity());
        ConfirmDialog::new("confirm-delete", "Delete all history?", "Every saved run and its screenshots are removed from this computer. This can't be undone.", move |_, cx| {
            closer.update(cx, |a, cx| { a.pages.confirm_delete = false; cx.notify(); })
        })
        .confirm("Delete all")
        .destructive()
        .on_confirm(move |_, cx| doer.update(cx, |a, cx| {
            let r = std::fs::remove_dir_all(Settings::traces_dir());
            a.pages.reload_history();
            a.pages.open_trace = None;
            let t = match r {
                Ok(()) => Toast::new("History deleted").severity(Severity::Success),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Toast::new("History deleted").severity(Severity::Success),
                Err(e) => Toast::new("Couldn't delete history").body(e.to_string()).severity(Severity::Danger),
            };
            a.toast(t, cx);
        }))
    });
    div()
        .size_full()
        .child(
            SettingsLayout::new(
                "settings",
                [
                    ("model", "Model & server", IconName::Server),
                    ("safety", "Safety", IconName::Shield),
                    ("appearance", "Appearance", IconName::Sun),
                    ("privacy", "Privacy", IconName::Lock),
                    ("memory", "Memory", IconName::BookOpen),
                    ("shortcuts", "Shortcuts", IconName::Keyboard),
                    ("about", "About", IconName::Info),
                ],
                section,
                move |key, _, cx| {
                    picker.update(cx, |a, cx| {
                        if key == "memory" {
                            a.pages.reload_memory();
                        }
                        a.pages.settings_section = key.to_string();
                        cx.notify();
                    })
                },
            )
            // A readable column with room on the right and at the end, so
            // fields don't run into the window edge on wide windows.
            .page(
                ScrollArea::new("settings-page").vertical().child(
                    div()
                        .w_full()
                        .max_w(gpui::rems(46.))
                        .pt_2()
                        .pr_6()
                        .pb_8()
                        .child(page),
                ),
            ),
        )
        .children(confirm)
        .children(confirm_memory)
}
