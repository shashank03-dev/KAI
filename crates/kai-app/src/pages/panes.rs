//! Parts of a run's view shared by the live and the finished run: the
//! transcript (steps with the user's steering messages between them), the
//! terminal log, the sub-agent tree and the per-run latency meter.

use crate::state::{RunState, StepView};
use crate::team::{SubState, SubView};
use ely_gpui_component::{
    agent::ToolCallCard,
    chat::{MessageBubble, MessageFooter, Role},
    data_display::{Badge, Sparkline, Tone},
    lists::{Tree, TreeNode},
    primitives::IconName,
    terminal::AnsiText,
    theme::{ActiveTheme, Radius, TextSize},
};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, div, px};
use kai_core::Outcome;
use kai_core::orchestrator::Kind;
use std::time::Duration;

pub fn card(ix: usize, s: &StepView) -> impl IntoElement {
    let mut c =
        ToolCallCard::new(("step", ix), s.title.clone(), s.state).summary(s.thought.clone());
    if let Some(tm) = s.timings {
        c = c.took(Duration::from_millis(tm.total_ms())).result(format!(
            "capture {} ms · tree {} ms · encode {} ms · model {} ms · act {} ms",
            tm.capture_ms, tm.tree_ms, tm.encode_ms, tm.model_ms, tm.act_ms
        ));
    }
    if !s.json.is_empty() {
        c = c.arguments(s.json.clone());
    }
    c
}

fn steer_bubble(text: &str, read_at: Option<u32>) -> AnyElement {
    let fact = match read_at {
        Some(step) => format!("Kai read this at step {}", step + 1),
        None => "Waiting for Kai's next step".to_string(),
    };
    div()
        .child(
            MessageBubble::new(Role::User)
                .footer(MessageFooter::new().fact(fact))
                .child(text.to_string()),
        )
        .into_any_element()
}

/// Steps in order, with each steering message placed where it was sent.
pub fn transcript(r: &RunState) -> Vec<AnyElement> {
    let mut out = Vec::new();
    let mut steers = r.steers.iter().peekable();
    for (i, s) in r.steps.iter().enumerate() {
        while let Some(m) = steers.next_if(|m| m.sent_at as usize <= i) {
            out.push(steer_bubble(&m.text, m.read_at));
        }
        out.push(card(i, s).into_any_element());
    }
    for m in steers {
        out.push(steer_bubble(&m.text, m.read_at));
    }
    out
}

/// "[exit 3]" at the end of Kai's shell output.
pub fn exit_code(output: &str) -> Option<i32> {
    let tail = output.trim_end().rsplit('\n').next()?;
    tail.strip_prefix("[exit ")?.strip_suffix(']')?.parse().ok()
}

/// Every command Kai ran in this run and what it printed.
pub fn terminal(r: &RunState, cx: &App) -> AnyElement {
    let theme = cx.theme();
    if r.outputs.is_empty() {
        return div()
            .p_4()
            .text_color(theme.colors.fg_muted)
            .child("No terminal commands in this run yet. Commands Kai runs, and what they print, appear here.")
            .into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .gap_2()
        .children(r.outputs.iter().map(|o| {
            let (label, tone) = match exit_code(&o.output) {
                Some(0) => ("exit 0".to_string(), Tone::Success),
                Some(c) => (format!("exit {c}"), Tone::Danger),
                None if o.output.contains("[stopped by the kill switch]") => {
                    ("stopped".to_string(), Tone::Warning)
                }
                None => ("ended".to_string(), Tone::Neutral),
            };
            let act = r
                .steps
                .iter()
                .find(|s| s.index == o.step)
                .and_then(|s| s.timings)
                .map(|t| format!("{} ms", t.act_ms))
                .unwrap_or_default();
            div()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .rounded(theme.radius(Radius::Md))
                .border_1()
                .border_color(theme.colors.border)
                .bg(theme.colors.surface)
                .text_size(theme.text_size(TextSize::Sm))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_color(theme.colors.fg_muted)
                                .child(format!("step {}", o.step + 1)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_family(theme.mono_family.clone())
                                .child(format!("$ {}", o.command)),
                        )
                        .child(div().text_color(theme.colors.fg_muted).child(act))
                        .child(Badge::new(label).tone(tone)),
                )
                .child(
                    div()
                        .font_family(theme.mono_family.clone())
                        .child(AnsiText::new(o.output.clone())),
                )
        }))
        .into_any_element()
}

fn sub_node(s: &SubView) -> TreeNode {
    let (note, tone) = match s.state {
        SubState::Waiting => ("waiting".to_string(), Tone::Neutral),
        SubState::Running => ("running".to_string(), Tone::Info),
        SubState::Finished => ("finished".to_string(), Tone::Neutral),
        SubState::Ended(Outcome::Success) => ("done".to_string(), Tone::Success),
        SubState::Ended(Outcome::Stopped) => ("stopped".to_string(), Tone::Warning),
        SubState::Ended(o) => (format!("{o:?}").to_lowercase(), Tone::Danger),
    };
    let note = match s.steps {
        Some(n) => format!("{note} · {n} steps"),
        None => note,
    };
    let (icon, kind) = match s.kind {
        Kind::Gui => (IconName::MousePointer2, "screen (takes turns)"),
        Kind::Shell => (IconName::Terminal, "terminal (parallel)"),
    };
    TreeNode::new(format!("sub-{}", s.id), format!("{} · {}", s.id, s.goal))
        .icon(icon)
        .note(note)
        .tone(tone)
        .child(TreeNode::new(format!("sub-{}-kind", s.id), kind))
        .child(TreeNode::new(
            format!("sub-{}-wave", s.id),
            format!("wave {}", s.wave + 1),
        ))
}

/// The team's sub-agents as a tree, with a live status per sub-agent.
pub fn team_tree(r: &RunState, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let subs = &r.team_view.subs;
    if subs.is_empty() {
        let text = if r.team {
            "Kai is planning the sub-agents…"
        } else {
            "This run has no sub-agents. Turn on Team in the composer to split a task into sub-agents."
        };
        return div()
            .p_4()
            .text_color(theme.colors.fg_muted)
            .child(text)
            .into_any_element();
    }
    let root = TreeNode::new("team", format!("{} sub-agents", subs.len()))
        .icon(IconName::Users)
        .children(subs.iter().map(sub_node));
    let open: Vec<String> = std::iter::once("team".to_string())
        .chain(subs.iter().map(|s| format!("sub-{}", s.id)))
        .collect();
    let rows = 1 + subs.len() * 3;
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .h(px(32. * rows as f32 + 8.))
                .child(Tree::new("team-tree", [root]).open(open)),
        )
        .child(
            div()
                .text_size(theme.text_size(TextSize::Xs))
                .text_color(theme.colors.fg_muted)
                .child("Live status follows the plan's waves; exact outcomes arrive with the team's report."),
        )
        .into_any_element()
}

/// Model time per step: a sparkline with the last and median values.
pub fn latency(r: &RunState, cx: &App) -> Option<AnyElement> {
    let theme = cx.theme();
    let ms = &r.model_ms;
    if ms.is_empty() {
        return None;
    }
    let mut sorted = ms.clone();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    let last = *ms.last()?;
    let spark = (ms.len() >= 2).then(|| {
        Sparkline::new(ms.iter().map(|v| *v as f32))
            .bars()
            .w(px(160.))
            .h(px(28.))
    });
    Some(
        div()
            .flex()
            .items_center()
            .gap_3()
            .children(spark)
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Sm))
                    .child(format!(
                        "model time · last {last} ms · median {median} ms · {} calls",
                        ms.len()
                    )),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_exit_code() {
        assert_eq!(exit_code("hello\n[exit 0]"), Some(0));
        assert_eq!(exit_code("oops\n[exit 3]\n"), Some(3));
        assert_eq!(exit_code("[stopped by the kill switch]"), None);
        assert_eq!(exit_code("x\n[exit signal]"), None);
    }
}
