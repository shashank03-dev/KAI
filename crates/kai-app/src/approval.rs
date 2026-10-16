//! Before a risky step: what Kai sees, where it points and exactly what it will
//! do; then Stop, Skip or Approve. Built on Ely's `overlays::Dialog`, which
//! `ToolApprovalDialog` also builds on, because the run needs a third answer
//! (Skip) that `ToolApprovalDialog` does not have. Escape, the scrim and Stop
//! all stop the run.

use crate::agent::Pending;
use crate::app::KaiApp;
use crate::state::RunState;
use crate::words::{decision_json, describe, more_waiting};
use ely_gpui_component::{
    agent::ComputerUseViewer,
    buttons::{Button, ButtonVariant},
    layout::ScrollArea,
    overlays::Dialog,
    typography::SelectableText,
};
use gpui::{Entity, IntoElement, ParentElement, Styled, div, point, px};
use kai_core::ApprovalChoice;
use std::cell::Cell;
use std::rc::Rc;

/// `more`: other decisions waiting after this one.
pub fn dialog(p: &Pending, app: Entity<KaiApp>, run: &RunState, more: usize) -> impl IntoElement {
    let what = describe(&p.decision, p.element.as_deref());
    let ready = run.approve_ready();
    let chosen = Rc::new(Cell::new(ApprovalChoice::Stop));
    let (on_close, skip, approve) = (chosen.clone(), chosen.clone(), chosen);
    // Approve the exact action, including suffixes beyond the compact title.
    let json = decision_json(&p.decision);

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
        move |window, cx| {
            let choice = on_close.get();
            // An approved step acts at once: the main window steps aside
            // first, so it never covers what Kai clicks (review APP-SH-03).
            if choice != ApprovalChoice::Stop && app.read(cx).capture.is_none() {
                window.minimize_window();
            }
            app.update(cx, |a, cx| a.answer(choice, cx));
        },
    )
    .detail(format!(
        "{}{reason}{why}Nothing happens until you choose. Escape stops the run.{}",
        p.explain
            .as_deref()
            .map(|e| format!("{e} "))
            .unwrap_or_default(),
        more_waiting(more)
    ))
    // The exact action first, in view without scrolling at the smallest
    // window; then a small picture of where it lands.
    .child(
        ScrollArea::new("approval-action")
            .h(px(160.))
            .vertical()
            .child(
                div()
                    .p_2()
                    .child(SelectableText::new("approval-json", json)),
            ),
    )
    .children(frame.map(|f| div().w_full().max_w(px(360.)).child(f)))
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
        // Off for a moment after the card appears, so a click meant for
        // something else cannot approve it (`APPROVE_GRACE`).
        Button::new("approval-ok", "Approve")
            .variant(ButtonVariant::Primary)
            .disabled(!ready)
            .on_click(move |_, window, cx| {
                approve.set(ApprovalChoice::Approve);
                close(window, cx)
            })
    })
}

#[cfg(test)]
mod tests {
    use crate::words::decision_json;

    #[test]
    fn approval_preserves_commands_that_differ_after_the_summary() {
        for suffix in ["keep", "delete"] {
            let command = format!("echo '{}'\n{suffix}", "界".repeat(60));
            let decision = kai_core::Decision::parse_checked(
                &serde_json::json!({
                    "thought":"run", "action":"shell", "text":command, "risk":"high"
                })
                .to_string(),
            )
            .unwrap();
            let displayed: serde_json::Value =
                serde_json::from_str(&decision_json(&decision)).unwrap();
            assert_eq!(displayed["text"].as_str(), Some(command.as_str()));
        }
    }
}
