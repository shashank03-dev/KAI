//! The pause for a human check: a CAPTCHA, a one-time code, a sign-in
//! approval or a question (`docs/HUMAN_CHECKS.md`). Kai is stopped until
//! the user answers. For a code or a question the dialog has a field: a code
//! is typed into the page by Kai (and never shown to the model); an answer
//! goes to the model as a note. Built on Ely's `overlays::Dialog` like the
//! approval dialog; Escape, the scrim and Stop all stop the run.

use crate::agent::HumanPending;
use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    chat::PromptInput,
    feedback::Toast,
    forms::TextInput,
    overlays::Dialog,
    primitives::Severity,
};
use gpui::{Context, Entity, IntoElement, ParentElement, Styled, div};
use kai_core::human::{CheckKind, HumanReply};
use std::cell::Cell;
use std::rc::Rc;

pub fn dialog(
    h: &HumanPending,
    app: Entity<KaiApp>,
    draft: &Entity<TextInput>,
) -> impl IntoElement {
    let check = &h.check;
    let resolved = Rc::new(Cell::new(false));
    let (on_close, done) = (resolved.clone(), resolved);
    let typed = check.accepts_code() || check.accepts_answer();
    let submit = app.clone();
    let input = typed.then(|| {
        div()
            .w_full()
            .child(PromptInput::new("human-input", draft, move |_, cx| {
                submit.update(cx, send)
            }))
    });
    let what = if check.accepts_code() {
        " Type the code below and press Enter: Kai enters it into the page for you \
         (the model never sees it)."
    } else if check.accepts_answer() {
        " Type your answer below and press Enter."
    } else {
        ""
    };
    let done_label = if check.kind == CheckKind::Question {
        "Carry on without answering"
    } else {
        "I did it"
    };

    Dialog::new("human-check", check.kind.title(), move |_, cx| {
        let reply = if on_close.get() {
            HumanReply::Resolved
        } else {
            HumanReply::Stop
        };
        app.update(cx, |a, cx| a.answer_human(reply, cx));
    })
    .detail(format!(
        "{}{what} Kai is paused until you answer (seen: {}). Escape stops the run.",
        check.prompt, check.evidence
    ))
    .children(input)
    .action(|close| {
        Button::new("human-stop", "Stop")
            .variant(ButtonVariant::Danger)
            .on_click(move |_, window, cx| close(window, cx))
    })
    .action(move |close| {
        Button::new("human-done", done_label)
            .variant(ButtonVariant::Primary)
            .on_click(move |_, window, cx| {
                done.set(true);
                close(window, cx)
            })
    })
}

/// Enter in the field: send the code or the answer. A code that isn't one
/// (4–12 letters or digits) is not sent; the dialog stays and says why.
fn send(a: &mut KaiApp, cx: &mut Context<KaiApp>) {
    if !a.send_human_text(cx) {
        a.toast(
            Toast::new("That doesn't look like a code")
                .body("Codes are 4–12 letters or digits. Kai only ever types codes.")
                .severity(Severity::Warning),
            cx,
        );
    }
}
