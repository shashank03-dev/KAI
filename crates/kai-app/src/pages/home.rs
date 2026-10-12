//! Idle: Kai's mark, a greeting, the composer as the one raised glass layer,
//! the keys that matter, and starters drawn from installed skills as cards.

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
        .map(|k| {
            let mut cap = k.to_string();
            if let Some(first) = cap.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            ui::kbd(cap, cx).into_any_element()
        })
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
        Conn::Unknown => (g.faint, false, "connecting"),
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
        .child(ui::dot(color, live, g))
        .child(ui::eyebrow(
            format!("{word} · {}", crate::runs::model_label(&app.settings)),
            cx,
        ))
        .into_any_element()
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let hour = jiff::Zoned::now().hour() as u8;
    let g = Glass::of(cx);

    let starters: Vec<(&str, &str, IconName)> = STARTERS
        .iter()
        .filter(|(skill, ..)| app.skills.iter().any(|s| s.front.name == *skill))
        .copied()
        .collect();

    let send = cx.entity();
    let model = crate::switcher::switcher("composer-model", app, cx);
    let team_pick = cx.entity();
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
    let no_hands = (!cx.global::<crate::AgentHandle>().0.can_act())
        .then(|| Banner::new("no-hands", Severity::Danger, "Kai can't control the mouse and keyboard on this desktop. On Linux, log in with an Xorg session."));
    let busy = app.active.map(|_| {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(ui::dot(g.ember, true, &g))
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
        hint(keys("ctrl-k", cx), "commands", &g),
    ];
    if let Some(stop) = crate::StopHotkey::label(cx) {
        hints.push(hint(keys(&stop, cx), "stops Kai anywhere", &g));
    }

    let cards = starters
        .into_iter()
        .enumerate()
        .map(|(ix, (skill, task, icon))| {
            let pick = cx.entity();
            let (hover, strong) = (g.card_hover, g.line_strong);
            div()
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
                    pick.update(cx, |a, cx| a.start_run(Some(task.to_string()), window, cx))
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
                .child(div().text_color(g.faint).child("→"))
        });

    let column = div()
        .w_full()
        .max_w(px(760.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(22.))
        .child(ui::orb(60., &g))
        .child(status_line(app, &g, cx))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(40.))
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
        )
        .child(div().h(px(4.)))
        .child(composer)
        .child(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(div().flex().flex_wrap().gap_4().children(hints))
                .children(busy),
        )
        .child(div().h(px(18.)))
        .child(
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
                ),
        );

    div()
        .size_full()
        .relative()
        .flex()
        .flex_col()
        .child(ui::aurora(360., &g))
        .children(offline)
        .children(no_hands)
        .child(
            div().flex_1().min_h_0().child(
                ScrollArea::new("home").vertical().child(
                    div()
                        .w_full()
                        .flex()
                        .justify_center()
                        .px(px(32.))
                        .pt(px(64.))
                        .pb(px(56.))
                        .child(column),
                ),
            ),
        )
}
