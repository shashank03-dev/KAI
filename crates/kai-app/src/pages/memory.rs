//! Settings › Memory: what Kai learned from earlier runs on this computer
//! (docs/MEMORY.md). Every lesson can be read and deleted; you can add your
//! own. Lessons are hints for the model only.

use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    data_display::{Badge, Tone},
    feedback::{EmptyState, Toast},
    primitives::{IconName, Severity},
    settings::SettingsSection,
    theme::{ActiveTheme, TextSize},
};
use gpui::{Context, IntoElement, ParentElement, SharedString, Styled, div};
use kai_core::memory::promote::{self, Suggestion};
use kai_core::memory::{Kind, Lesson, MemoryStore, Scope, scope};
use kai_core::settings::Settings;

fn store() -> Option<MemoryStore> {
    MemoryStore::load(Settings::memory_dir())
        .map_err(|e| log::warn!("memory: {e}"))
        .ok()
}

/// Lessons, most confident first, and skill suggestions.
pub fn load() -> (Vec<Lesson>, Vec<Suggestion>) {
    let Some(s) = store() else {
        return (Vec::new(), Vec::new());
    };
    let lessons = s.lessons();
    let plans = kai_core::PlanCache::load(Settings::dir().join("plans"))
        .map(|c| c.plans())
        .unwrap_or_default();
    let suggestions = promote::suggestions(&lessons, &plans);
    (lessons, suggestions)
}

fn tone(k: Kind) -> Tone {
    match k {
        Kind::Avoid => Tone::Danger,
        Kind::Prefer => Tone::Success,
        Kind::Fact => Tone::Info,
        Kind::Preference => Tone::Accent,
    }
}

/// "mousepad, github.com · 75% · used 3 times".
pub fn meta(l: &Lesson) -> String {
    let mut where_: Vec<String> = l.scope.apps.clone();
    where_.extend(l.scope.domains.iter().cloned());
    let where_ = if where_.is_empty() {
        "Any app".to_string()
    } else {
        where_.join(", ")
    };
    let used = match l.uses {
        0 => "not used yet".to_string(),
        1 => "used once".to_string(),
        n => format!("used {n} times"),
    };
    format!("{where_} · {:.0}% sure · {used}", l.confidence() * 100.0)
}

fn lesson_row(l: &Lesson, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let theme = cx.theme();
    let me = cx.entity();
    let id = l.id.clone();
    div()
        .py_2()
        .flex()
        .items_start()
        .gap_3()
        .border_b_1()
        .border_color(theme.colors.border)
        .child(Badge::new(l.kind.as_str()).tone(tone(l.kind)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(l.text.clone())
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Sm))
                        .text_color(theme.colors.fg_muted)
                        .child(meta(l)),
                ),
        )
        .child(
            Button::new(SharedString::from(format!("forget-{id}")), "Delete")
                .variant(ButtonVariant::Ghost)
                .on_click(move |_, _, cx| {
                    me.update(cx, |a, cx| {
                        let ok = store().is_some_and(|s| s.forget(&id).unwrap_or(false));
                        a.pages.reload_memory();
                        if !ok {
                            a.toast(
                                Toast::new("Couldn't delete the lesson").severity(Severity::Danger),
                                cx,
                            );
                        }
                        cx.notify();
                    })
                }),
        )
}

/// Add the text in the "Add a lesson" field as your own preference.
fn add_lesson(a: &mut KaiApp, cx: &mut Context<KaiApp>) {
    let text = a.pages.lesson_draft.read(cx).text().trim().to_string();
    if text.is_empty() {
        return;
    }
    let sc = Scope {
        apps: scope::apps_in(&text, &[]),
        domains: scope::domains_in(&text),
        task_words: scope::task_words(&text),
    };
    let r = store()
        .ok_or_else(|| anyhow::anyhow!("the memory folder can't be opened"))
        .and_then(|s| s.add_user(Kind::Preference, &text, sc));
    match r {
        Ok(_) => {
            a.pages.lesson_draft.update(cx, |f, cx| f.set_text("", cx));
            a.pages.reload_memory();
            a.toast(Toast::new("Lesson added").severity(Severity::Success), cx);
        }
        Err(e) => a.toast(
            Toast::new("Lesson not added")
                .body(e.to_string())
                .severity(Severity::Warning),
            cx,
        ),
    }
    cx.notify();
}

pub fn section(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let (adder, clearer) = (cx.entity(), cx.entity());
    let n = app.pages.lessons.len();
    let mut sec = SettingsSection::new("Memory")
        .description(
            "What Kai learned from earlier runs on this computer. The few that fit a task are shown to the model as hints; they never change what needs your approval. Stored only here.",
        )
        .row(crate::pages::settings::row(
            "Add a lesson",
            "Tell Kai how you want things done, e.g. \"Save notes to ~/Notes\" (at most 30 words).",
            div()
                .flex()
                .gap_2()
                .child(div().flex_1().child(app.pages.lesson_draft.clone()))
                .child(Button::new("add-lesson", "Add").on_click(move |_, _, cx| {
                    adder.update(cx, add_lesson)
                })),
            cx,
        ));
    for sg in &app.pages.memory_suggestions {
        let me = cx.entity();
        let draft = sg.draft.clone();
        sec = sec.row(crate::pages::settings::row(
            &format!("Suggested skill for {}", sg.scope),
            &format!(
                "\"{}\" has a remembered opening and {} lessons that helped. Review it as a skill draft; nothing is installed unless you save it.",
                sg.task,
                sg.lessons.len()
            ),
            Button::new(SharedString::from(format!("suggest-{}", sg.scope)), "Draft a skill")
                .icon(IconName::BookOpen)
                .on_click(move |_, _, cx| {
                    me.update(cx, |a, cx| {
                        let text = draft.clone();
                        a.pages.skill_draft.update(cx, |f, cx| f.set_text(text, cx));
                        a.pages.drafting = true;
                        a.go(crate::app::Page::Skills, cx);
                    })
                }),
            cx,
        ));
    }
    if n == 0 {
        return sec.row(
            EmptyState::new("no-lessons", IconName::BookOpen, "Nothing learned yet")
                .body("When a run goes wrong and Kai finds what works, it remembers it here."),
        );
    }
    for l in &app.pages.lessons {
        sec = sec.row(lesson_row(l, cx));
    }
    sec.row(crate::pages::settings::row(
        "Forget everything",
        &format!("{n} lessons on this computer."),
        Button::new("clear-memory", "Clear all")
            .variant(ButtonVariant::Danger)
            .on_click(move |_, _, cx| {
                clearer.update(cx, |a, cx| {
                    let r = store().map(|s| s.clear());
                    a.pages.reload_memory();
                    let t = match r {
                        Some(Ok(())) => Toast::new("Memory cleared").severity(Severity::Success),
                        _ => Toast::new("Couldn't clear memory").severity(Severity::Danger),
                    };
                    a.toast(t, cx);
                    cx.notify();
                })
            }),
        cx,
    ))
}
