//! Headless runs in a terminal: print each step as it happens, and ask on
//! stdin before a risky action.

use async_trait::async_trait;
use kai_core::engine::{ApprovalChoice, Approver, EngineEvent, EventSink};
use kai_core::{Decision, Grounding};
use std::io::{BufRead, Write};

/// Prints the live step list.
pub struct Printer;

impl EventSink for Printer {
    fn emit(&self, ev: &EngineEvent) {
        match ev {
            EngineEvent::Thinking { step } => eprint!("  {:>2} · thinking…", step + 1),
            EngineEvent::Decided { decision, .. } => {
                eprintln!("\r     {} — {}", describe(decision), decision.thought);
            }
            EngineEvent::AwaitingApproval { .. } => {}
            EngineEvent::Acted { timings, .. } => eprintln!(
                "       capture {} · tree {} · encode {} · model {} · act {} ms",
                timings.capture_ms,
                timings.tree_ms,
                timings.encode_ms,
                timings.model_ms,
                timings.act_ms
            ),
            EngineEvent::Finished { outcome } => eprintln!("  ⏹ {outcome:?}"),
            EngineEvent::Observed { .. } => {}
        }
    }
}

/// One line saying what an action does, in plain words.
pub fn describe(d: &Decision) -> String {
    use kai_core::Action::*;
    let target = d.target.map(|t| format!(" #{t}")).unwrap_or_default();
    let text = d.text.as_deref().unwrap_or("");
    match d.action {
        Click => format!("click{target}"),
        DoubleClick => format!("double-click{target}"),
        RightClick => format!("right-click{target}"),
        Type => format!("type \"{}\"{target}", short(text)),
        Key => format!("press {}", d.keys.join("+")),
        Scroll => format!("scroll {:?}", d.direction.unwrap_or_default()).to_lowercase(),
        OpenApp => format!("open app {text}"),
        OpenUrl => format!("open {text}"),
        Wait => "wait".into(),
        AskUser => format!("ask you: {}", short(text)),
        Done => "done".into(),
        Shell => format!("run `{}`", short(text)),
    }
}

fn short(s: &str) -> String {
    let t: String = s.chars().take(70).collect();
    if s.chars().count() > 70 {
        format!("{t}…")
    } else {
        t
    }
}

/// Asks on the terminal. `auto_yes` approves everything except passwords and
/// payments, which always ask.
pub struct TerminalApprover {
    pub auto_yes: bool,
}

#[async_trait]
impl Approver for TerminalApprover {
    async fn approve(&self, d: &Decision, g: &Grounding) -> ApprovalChoice {
        if self.auto_yes && !kai_core::risk::is_payment_or_password(d, g) {
            eprintln!("     ✔ auto-approved: {}", describe(d));
            return ApprovalChoice::Approve;
        }
        let what = describe(d);
        let on = d
            .target
            .and_then(|t| g.element(t))
            .map(|e| format!(" on \"{}\"", e.name))
            .unwrap_or_default();
        tokio::task::spawn_blocking(move || {
            eprint!("\n  ⚠ Kai wants to {what}{on}. Approve? [y]es / [s]kip / [n]o, stop: ");
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            if std::io::stdin().lock().read_line(&mut line).is_err() {
                return ApprovalChoice::Stop;
            }
            match line.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" => ApprovalChoice::Approve,
                "s" | "skip" => ApprovalChoice::Skip,
                _ => ApprovalChoice::Stop,
            }
        })
        .await
        .unwrap_or(ApprovalChoice::Stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_actions_plainly() {
        let d = Decision::parse_checked(
            r#"{"thought":"","action":"key","keys":["ctrl","s"],"risk":"none"}"#,
        )
        .unwrap();
        assert_eq!(describe(&d), "press ctrl+s");
        let d = Decision::parse_checked(
            r#"{"thought":"","action":"shell","text":"df -h","risk":"low"}"#,
        )
        .unwrap();
        assert_eq!(describe(&d), "run `df -h`");
    }
}
