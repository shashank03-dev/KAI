//! Idle: the greeting, the composer, and starters drawn from installed skills.

use crate::app::{Conn, KaiApp, Page};
use crate::i18n::t;
use ely_gpui_component::{
    chat::{PromptInput, SuggestionChips, WelcomeScreen},
    feedback::Banner,
    forms::Switch,
    primitives::{IconName, Severity},
    theme::{ActiveTheme, TextSize},
};
use gpui::{Context, IntoElement, ParentElement, Styled, Window, div};

/// One starter per skill, worded as a task.
const STARTERS: &[(&str, &str, IconName)] = &[
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

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let hour = jiff::Zoned::now().hour() as u8;
    let theme = cx.theme();

    let starters: Vec<(String, IconName)> = STARTERS
        .iter()
        .filter(|(skill, ..)| app.skills.iter().any(|s| s.front.name == *skill))
        .map(|(_, text, icon)| (text.to_string(), *icon))
        .collect();
    let chosen = starters.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();

    let (send, pick, team_pick) = (cx.entity(), cx.entity(), cx.entity());
    let model = crate::switcher::switcher("composer-model", app, cx);
    let team = Switch::new("team", app.team)
        .label(t(cx, "home.team", &[]))
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
    let busy = app.active.map(|_| {
        "Kai is working on another task. New tasks wait in the queue and start when the screen is free."
    });
    let no_hands = (!cx.global::<crate::AgentHandle>().0.can_act())
        .then(|| Banner::new("no-hands", Severity::Danger, "Kai can't control the mouse and keyboard on this desktop. On Linux, log in with an Xorg session."));

    div()
        .size_full()
        .flex()
        .flex_col()
        .children(offline)
        .children(no_hands)
        .child(
            div().flex_1().min_h_0().child(
                WelcomeScreen::new("welcome", hour)
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(theme.text_size(TextSize::Sm))
                            .text_color(theme.colors.fg_muted)
                            .child(t(cx, "home.ask", &[])),
                    )
                    .children(busy.map(|b| {
                        div()
                            .w_full()
                            .text_center()
                            .text_size(theme.text_size(TextSize::Xs))
                            .text_color(theme.colors.fg_muted)
                            .child(b)
                    }))
                    .child(
                        PromptInput::new("composer", &app.draft, move |window, cx| {
                            send.update(cx, |a, cx| a.start_run(None, window, cx))
                        })
                        .tool(model)
                        .tool(team),
                    )
                    .child(SuggestionChips::new(
                        "starters",
                        starters,
                        move |ix, window, cx| {
                            let task = chosen[ix].clone();
                            pick.update(cx, |a, cx| a.start_run(Some(task), window, cx))
                        },
                    )),
            ),
        )
}
