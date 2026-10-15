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
    layout::ScrollArea,
    overlays::Dialog,
    typography::SelectableText,
};
use gpui::{Entity, IntoElement, ParentElement, Styled, div, point, px};
use kai_core::ApprovalChoice;
use std::cell::Cell;
use std::rc::Rc;

pub fn dialog(p: &Pending, app: Entity<KaiApp>, run: &RunState) -> impl IntoElement {
    let what = describe(&p.decision, p.element.as_deref());
    let chosen = Rc::new(Cell::new(ApprovalChoice::Stop));
    let (on_close, skip, approve) = (chosen.clone(), chosen.clone(), chosen);
    // Approve the exact action, including suffixes beyond the compact title.
    let json = approval_json(&p.decision);

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
        "{}{reason}{why}Nothing happens until you choose. Escape stops the run.",
        p.explain
            .as_deref()
            .map(|e| format!("{e} "))
            .unwrap_or_default()
    ))
    .children(frame)
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

fn approval_json(decision: &kai_core::Decision) -> String {
    serde_json::to_string_pretty(decision).expect("Decision serializes without fallible values")
}

#[cfg(test)]
mod tests {
    use super::approval_json;

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
                serde_json::from_str(&approval_json(&decision)).unwrap();
            assert_eq!(displayed["text"].as_str(), Some(command.as_str()));
        }
    }
}
