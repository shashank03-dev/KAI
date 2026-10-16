//! Runs and steps in plain words, for the transcript, lists, toasts and the
//! approval dialog.

use kai_core::trace::Outcome;
use kai_core::{Action, Decision};

/// A run's outcome in a few plain words, for lists, toasts and badges.
pub fn outcome_text(o: Outcome) -> &'static str {
    match o {
        Outcome::Success => "Done",
        Outcome::Failed => "Couldn't finish",
        Outcome::Stopped => "Stopped",
        Outcome::StepLimit => "Hit the step limit",
        Outcome::TimeLimit => "Hit the time limit",
        Outcome::AskedUser => "Needs your answer",
    }
}

/// A decision as the exact JSON shown for review. Serializing a `Decision`
/// does not fail today; if it ever did, the debug form still shows every
/// field, so neither the transcript nor an approval shows less than the step.
pub fn decision_json(d: &Decision) -> String {
    serde_json::to_string_pretty(d).unwrap_or_else(|e| {
        log::error!("decision not serialised: {e}");
        format!("{d:#?}")
    })
}

/// An action in plain words: "Clicking “Save”", "Typing “hello”".
pub fn describe(d: &Decision, element: Option<&str>) -> String {
    let on = element
        .map(|e| format!(" “{e}”"))
        .unwrap_or_else(|| d.target.map(|t| format!(" #{t}")).unwrap_or_default());
    let text = d.text.as_deref().unwrap_or("");
    let short: String = text.chars().take(48).collect();
    match d.action {
        Action::Computer => "Using computer tool".into(),
        Action::Click => format!("Clicking{on}"),
        Action::DoubleClick => format!("Double-clicking{on}"),
        Action::RightClick => format!("Right-clicking{on}"),
        Action::Type => format!("Typing “{short}”"),
        Action::Key => format!("Pressing {}", d.keys.join("+")),
        Action::Scroll => "Scrolling".into(),
        Action::OpenApp => format!("Opening {short}"),
        Action::OpenUrl => format!("Opening {short}"),
        Action::Wait => "Waiting for the screen".into(),
        Action::AskUser => format!("Asking you: {short}"),
        Action::Done => "Finished".into(),
        Action::Shell => format!("Running `{short}`"),
    }
}

/// " 2 more wait after this one." for a dialog's detail, or nothing.
pub fn more_waiting(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => " 1 more waits after this one.".into(),
        n => format!(" {n} more wait after this one."),
    }
}

#[cfg(test)]
mod tests {
    use super::more_waiting;

    #[test]
    fn says_how_many_more_decisions_wait() {
        assert_eq!(more_waiting(0), "");
        assert_eq!(more_waiting(1), " 1 more waits after this one.");
        assert_eq!(more_waiting(3), " 3 more wait after this one.");
    }
}
