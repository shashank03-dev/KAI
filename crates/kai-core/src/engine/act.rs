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

impl Engine<'_> {
    /// Perform one action. Returns command output for `shell` (so the model
    /// sees it next step) and the path taken: `a11y`, `mouse`, `keyboard`,
    /// `os`, `shell` or `wait`.
    pub(super) async fn act(
        &self,
        decision: &Decision,
        grounding: &Grounding,
    ) -> anyhow::Result<(Option<String>, &'static str)> {
        let text = decision.text.as_deref().unwrap_or("");
        // Fast paths reach the OS directly, no grounding needed.
        match decision.action {
            Action::OpenApp => self.actuator.open_app(text).await.map(|_| (None, "os")),
            Action::OpenUrl => self.actuator.open_url(text).await.map(|_| (None, "os")),
            Action::Shell => self.actuator.shell(text).await.map(|o| (Some(o), "shell")),
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
                    match self.actuator.a11y_act(decision, grounding).await {
                        Ok(true) => return Ok((None, "a11y")),
                        Ok(false) => {}
                        Err(e) => log::debug!("a11y path failed, using the mouse: {e}"),
                    }
                }
                let path = if decision.target.is_some() || decision.action == Action::Scroll {
                    "mouse"
                } else {
                    "keyboard"
                };
                self.actuator.act(decision, grounding).await?;
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
                    self.actuator.act(&enter, grounding).await?;
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
            if d.action == Action::Type {
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
            let (output, path) = match self.act(d, &g).await {
                Ok(x) => x,
                Err(e) => {
                    log::warn!("{label} step {d:?} failed: {e}; handing over to the model");
                    break;
                }
            };
            self.perceptor.settle(Duration::from_secs(3)).await;
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
            r.trace.push(rec);
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
}
