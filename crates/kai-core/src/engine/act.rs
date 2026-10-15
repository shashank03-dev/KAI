//! Acting: accessibility first, then the mouse/keyboard; OS fast paths; and
//! the no-model prefixes (skill steps, cached plans).

use super::{CACHED, Engine, Pending, Run};
use crate::element::Grounding;
use crate::ports::EngineEvent;
use crate::prompt::HistoryItem;
use crate::risk;
use crate::schema::{Action, Decision};
use crate::skill::DeterministicStep;
use crate::trace::{Outcome, StepRecord, Timings};
use std::time::{Duration, Instant};

/// Refuse to open a URL this run already opened: a reload wipes what was
/// typed into the page. Returns the note the model sees instead.
pub(super) fn reopen_guard(opened: &mut Vec<String>, d: &Decision) -> Option<String> {
    if d.action != Action::OpenUrl {
        return None;
    }
    let url = d
        .text
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_end_matches('/')
        .to_lowercase();
    if url.is_empty() {
        return None;
    }
    if opened.contains(&url) {
        return Some(format!(
            "Not reopened: {url} is already open. Reopening would clear what you typed; \
             carry on in the page that is open."
        ));
    }
    opened.push(url);
    None
}

/// What [`repeat_guard`] answers instead of running a command again.
const NOT_AGAIN: &str = "Not run again:";

/// Refuse a shell command that already ran twice in a row with the same
/// result: a third run will not change it (seen live: Gemma wrote the same
/// file 40 times). Returns the note the model sees instead.
pub(super) fn repeat_guard(history: &[HistoryItem], d: &Decision) -> Option<String> {
    if d.action != Action::Shell {
        return None;
    }
    let this = HistoryItem::from_decision(d);
    let same: Vec<&HistoryItem> = history
        .iter()
        .rev()
        .take_while(|h| h.same_step(&this))
        .collect();
    let looping =
        same.iter().any(|h| refused(h)) || (same.len() >= 2 && same[0].output == same[1].output);
    looping.then(|| {
        format!(
            "{NOT_AGAIN} this exact command already ran twice in a row with the same result \
             (see your last steps). If the task is complete, reply done; otherwise do \
             something different."
        )
    })
}

/// A step [`repeat_guard`] did not run.
fn refused(h: &HistoryItem) -> bool {
    h.output
        .as_deref()
        .is_some_and(|o| o.starts_with(NOT_AGAIN))
}

/// The last shell step repeats the one before it with the same result, or
/// was refused as a repeat: the run is looping, not progressing.
pub(super) fn looping(history: &[HistoryItem]) -> bool {
    match history {
        [.., b] if refused(b) => true,
        [.., a, b] => a.same_step(b) && a.output == b.output,
        _ => false,
    }
}

/// The last four steps are two different steps done twice in turn (A B A
/// B) with the same results.
pub(super) fn cycling(history: &[HistoryItem]) -> bool {
    match history {
        [.., a, b, c, d] => {
            a.same_step(c)
                && b.same_step(d)
                && !a.same_step(b)
                && a.output == c.output
                && b.output == d.output
        }
        _ => false,
    }
}

impl Engine<'_> {
    /// Perform one action. Returns command output for `shell` (so the model
    /// sees it next step) and the path taken: `a11y`, `mouse`, `keyboard`,
    /// `os`, `shell` or `wait`.
    pub(super) async fn act(
        &self,
        decision: &Decision,
        grounding: &Grounding,
        privacy: &super::privacy::Privacy,
        observed: Option<&crate::Observation>,
    ) -> anyhow::Result<(Option<String>, &'static str)> {
        let ctx = crate::computer::ExecutionContext::new(Duration::from_secs(180));
        let _guard = crate::computer::ExecutionGuard(ctx.clone());
        let operation = async {
            let _owner = if decision.action == Action::Shell {
                None
            } else {
                Some(crate::computer::mutation_owner().await)
            };
            // Approvals and queued ownership waits can outlive the screen.
            if !matches!(
                decision.action,
                Action::Shell | Action::OpenApp | Action::OpenUrl | Action::Wait
            ) {
                let grid = matches!(grounding, Grounding::Grid { .. });
                anyhow::ensure!(
                    !grid || observed.is_some(),
                    "grid input needs a current captured observation"
                );
                let mut fresh = if grid || !self.config.tree_only {
                    self.perceptor.observe().await?
                } else {
                    self.perceptor.observe_tree_only().await?
                };
                privacy.sanitise(&mut fresh);
                let mut expected = grounding.clone();
                privacy.redact(&mut expected);
                anyhow::ensure!(
                    fresh.grounding == expected,
                    "the approved target changed; observe again"
                );
                if let Some(old) = observed {
                    anyhow::ensure!(
                        fresh.screen == old.screen
                            && fresh.window_title
                                == old.window_title.as_deref().map(|s| privacy.text(s))
                            && fresh.url == old.url.as_deref().map(|s| privacy.text(s))
                            && (!grid || fresh.screen_hash == old.screen_hash),
                        "the approved foreground context changed; observe again"
                    );
                }
            }
            self.act_inner(decision, grounding, &ctx).await
        };
        tokio::select! {
            biased;
            ()=crate::ports::wait_cancelled(self.interrupt)=>anyhow::bail!("run cancelled"),
            result=operation=>result,
        }
    }

    async fn act_inner(
        &self,
        decision: &Decision,
        grounding: &Grounding,
        ctx: &crate::computer::ExecutionContext,
    ) -> anyhow::Result<(Option<String>, &'static str)> {
        anyhow::ensure!(!self.interrupt.cancelled(), "run cancelled");
        // Fast paths reach the OS directly, no grounding needed.
        match decision.action {
            Action::OpenApp => self
                .actuator
                .act_context(decision, grounding, ctx)
                .await
                .map(|o| (o, "os")),
            Action::OpenUrl => self
                .actuator
                .act_context(decision, grounding, ctx)
                .await
                .map(|o| (o, "os")),
            Action::Shell => self
                .actuator
                .act_context(decision, grounding, ctx)
                .await
                .map(|o| (o, "shell")),
            Action::Wait => {
                self.perceptor.settle(Duration::from_secs(2)).await;
                Ok((None, "wait"))
            }
            _ => {
                // Accessibility first: invoke/set-value/toggle with no mouse.
                if self.config.a11y_actions
                    && let Some(t) = decision.target
                    && grounding.element(t).is_some()
                {
                    match self.actuator.a11y_context(decision, grounding, ctx).await {
                        Ok(true) => return Ok((None, "a11y")),
                        Ok(false) => {}
                        Err(_) => {
                            anyhow::bail!("accessibility action failed; its effect is unknown")
                        }
                    }
                }
                let path = if decision.target.is_some() || decision.action == Action::Scroll {
                    "mouse"
                } else {
                    "keyboard"
                };
                anyhow::ensure!(!self.interrupt.cancelled(), "run cancelled");
                self.actuator.act_context(decision, grounding, ctx).await?;
                // Typing into a dropdown picks the matching option; Enter
                // commits it (a click alone only toggles the list).
                if decision.action == Action::Type
                    && decision
                        .target
                        .and_then(|t| grounding.element(t))
                        .is_some_and(|e| e.role == crate::element::Role::ComboBox)
                {
                    let mut enter = decision.clone();
                    enter.action = Action::Key;
                    enter.target = None;
                    enter.text = None;
                    enter.keys = vec!["enter".to_string()];
                    anyhow::ensure!(!self.interrupt.cancelled(), "run cancelled");
                    self.actuator.act_context(&enter, grounding, ctx).await?;
                }
                Ok((None, path))
            }
        }
    }

    /// Run skill steps or a cached plan's prefix with no model call. Cached
    /// steps that would need approval end the prefix (the model takes over and
    /// proposes them through the normal approval path). Any failure hands over
    /// to the model. `Some` ends the run.
    pub(super) async fn run_prefix(
        &self,
        r: &mut Run<'_>,
        steps: &[Decision],
        label: &str,
    ) -> Option<Outcome> {
        let none = Grounding::Marks(Vec::new());
        let mut done = 0usize;
        for d in steps {
            if r.step >= self.config.max_steps {
                break;
            }
            if self.interrupt.stopped() {
                return Some(Outcome::Stopped);
            }
            // Untargeted typing lands wherever focus is: read the screen
            // first, so a focused password field, a CAPTCHA or suspicious
            // text hands over to the model instead of being typed into.
            let mut g = none.clone();
            if matches!(d.action, Action::Type | Action::Key | Action::Scroll) {
                match self.perceptor.observe_tree_only().await {
                    Ok(mut o) => {
                        r.redact(&mut o.grounding);
                        g = o.grounding;
                    }
                    Err(e) => {
                        log::warn!("{label} step: reading the screen failed: {e}; handing over");
                        break;
                    }
                }
                if risk::is_payment_or_password(d, &g)
                    || crate::human::guard(d, &g).is_some()
                    || self.unhandled_check(r, &g).is_some()
                {
                    log::info!("{label} step types into a secure field or a check; handing over");
                    break;
                }
            }
            if label == CACHED {
                let level = self.config.approval;
                let pw = risk::is_payment_or_password(d, &g);
                if risk::needs_approval(level, risk::classify(d, &g), pw) {
                    break;
                }
            }
            let t = Instant::now();
            r.protect(d, &g, false);
            let (output, path) = match self.act(d, &g, &r.privacy, None).await {
                Ok(x) => x,
                Err(e) => {
                    log::warn!("{label} step failed; handing over to the model");
                    let _ = e;
                    break;
                }
            };
            self.perceptor.settle(Duration::from_secs(3)).await;
            let output = output.map(|text| r.privacy.text(&text));
            let timings = Timings {
                act_ms: t.elapsed().as_millis() as u64,
                ..Default::default()
            };
            let step = r.step;
            self.sink.emit(&EngineEvent::Acted { step, timings });
            self.emit_output(step, d, output.as_deref());
            let mut rec = StepRecord::new(step, d, timings, label);
            rec.path = Some(path.to_string());
            rec.output = output.clone();
            r.push(rec);
            let mut item = HistoryItem::from_decision(d);
            if let Some(o) = &output {
                item.push_output(o);
            }
            r.history.push(item);
            r.step += 1;
            done += 1;
        }
        if done > 0 && label == CACHED {
            r.notes.push(format!(
                "{done} steps of a remembered plan for this task already ran (see your last \
                 steps). Check the screen, then continue."
            ));
        }
        // Verify the prefix with a full observation before the model goes on.
        r.force_full = true;
        r.pending = None::<Pending>;
        None
    }
}

/// The decision a deterministic skill step stands for.
pub(super) fn fixed_decision(st: &DeterministicStep) -> Decision {
    let (action, text, keys) = match st {
        DeterministicStep::OpenUrl { url } => (Action::OpenUrl, Some(url.clone()), vec![]),
        DeterministicStep::OpenApp { app } => (Action::OpenApp, Some(app.clone()), vec![]),
        DeterministicStep::Shortcut { keys } => (Action::Key, None, keys.clone()),
        DeterministicStep::Type { text } => (Action::Type, Some(text.clone()), vec![]),
    };
    let mut d = Decision {
        thought: "skill step".to_string(),
        action,
        target: None,
        text,
        keys,
        direction: None,
        amount: None,
        risk: crate::schema::Risk::None,
        expect: String::new(),
    };
    // Normalise keys exactly like a model reply; `type` without a target types
    // into whatever has focus.
    let _ = d.validate_and_clamp();
    d
}

/// Steering and terminal output, shared by every action path.
impl Engine<'_> {
    /// Fold mid-run user messages into the next turn.
    pub(super) fn drain_steering(&self, r: &mut Run<'_>, step: u32) -> bool {
        let msgs = self.steering.take();
        let any = !msgs.is_empty();
        for m in msgs {
            self.sink.emit(&EngineEvent::Steered {
                step,
                message: m.clone(),
            });
            r.notes.push(format!("User says (mid-run): {m}"));
            r.trace.steered.push(m);
        }
        if any {
            // The user may refer to what they see: look again properly.
            r.force_full = true;
        }
        any
    }

    pub(super) fn emit_output(&self, step: u32, d: &crate::schema::Decision, output: Option<&str>) {
        if d.action == Action::Shell
            && let Some(out) = output
        {
            self.sink.emit(&EngineEvent::Output {
                step,
                command: d.text.clone().unwrap_or_default(),
                output: out.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_opened_once_per_run() {
        let open = |u: &str| {
            Decision::parse_checked(&format!(
                r#"{{"thought":"x","action":"open_url","text":"{u}","risk":"none"}}"#
            ))
            .unwrap()
        };
        let mut seen = Vec::new();
        assert!(reopen_guard(&mut seen, &open("http://127.0.0.1:8765/contact.html")).is_none());
        assert!(reopen_guard(&mut seen, &open("http://127.0.0.1:8765/contact.html/")).is_some());
        assert!(reopen_guard(&mut seen, &open("https://example.com")).is_none());
    }

    #[test]
    fn a_third_identical_command_in_a_row_is_not_run() {
        let echo = Decision::parse_checked(
            r#"{"thought":"x","action":"shell","text":"echo 1 > f","risk":"low"}"#,
        )
        .unwrap();
        let ran = |out: &str| {
            let mut h = HistoryItem::from_decision(&echo);
            h.push_output(out);
            h
        };
        assert!(repeat_guard(&[], &echo).is_none());
        assert!(repeat_guard(&[ran("[exit 0]")], &echo).is_none());
        // Polling that changes its output is not a loop.
        assert!(repeat_guard(&[ran("10%"), ran("50%")], &echo).is_none());
        let twice = [ran("[exit 0]"), ran("[exit 0]")];
        let note = repeat_guard(&twice, &echo).unwrap();
        // Still refused after the refusal itself joins the history.
        let mut after = twice.to_vec();
        after.push(ran(&note));
        assert!(repeat_guard(&after, &echo).is_some());
        // Something else in between starts over.
        let mut other = twice.to_vec();
        other.push(HistoryItem::note("click #3"));
        assert!(repeat_guard(&other, &echo).is_none());
    }

    #[test]
    fn long_scripts_that_only_start_alike_are_not_a_repeat() {
        // The summary keeps a long script's first line and line count only.
        let script = |n: u32| {
            let body: String = (0..12)
                .map(|i| format!("echo step {i} of a long setup script for file {n}\n"))
                .collect();
            let json = serde_json::json!({"thought": "x", "action": "shell",
                "text": format!("set -e\n{body}"), "risk": "low"});
            Decision::parse_checked(&json.to_string()).unwrap()
        };
        let (a, b, c) = (script(1), script(2), script(3));
        assert_eq!(
            HistoryItem::from_decision(&a).summary,
            HistoryItem::from_decision(&b).summary
        );
        let ran = |d: &Decision| {
            let mut h = HistoryItem::from_decision(d);
            h.push_output("[exit 0]");
            h
        };
        let history = [ran(&a), ran(&b)];
        assert!(repeat_guard(&history, &c).is_none());
        assert!(!looping(&history));
        assert!(repeat_guard(&[ran(&c), ran(&c)], &c).is_some());
        assert!(looping(&[ran(&c), ran(&c)]));
    }
}
