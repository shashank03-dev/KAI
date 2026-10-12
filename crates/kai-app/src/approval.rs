//! Before a risky step: what Kai sees, where it points and exactly what it will
//! do; then Stop, Skip or Approve. Built on Ely's `overlays::Dialog`, which
//! `ToolApprovalDialog` also builds on, because the run needs a third answer
//! (Skip) that `ToolApprovalDialog` does not have. Escape, the scrim and Stop
//! all stop the run.

use crate::agent::Pending;
use crate::app::KaiApp;
use crate::state::{RunState, describe};
use ely_gpui_component::{
    agent::ComputerUseViewer,
    buttons::{Button, ButtonVariant},
    chat::CodeBlock,
    overlays::Dialog,
};
use gpui::{Entity, IntoElement, ParentElement, Styled, div, point};
use kai_core::ApprovalChoice;
use std::cell::Cell;
use std::rc::Rc;

pub fn dialog(p: &Pending, app: Entity<KaiApp>, run: &RunState) -> impl IntoElement {
    let what = describe(&p.decision, p.element.as_deref());
    let chosen = Rc::new(Cell::new(ApprovalChoice::Stop));
    let (on_close, skip, approve) = (chosen.clone(), chosen.clone(), chosen);
    // The dialog is narrow and the code block doesn't wrap: a long line (the
    // thought, typed text) ran off its edge. The full reason is in the
    // detail above, so long lines are shortened here.
    let json = fit_lines(
        &serde_json::to_string_pretty(&p.decision).unwrap_or_default(),
        JSON_LINE_CHARS,
    );

    let frame = run.frames.last().map(|f| {
        let mut v = ComputerUseViewer::new("approval-frame", f.to_string_lossy().to_string())
            .action(what.clone());
        if let Some(s) = run.screen {
            v = v.ratio(s.w / s.h.max(1.0));
        }
        if let Some((x, y)) = run.pointer {
            v = v.pointer(point(x, y));
        }
        v
    });

    // Asked for because of the approval level, the model's risk or a
    // payment/password rule: say only what is known.
    let why = if p.decision.risk == kai_core::schema::Risk::High {
        "Kai marked this step high risk. "
    } else {
        ""
    };
    let reason = p.decision.thought.trim().trim_end_matches('.');
    let reason = if reason.is_empty() {
        String::new()
    } else {
        format!("Kai's reason: {reason}. ")
    };
    Dialog::new(
        "approval",
        format!("Allow this step? {what}"),
        move |_, cx| {
            let choice = on_close.get();
            app.update(cx, |a, cx| a.answer(choice, cx));
        },
    )
    .detail(format!(
        "{reason}{why}Nothing happens until you choose. Escape stops the run."
    ))
    .children(frame)
    .child(
        div()
            .w_full()
            .child(CodeBlock::new("approval-json", json).language("json")),
    )
    .action(|close| {
        Button::new("approval-stop", "Stop")
            .variant(ButtonVariant::Danger)
            .on_click(move |_, window, cx| close(window, cx))
    })
    .action(move |close| {
        Button::new("approval-skip", "Skip this step")
            .variant(ButtonVariant::Secondary)
            .on_click(move |_, window, cx| {
                skip.set(ApprovalChoice::Skip);
                close(window, cx)
            })
    })
    .action(move |close| {
        Button::new("approval-ok", "Approve")
            .variant(ButtonVariant::Primary)
            .on_click(move |_, window, cx| {
                approve.set(ApprovalChoice::Approve);
                close(window, cx)
            })
    })
}

/// Longest JSON line the approval dialog shows in full.
const JSON_LINE_CHARS: usize = 48;

/// Cut every line longer than `max` characters to `max - 1` plus "…".
fn fit_lines(text: &str, max: usize) -> String {
    text.lines()
        .map(|l| {
            if l.chars().count() > max {
                let mut cut: String = l.chars().take(max.saturating_sub(1)).collect();
                cut.push('…');
                cut
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::fit_lines;

    #[test]
    fn long_json_lines_are_shortened_short_ones_kept() {
        let text = "{\n  \"thought\": \"Send the drafted email to the professor now\",\n  \"target\": 12\n}";
        let out = fit_lines(text, 20);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "{");
        assert_eq!(lines[1].chars().count(), 20);
        assert!(lines[1].ends_with('…'));
        assert_eq!(lines[2], "  \"target\": 12");
        assert_eq!(lines[3], "}");
    }
}
