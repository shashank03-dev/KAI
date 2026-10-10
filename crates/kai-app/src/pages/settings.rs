//! Settings: model and server, safety, appearance, privacy, shortcuts, about.

use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, SegmentedControl},
    feedback::Toast,
    forms::Switch,
    layout::ScrollArea,
    overlays::ConfirmDialog,
    primitives::{IconName, Severity},
    settings::{Appearance, KeyboardShortcutsList, SettingsLayout, SettingsSection, ThemeSelector},
    theme::{ActiveTheme, Mode, TextSize, Theme},
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
        .row(row("Stop shortcut", &format!("{} from any app, or the pointer in the top-left corner.", app.settings.kill_hotkey), div(), cx))
        .row(Switch::new("team-default", app.settings.team_mode).label("Use sub-agents by default").on_change(move |v, _, cx| team.update(cx, |a, cx| { a.settings.team_mode = v; a.team = v; a.save_settings(cx); })))
}

fn appearance(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let me = cx.entity();
    let current = match app.settings.theme.as_str() {
        "dark" => Appearance::Dark,
        "light" => Appearance::Light,
        _ => Appearance::System,
    };
    SettingsSection::new("Appearance").row(ThemeSelector::new(
        "theme-sel",
        current,
        move |a, _, cx| {
            let mode = match a {
                Appearance::Dark => Some(Mode::Dark),
                Appearance::Light => Some(Mode::Light),
                Appearance::System => None,
            };
            if let Some(m) = mode {
                Theme::set_mode(m, cx);
            }
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
                    ("shortcuts", "Shortcuts", IconName::Keyboard),
                    ("about", "About", IconName::Info),
                ],
                section,
                move |key, _, cx| {
                    picker.update(cx, |a, cx| {
                        a.pages.settings_section = key.to_string();
                        cx.notify();
                    })
                },
            )
            .page(ScrollArea::new("settings-page").vertical().child(page)),
        )
        .children(confirm)
}
