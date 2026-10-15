//! Multi-action steps: run the follow-ups (`then`) a reply was certain about,
//! with no new model call. Each one is re-checked before it runs; the batch
//! stops early, leaving the rest to the model, when:
//! - the interrupt fires (the run stops) or the user steers;
//! - a follow-up needs approval (it is never run blind; the model proposes it
//!   again next step and it goes through the normal approval path);
//! - it has a target and, after a fresh tree-only observation, that number no
//!   longer names the same element (role + name) the model saw, or the
//!   screen is a grid (no blind grid clicks);
//! - a follow-up would work a CAPTCHA or type a one-time code (only the user
//!   does that; the next step pauses for them);
//! - an action fails, or the step limit is reached.

use super::{BATCHED, Engine, Pending, Run, observe};
use crate::element::Grounding;
use crate::ports::EngineEvent;
use crate::prompt::HistoryItem;
use crate::risk;
use crate::schema::{Action, Decision};
use crate::trace::{Outcome, StepRecord, Timings};
use std::time::{Duration, Instant};

/// Does mark `t` still name the element the model saw?
pub(super) fn same_target(seen: &Grounding, now: &Grounding, t: u32) -> bool {
    match (seen.element(t), now.element(t)) {
        (Some(a), Some(b)) => a.role == b.role && a.name == b.name && b.enabled,
        _ => false,
    }
}

impl Engine<'_> {
    pub(super) async fn run_batch(
        &self,
        r: &mut Run<'_>,
        follow: &[Decision],
        seen: &Grounding,
        pending: &mut Pending,
    ) -> Option<Outcome> {
        let mut grounding = seen.clone();
        for d in follow.iter().take(self.config.max_batch as usize) {
            if self.interrupt.stopped() {
                return Some(Outcome::Stopped);
            }
            if r.step >= self.config.max_steps {
                break;
            }
            if self.drain_steering(r, r.step) {
                log::info!("batch stopped: the user steered");
                break;
            }
            let mut timings = Timings::default();
            if !matches!(
                d.action,
                Action::Shell | Action::OpenApp | Action::OpenUrl | Action::Wait
            ) {
                let t_obs = Instant::now();
                let now = match self.perceptor.observe_tree_only().await {
                    Ok(mut o) => {
                        r.redact(&mut o.grounding);
                        o.grounding
                    }
                    Err(e) => {
                        log::info!("batch stopped: re-observe failed: {e}");
                        break;
                    }
                };
                timings.tree_ms = t_obs.elapsed().as_millis() as u64;
                if let Some(t) = d.target
                    && (!matches!(seen, Grounding::Marks(_)) || !same_target(seen, &now, t))
                {
                    log::info!("batch stopped: target #{t} changed or vanished");
                    break;
                }
                grounding = now;
            }
            let pw = risk::is_payment_or_password(d, &grounding);
            if risk::needs_approval(self.config.approval, risk::classify(d, &grounding), pw) {
                log::info!("batch stopped: {:?} needs approval", d.action);
                break;
            }
            // A check (CAPTCHA, suspicious text) that appeared since the
            // reply: the next model step raises it.
            if crate::human::guard(d, &grounding).is_some()
                || self.unhandled_check(r, &grounding).is_some()
            {
                log::info!("batch stopped: {:?} is the user's to do", d.action);
                break;
            }

            let step = r.step;
            r.step += 1;
            r.protect(d, &grounding, false);
            self.sink.emit(&EngineEvent::Decided {
                step,
                decision: Box::new(r.privacy.decision(d)),
                target: d.target.and_then(|t| grounding.target_rect(t)),
            });
            let t_act = Instant::now();
            r.protect(d, &grounding, false);
            let result = match super::act::reopen_guard(&mut r.opened, d) {
                Some(note) => Ok((Some(note), "none")),
                None => self.act(d, &grounding, &r.privacy, None).await,
            };
            timings.act_ms = t_act.elapsed().as_millis() as u64;
            let (output, path, ok) = match result {
                Ok((o, p)) => (o, p, true),
                Err(e) => {
                    let _ = e;
                    let e = "action failed";
                    log::error!("batched act failed at step {step}");
                    r.failure_streak += 1;
                    (Some(format!("ERROR: {e}")), "error", false)
                }
            };
            let output = output.map(|text| r.privacy.text(&text));
            self.sink.emit(&EngineEvent::Acted { step, timings });
            self.emit_output(step, d, output.as_deref());
            self.perceptor.settle(Duration::from_secs(3)).await;

            let mut rec = StepRecord::new(step, d, timings, BATCHED);
            rec.elements = grounding.as_text();
            rec.target = crate::trace::TargetSig::of(d, &grounding);
            rec.path = Some(path.to_string());
            rec.output = output.clone();
            // Confirmed by the next observation (or the verifier).
            rec.verified = false;
            r.push(rec);
            let mut item = HistoryItem::from_step(d, &grounding);
            item.summary.push_str(" (batched)");
            if let Some(o) = &output {
                item.push_output(o);
                r.no_progress_streak = 0;
            }
            r.history.push(item);
            pending.records.push(r.trace.steps.len() - 1);
            pending.summary = HistoryItem::from_decision(d).summary;
            pending.action = d.action;
            pending.keys = d.keys.clone();
            pending.popup = observe::opens_popup(d, &grounding);
            if d.action == Action::Type {
                pending.typed = d.text.clone();
            }
            if !d.expect.trim().is_empty() || d.action == Action::Shell {
                pending.expect = d.expect.clone();
            }
            r.grid_click = observe::grid_click(d, &grounding);
            if !ok {
                break;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Element, Rect, Role};

    fn g(name: &str) -> Grounding {
        Grounding::Marks(vec![Element {
            mark: 1,
            role: Role::Button,
            name: name.into(),
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }])
    }

    #[test]
    fn target_identity_is_role_and_name() {
        assert!(same_target(&g("Save"), &g("Save"), 1));
        assert!(!same_target(&g("Save"), &g("Delete"), 1));
        assert!(!same_target(&g("Save"), &g("Save"), 2));
    }
}
