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
    let json = serde_json::to_string_pretty(&p.decision).unwrap_or_default();

    let frame = run.frames.last().map(|f| {
        let mut v = ComputerUseViewer::new("approval-frame", f.to_string_lossy().to_string()).action(what.clone());
        if let Some(s) = run.screen {
            v = v.ratio(s.w / s.h.max(1.0));
        }
        if let Some((x, y)) = run.pointer {
            v = v.pointer(point(x, y));
        }
        v
    });

    Dialog::new("approval", format!("Kai wants to: {what}"), move |_, cx| {
        let choice = on_close.get();
        app.update(cx, |a, cx| a.answer(choice, cx));
    })
    .detail(format!(
        "This step is high risk ({}). Nothing happens until you choose. Escape stops the run.",
        p.decision.thought
    ))
    .children(frame)
    .child(div().w_full().child(CodeBlock::new("approval-json", json).language("json")))
    .action(|close| {
        Button::new("approval-stop", "Stop").variant(ButtonVariant::Danger).on_click(move |_, window, cx| close(window, cx))
    })
    .action(move |close| {
        Button::new("approval-skip", "Skip this step").variant(ButtonVariant::Secondary).on_click(move |_, window, cx| {
            skip.set(ApprovalChoice::Skip);
            close(window, cx)
        })
    })
    .action(move |close| {
        Button::new("approval-ok", "Approve").variant(ButtonVariant::Primary).on_click(move |_, window, cx| {
            approve.set(ApprovalChoice::Approve);
            close(window, cx)
        })
    })
}
