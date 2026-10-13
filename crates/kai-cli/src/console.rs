//! Headless runs in a terminal: print each step as it happens, ask on stdin
//! before a risky action, and ask the person at a CAPTCHA, one-time code,
//! sign-in approval or question (`docs/HUMAN_CHECKS.md`).

use async_trait::async_trait;
use kai_core::engine::{ApprovalChoice, Approver, EngineEvent, EventSink};
use kai_core::human::{CheckKind, HumanCheck, HumanHelper, HumanReply, OneTimeCode};
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
            EngineEvent::HumanDone { result, .. } => {
                eprintln!("     ↩ back from you: {}", result.replace('_', " "));
            }
            EngineEvent::Outcomes { outcomes, .. } => {
                for o in outcomes {
                    eprintln!("     {}", checklist_line(o));
                }
            }
            EngineEvent::Gated { reason, .. } => eprintln!("     ⛔ {reason}"),
            EngineEvent::NeedsHuman { .. }
            | EngineEvent::Output { .. }
            | EngineEvent::Steered { .. } => {}
        }
    }
}

/// `✓ sent — the form was submitted [window_title(thank you)]`.
pub fn checklist_line(o: &kai_core::OutcomeRecord) -> String {
    use kai_core::ledger::{Status, VerifiedBy};
    let mark = match (o.status, o.by) {
        (Status::Verified, _) => "✓",
        (Status::Reverted, _) => "↺",
        (Status::Pending, Some(VerifiedBy::User)) => "☑ (you)",
        (Status::Pending, _) => "☐",
    };
    let probe = match (&o.probe, &o.probe_refused) {
        (Some(p), _) => format!(" [{p}]"),
        (None, Some(why)) => format!(" [probe refused: {why}]"),
        (None, None) => " [no probe]".into(),
    };
    format!("{mark} {} — {}{probe}", o.id, o.description)
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
        let explain = kai_core::risk::explain(d, g);
        tokio::task::spawn_blocking(move || {
            if let Some(e) = explain {
                eprintln!("\n  ⚠ {e}");
            }
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

/// Asks the person at the terminal (bell + desktop notification), then reads
/// one line: Enter = "I did it", `stop` = stop the run, a code (when Kai can
/// type it) or an answer (for a question).
pub struct TerminalHuman;

#[async_trait]
impl HumanHelper for TerminalHuman {
    async fn ask(&self, check: &HumanCheck) -> HumanReply {
        kai_os::notify::desktop(check.kind.title(), &check.prompt).await;
        let check = check.clone();
        tokio::task::spawn_blocking(move || {
            eprintln!("\x07\n  🙋 {}\n     {}", check.kind.title(), check.prompt);
            eprintln!("     (seen: {})", check.evidence);
            let hint = if check.accepts_code() {
                "Type the code for Kai to enter, or press Enter once you entered it; `stop` stops"
            } else if check.kind == CheckKind::Suspicious {
                "Press Enter to ignore it and continue; `stop` stops"
            } else if check.kind == CheckKind::Question {
                "Type your answer (Enter alone: you handled it); `stop` stops"
            } else {
                "Press Enter once you're done; `stop` stops"
            };
            for _ in 0..3 {
                eprint!("     {hint}: ");
                let _ = std::io::stderr().flush();
                let mut line = String::new();
                match std::io::stdin().lock().read_line(&mut line) {
                    Ok(0) | Err(_) => return HumanReply::Stop,
                    Ok(_) => {}
                }
                if let Some(reply) = parse_reply(&check, &line) {
                    return reply;
                }
                eprintln!("     That doesn't look like a code (4–12 letters or digits).");
            }
            HumanReply::Stop
        })
        .await
        .unwrap_or(HumanReply::Stop)
    }
}

/// One terminal line as an answer; `None` when a code was expected and the
/// line is neither a code nor empty.
fn parse_reply(check: &HumanCheck, line: &str) -> Option<HumanReply> {
    let line = line.trim();
    if line.eq_ignore_ascii_case("stop") {
        return Some(HumanReply::Stop);
    }
    if line.is_empty() || line.eq_ignore_ascii_case("done") {
        return Some(HumanReply::Resolved);
    }
    if check.accepts_code() {
        return OneTimeCode::parse(line).map(HumanReply::Code);
    }
    if check.accepts_answer() {
        return Some(HumanReply::Answer(line.to_string()));
    }
    Some(HumanReply::Resolved)
}

/// Terminal setup, first thing in `main`: warnings-level logging (to
/// stderr), and on Unix the default SIGPIPE, so `kai models list | head`
/// ends quietly when the reader goes away instead of panicking.
pub fn init() {
    // SAFETY: SIG_DFL is a valid disposition for SIGPIPE; no handler runs.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
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

    #[test]
    fn terminal_replies() {
        use kai_core::{Element, Rect, Role};
        let code_screen = Grounding::Marks(vec![Element {
            mark: 1,
            role: Role::TextField,
            name: "Verification code".into(),
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }]);
        let code = kai_core::human::detect(&code_screen).unwrap();
        assert!(matches!(
            parse_reply(&code, " 123 456\n"),
            Some(HumanReply::Code(_))
        ));
        assert_eq!(parse_reply(&code, "\n"), Some(HumanReply::Resolved));
        assert_eq!(parse_reply(&code, "STOP"), Some(HumanReply::Stop));
        assert_eq!(parse_reply(&code, "not a code!"), None);
        let q = HumanCheck::question("Which account?");
        assert_eq!(
            parse_reply(&q, "the work one\n"),
            Some(HumanReply::Answer("the work one".into()))
        );
    }
}
