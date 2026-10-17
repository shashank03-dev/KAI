//! Idle: Kai's mark, a greeting, the composer as the one raised glass layer,
//! the keys that matter, and starters drawn from enabled skills as cards.

use crate::anim;
use crate::app::{Conn, KaiApp, Page};
use crate::i18n::t;
use crate::ui::{self, Glass, R_LG, R_SM};
use ely_gpui_component::{
    chat::PromptInput,
    feedback::Banner,
    forms::Switch,
    layout::ScrollArea,
    primitives::{Icon, IconName, Severity},
    theme::{ActiveTheme, IconSize},
};
use gpui::{
    AnyElement, App, BoxShadow, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window, div, px,
};
use std::collections::BTreeSet;

/// Example tasks for the built-in skills, shown when the skill is installed.
const EXAMPLES: &[(&str, &str, IconName)] = &[
    (
        "open-and-search-web",
        "Open bmsce.ac.in and find the IEEE CS page",
        IconName::Globe,
    ),
    (
        "run-terminal-task",
        "Find the 5 biggest files in my Downloads folder",
        IconName::Terminal,
    ),
    (
        "rename-files-in-folder",
        "Add today's date to every file name in ~/Pictures/trip",
        IconName::FolderOpen,
    ),
    (
        "pc-security-checkup",
        "Check my PC's firewall, updates and open ports",
        IconName::Shield,
    ),
    (
        "system-settings-dark-mode",
        "Switch my computer to dark mode",
        IconName::Moon,
    ),
    (
        "spotify-or-youtube-play",
        "Play lofi hip hop radio on YouTube",
        IconName::Music,
    ),
];

/// At most this many starter cards.
const MAX_STARTERS: usize = 6;

/// A starter card. An example runs at once; one of your own skills puts its
/// task into the composer to read and send.
#[derive(Debug, Clone, PartialEq)]
pub enum Starter {
    Example {
        skill: &'static str,
        task: &'static str,
        icon: IconName,
    },
    Own {
        skill: String,
        task: String,
    },
}

/// Your own enabled skills first, then examples for the enabled built-ins.
pub fn starters(skills: &[kai_core::Skill], disabled: &BTreeSet<String>) -> Vec<Starter> {
    let on = |s: &&kai_core::Skill| !disabled.contains(&s.front.name);
    let own = skills
        .iter()
        .filter(|s| !s.builtin)
        .filter(on)
        .map(|s| Starter::Own {
            skill: s.front.name.clone(),
            task: task_of(&s.front.description),
        });
    let examples = EXAMPLES
        .iter()
        .filter(|(name, ..)| {
            skills
                .iter()
                .filter(on)
                .any(|s| s.builtin && s.front.name == *name)
        })
        .map(|&(skill, task, icon)| Starter::Example { skill, task, icon });
    own.chain(examples).take(MAX_STARTERS).collect()
}

/// A skill's description as a task: the part before "Use when …" (how
/// Save as skill writes it), else the description itself.
fn task_of(description: &str) -> String {
    description
        .split(" Use when")
        .next()
        .unwrap_or(description)
        .trim()
        .to_string()
}

/// "just now", "12m ago", "3h ago", "2d ago".
fn ago(at: jiff::Timestamp) -> String {
    let s = jiff::Timestamp::now().as_second() - at.as_second();
    match s {
        ..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", s / 60),
        3600..=86_399 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86_400),
    }
}

fn greeting(hour: u8) -> &'static str {
    match hour {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        18..=22 => "Good evening",
        _ => "Working late",
    }
}

/// "ctrl-alt-escape" as key caps.
fn keys(combo: &str, cx: &App) -> Vec<AnyElement> {
    combo
        .split('-')
        .map(|k| ui::kbd(ui::key_label(k), cx).into_any_element())
        .collect()
}

/// One hint: key caps and what they do.
fn hint(caps: Vec<AnyElement>, what: &str, g: &Glass) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .children(caps)
        .child(
            div()
                .pl_1()
                .text_size(px(12.))
                .text_color(g.faint)
                .child(what.to_string()),
        )
        .into_any_element()
}

/// The connection as one line over the greeting: a dot and the model.
fn status_line(app: &KaiApp, g: &Glass, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let (color, live, word) = match &app.conn {
        Conn::Unknown => (g.faint, false, "not checked"),
        Conn::Online(_) => (colors.success, true, "ready"),
        Conn::Fallback(_) => (colors.warning, true, "on fallback"),
        Conn::Offline(_) => (colors.danger, false, "offline"),
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py(px(5.))
        .rounded_full()
        .bg(g.card)
        .border_1()
        .border_color(g.line)
        // A steady glow, not the breathing dot: the connection is "ready"
        // whenever Kai sits idle, and a looping animation there would keep
        // the window redrawing forever (anim.rs: nothing loops while idle).
        // The busy line below breathes while a run is going.
        .child(ui::dot(color, live, g))
        .child(ui::eyebrow(
            format!("{word} · {}", crate::runs::model_label(&app.settings)),
            cx,
        ))
        .into_any_element()
}

pub fn render(app: &mut KaiApp, window: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let hour = jiff::Zoned::now().hour() as u8;
    let g = Glass::of(cx);

    let starters = if app.settings.gui_only {
        Vec::new()
    } else {
        starters(&app.skills, &app.pages.disabled)
    };

    let send = cx.entity();
    let model = crate::switcher::switcher("composer-model", app, cx);
    let team_pick = cx.entity();
    let team = Switch::new("team", app.team)
        .label(t(cx, "home.team", &[]))
        .disabled(app.settings.gui_only)
        .on_change(move |on, _, cx| {
            team_pick.update(cx, |a, cx| {
                a.team = on;
                cx.notify();
            })
        });

    let offline = match &app.conn {
        Conn::Offline(reason) => {
            let go = cx.entity();
            Some(
                Banner::new("offline", Severity::Warning, reason.clone())
                    .action("Open settings", move |_, cx| {
                        go.update(cx, |a, cx| a.go(Page::Settings, cx))
                    }),
            )
        }
        _ => None,
    };
    let no_hands = (!cx.global::<crate::AgentHandle>().0.can_act())
        .then(|| Banner::new("no-hands", Severity::Danger, "Kai needs mouse/keyboard access. On macOS, enable Kai in System Settings › Privacy & Security › Accessibility and Screen Recording. On Linux, use an Xorg session."));
    let gui_mode = app.settings.gui_only.then(|| Banner::new("gui-mode", Severity::Info, "GUI testing is prepared. Press Run when ready. Watch Steps and screenshots; use Pause, Stop or a steering message if needed. No shell tasks or cached procedures run in this mode."));
    let busy = app.active.map(|_| {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(anim::live_dot(g.ember, "busy-dot", &g))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(g.dim)
                    .child("Kai is busy with another task. New tasks wait in the queue."),
            )
    });

    // The composer is the one raised layer on the page, with a faint ember
    // halo so the eye lands there first.
    let composer = div()
        .w_full()
        .rounded(px(R_LG + 2.))
        .shadow(vec![
            g.lift(),
            BoxShadow::new(px(0.), px(0.), g.ember_soft).blur_radius(px(36.)),
        ])
        .child(
            PromptInput::new("composer", &app.draft, move |window, cx| {
                send.update(cx, |a, cx| a.start_run(None, window, cx))
            })
            .tool(model)
            .tool(team),
        );

    let mut hints = vec![
        hint(vec![ui::kbd("↵", cx).into_any_element()], "run", &g),
        hint(keys("secondary-k", cx), "commands", &g),
    ];
    if let Some(stop) = crate::StopHotkey::label(cx) {
        hints.push(hint(keys(&stop, cx), "stops Kai anywhere", &g));
    }

    let has_starters = !starters.is_empty();
    let cards = starters.into_iter().enumerate().map(|(ix, starter)| {
        let (skill, task, icon) = match &starter {
            Starter::Example { skill, task, icon } => (skill.to_string(), task.to_string(), *icon),
            Starter::Own { skill, task } => (skill.clone(), task.clone(), IconName::Puzzle),
        };
        let pick = cx.entity();
        let (hover, strong) = (g.card_hover, g.line_strong);
        let card = div()
            .id(("starter", ix))
            .p(px(14.))
            .flex()
            .items_center()
            .gap_3()
            .cursor_pointer()
            .bg(g.card)
            .border_1()
            .border_color(g.line)
            .rounded(px(ui::R_MD))
            .shadow(vec![g.sheen_shadow()])
            .hover(move |s| s.bg(hover).border_color(strong))
            .on_click(move |_, window, cx| {
                pick.update(cx, |a, cx| match &starter {
                    Starter::Example { task, .. } => {
                        a.start_run(Some(task.to_string()), window, cx)
                    }
                    Starter::Own { task, .. } => {
                        let task = task.clone();
                        a.draft.update(cx, |d, cx| d.set_text(task, cx));
                        cx.notify();
                    }
                })
            })
            .child(
                div()
                    .size(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(R_SM))
                    .bg(g.raised)
                    .border_1()
                    .border_color(g.line)
                    .child(Icon::new(icon).size(IconSize::Sm).color(g.dim)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(13.5))
                            .text_color(g.text)
                            .line_clamp(2)
                            .child(task),
                    )
                    .child(ui::mono(skill, g.faint, cx).text_size(px(11.))),
            )
            .child(div().text_color(g.faint).child("→"));
        anim::enter(card, ("starter-in", ix), 6 + ix)
    });

    // Raycast-style recents: the last few distinct tasks, one click back into
    // the composer to edit and run again.
    let mut seen = std::collections::HashSet::new();
    let recents: Vec<AnyElement> = app
        .pages
        .history
        .iter()
        .filter(|e| seen.insert(e.task.clone()))
        .take(3)
        .enumerate()
        .map(|(ix, e)| {
            let filler = cx.entity();
            let task = e.task.clone();
            let hover = g.card_hover;
            div()
                .id(("recent", ix))
                .px(px(12.))
                .py(px(9.))
                .flex()
                .items_center()
                .gap_3()
                .cursor_pointer()
                .rounded(px(R_SM + 2.))
                .hover(move |s| s.bg(hover))
                .on_click(move |_, _, cx| {
                    let task = task.clone();
                    filler.update(cx, |a, cx| {
                        a.draft.update(cx, |d, cx| d.set_text(task, cx));
                        cx.notify();
                    })
                })
                .child(
                    Icon::new(IconName::History)
                        .size(IconSize::Sm)
                        .color(g.faint),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.))
                        .text_color(g.dim)
                        .truncate()
                        .child(e.task.clone()),
                )
                .child(ui::mono(ago(e.at), g.faint, cx).text_size(px(11.)))
                .into_any_element()
        })
        .collect();
    let recent = (!recents.is_empty()).then(|| {
        anim::enter(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .child(ui::eyebrow("Recent · click to edit and run", cx))
                .child(ui::card(&g).p(px(4.)).flex().flex_col().children(recents)),
            "home-recent",
            12,
        )
    });

    let short = window.viewport_size().height < px(650.);
    let column = div()
        .w_full()
        .max_w(px(760.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(if short { 14. } else { 22. }))
        .child(anim::enter(
            div().child(ui::orb(if short { 42. } else { 60. }, &g)),
            "home-orb",
            0,
        ))
        .child(anim::enter(
            div().child(status_line(app, &g, cx)),
            "home-status",
            1,
        ))
        .child(anim::enter(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(if short { 32. } else { 40. }))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(g.text)
                        .child(format!("{}.", greeting(hour))),
                )
                .child(
                    div()
                        .text_size(px(15.))
                        .text_color(g.dim)
                        .child(t(cx, "home.ask", &[])),
                ),
            "home-greeting",
            2,
        ))
        .child(div().h(px(4.)))
        .child(anim::enter(composer, "home-composer", 3))
        .child(anim::enter(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(div().flex().flex_wrap().gap_4().children(hints))
                .children(busy),
            "home-hints",
            4,
        ))
        .child(div().h(px(18.)))
        // No heading over an empty grid: with no skills on, the section
        // is left out.
        .children(has_starters.then(|| {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_3()
                .child(ui::eyebrow("Start from a skill", cx))
                .child(
                    div()
                        .w_full()
                        .grid()
                        .grid_cols(2)
                        .gap(px(10.))
                        .children(cards),
                )
        }))
        .children(recent);

    div()
        .size_full()
        .relative()
        .flex()
        .flex_col()
        .child(ui::aurora(360., &g))
        .children(gui_mode)
        .children(offline)
        .children(no_hands)
        .child(
            div().flex_1().min_h_0().overflow_hidden().child(
                ScrollArea::new("home").size_full().vertical().child(
                    div()
                        .w_full()
                        .flex()
                        .justify_center()
                        .px(px(32.))
                        .pt(px(if short { 24. } else { 64. }))
                        .pb(px(56.))
                        .child(column),
                ),
            ),
        )
}

#[cfg(test)]
#[path = "home_tests.rs"]
mod tests;
