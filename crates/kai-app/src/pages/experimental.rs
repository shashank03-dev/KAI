//! Settings › Experimental: behaviour still being measured, kept apart from
//! the safety choices. Every switch is off by default until a live bench A/B
//! shows it helps (CLAUDE.md), so one button puts them all back.

use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    forms::Switch,
    primitives::IconName,
    settings::SettingsSection,
};
use gpui::{Context, IntoElement, ParentElement, Styled, div};
use kai_core::settings::Settings;

/// One switch: its id, its label, and the setting it flips.
struct Flag {
    id: &'static str,
    label: &'static str,
    field: fn(&mut Settings) -> &mut bool,
}

/// The switches by purpose, in the order the page shows them.
const GROUPS: &[(&str, &[Flag])] = &[
    (
        "Checking the result",
        &[
            Flag {
                id: "verify-outcomes",
                label: "Before \"done\", probe the outcomes Gemma named (files, window title, address bar)",
                field: |s| &mut s.verify_outcomes,
            },
            Flag {
                id: "task-checks",
                label: "Before \"done\", check the file or folder the task asks for",
                field: |s| &mut s.task_checks,
            },
            Flag {
                id: "recover",
                label: "When nothing changes, work out why and try another way once before asking",
                field: |s| &mut s.recover,
            },
        ],
    ),
    (
        "Memory within and across runs",
        &[
            Flag {
                id: "macro-history",
                label: "Remember older steps as a short summary instead of only the last three",
                field: |s| &mut s.macro_history,
            },
            Flag {
                id: "memo",
                label: "Let the model keep a short memo of what it read, for long tasks",
                field: |s| &mut s.memo,
            },
            Flag {
                id: "procedures",
                label: "Replay remembered procedures for tasks Kai has done before, with no model calls",
                field: |s| &mut s.use_procedures,
            },
        ],
    ),
    (
        "Planning and acting",
        &[
            Flag {
                id: "best-of",
                label: "Plan long tasks three ways first and follow the plan they agree on (3 extra model calls)",
                field: |s| &mut s.best_of_plans,
            },
            Flag {
                id: "use-executor",
                label: "Fast executor: a local decision model (jevos) carries out Gemma's plan",
                field: |s| &mut s.use_executor,
            },
        ],
    ),
];

/// Every experimental switch back to its default; nothing else changes.
pub fn reset(s: &mut Settings) {
    let mut defaults = Settings::default();
    for flag in GROUPS.iter().flat_map(|(_, flags)| flags.iter()) {
        *(flag.field)(s) = *(flag.field)(&mut defaults);
    }
    s.use_browser = defaults.use_browser;
}

fn switch(flag: &'static Flag, on: bool, cx: &mut Context<KaiApp>) -> Switch {
    let e = cx.entity();
    Switch::new(flag.id, on)
        .label(flag.label)
        .on_change(move |v, _, cx| {
            e.update(cx, |a, cx| {
                *(flag.field)(&mut a.settings) = v;
                a.save_settings(cx);
            })
        })
}

fn browser(app: &KaiApp, cx: &mut Context<KaiApp>) -> Switch {
    let e = cx.entity();
    Switch::new("use-browser", app.settings.use_browser)
        .label(if cfg!(unix) {
            "Do tasks in Kai's own browser: pages are read from the browser itself and Kai works in the background, without your mouse"
        } else {
            "Do tasks in Kai's own browser (Linux and macOS only for now)"
        })
        .disabled(!cfg!(unix))
        .on_change(move |v, _, cx| {
            e.update(cx, |a, cx| {
                a.settings.use_browser = v;
                a.save_settings(cx);
            })
        })
}

pub fn render(app: &KaiApp, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let resetter = cx.entity();
    let intro = SettingsSection::new("Experimental")
        .description("Still being measured, so all are off by default. Turn one on to try it; each changes how Kai works, not what it may do.")
        .row(
            div().child(
                Button::new("experimental-reset", "Reset to defaults")
                    .variant(ButtonVariant::Secondary)
                    .icon(IconName::RotateCcw)
                    .on_click(move |_, _, cx| {
                        resetter.update(cx, |a, cx| {
                            reset(&mut a.settings);
                            a.save_settings(cx);
                        })
                    }),
            ),
        );
    // The switches read through `field`, which needs a copy it may borrow.
    let mut current = app.settings.clone();
    let groups: Vec<SettingsSection> = GROUPS
        .iter()
        .map(|(title, flags)| {
            flags
                .iter()
                .fold(SettingsSection::new(*title), |section, flag| {
                    let on = *(flag.field)(&mut current);
                    section.row(switch(flag, on, cx))
                })
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .gap_6()
        .child(intro)
        .children(groups)
        .child(SettingsSection::new("Browser").row(browser(app, cx)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags() -> impl Iterator<Item = &'static Flag> {
        GROUPS.iter().flat_map(|(_, flags)| flags.iter())
    }

    #[test]
    fn each_switch_flips_its_own_setting_only() {
        let base = serde_json::to_value(Settings::default()).unwrap();
        let mut ids = std::collections::BTreeSet::new();
        for flag in flags() {
            assert!(ids.insert(flag.id), "{} twice", flag.id);
            let mut s = Settings::default();
            let v = (flag.field)(&mut s);
            *v = !*v;
            let changed = serde_json::to_value(&s).unwrap();
            let diff = base
                .as_object()
                .unwrap()
                .iter()
                .filter(|(k, v)| changed[k.as_str()] != **v)
                .count();
            assert_eq!(diff, 1, "{} changes {diff} settings", flag.id);
        }
    }

    #[test]
    fn reset_restores_every_switch_and_nothing_else() {
        let mut s = Settings::default();
        for flag in flags() {
            *(flag.field)(&mut s) = true;
        }
        s.use_browser = true;
        s.max_steps = 80;
        reset(&mut s);
        let mut d = Settings::default();
        for flag in flags() {
            assert_eq!(*(flag.field)(&mut s), *(flag.field)(&mut d), "{}", flag.id);
        }
        assert!(!s.use_browser);
        assert_eq!(s.max_steps, 80, "not an experimental setting");
    }
}
