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
                if let Some(t) = decision.target
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
                self.actuator
                    .act(decision, grounding)
                    .await
                    .map(|_| (None, path))
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
            if label == CACHED {
                let level = self.config.approval;
                let pw = risk::is_payment_or_password(d, &none);
                if risk::needs_approval(level, risk::classify(d, &none), pw) {
                    break;
                }
            }
            let t = Instant::now();
            let (output, path) = match self.act(d, &none).await {
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
