//! Replaying a stored procedure (`crate::procedure`, `docs/PROCEDURES.md`).
//!
//! Each step's target is found again by role and name on a fresh tree read;
//! if it is missing or ambiguous, would need approval, or meets a human
//! check, the replay stops there and the model takes over with the rest of
//! the procedure as a hint. The model always sees a full frame afterwards
//! and decides `done` itself (through the gate when outcomes are on).

use super::{Engine, Pending, Run, RunOptions};
use crate::element::{Element, Grounding};
use crate::ports::EngineEvent;
use crate::procedure::{Bound, ProcStep};
use crate::prompt::HistoryItem;
use crate::risk;
use crate::skill::Skill;
use crate::trace::{Outcome, StepRecord, Timings};
use std::time::{Duration, Instant};

/// Trace backend label for replayed procedure steps.
pub const PROCEDURE: &str = "procedure (no model call)";

impl Engine<'_> {
    /// Replay the stored procedure for this task, if procedures are on, one
    /// is active for it, and no skill with fixed steps applies. `Some` ends
    /// the run (stopped).
    pub(super) async fn replay_procedure(
        &self,
        r: &mut Run<'_>,
        opts: RunOptions<'_>,
        skill: Option<&Skill>,
    ) -> Option<Outcome> {
        if !self.config.procedures || skill.is_some_and(|s| !s.steps.is_empty()) {
            return None;
        }
        let b = opts.procedures?.lookup(r.task)?;
        self.run_procedure(r, &b).await
    }

    /// At the end of the run: credit or quarantine the replayed procedure,
    /// or record this run as one.
    pub(super) fn finish_procedure(&self, r: &Run<'_>, opts: RunOptions<'_>, outcome: Outcome) {
        let Some(store) = opts.procedures.filter(|_| self.config.procedures) else {
            return;
        };
        let res = match &r.procedure {
            Some(shape) => store.report(shape, outcome == Outcome::Success),
            None if outcome == Outcome::Success => store
                .record(&r.trace, crate::procedure::all_verified(&r.trace))
                .map(|stored| log::debug!("procedure recorded: {stored}")),
            None => Ok(()),
        };
        if let Err(e) = res {
            log::warn!("procedure store update failed: {e}");
        }
    }

    /// Replay `b`. `Some` ends the run (stopped).
    async fn run_procedure(&self, r: &mut Run<'_>, b: &Bound) -> Option<Outcome> {
        let mut done = 0usize;
        let mut stopped_at: Option<String> = None;
        for st in &b.steps {
            if self.interrupt.stopped() {
                return Some(Outcome::Stopped);
            }
            if r.step >= self.config.max_steps {
                break;
            }
            if self.drain_steering(r, r.step) {
                stopped_at = Some("you sent a message".into());
                break;
            }
            let t = Instant::now();
            let (d, g) = match self.resolve(r, st).await {
                Ok(x) => x,
                Err(why) => {
                    stopped_at = Some(why);
                    break;
                }
            };
            let tree_ms = t.elapsed().as_millis() as u64;
            let pw = risk::is_payment_or_password(&d, &g);
            if risk::needs_approval(self.config.approval, risk::classify(&d, &g), pw) {
                stopped_at = Some("the next step needs your approval".into());
                break;
            }
            if crate::human::guard(&d, &g).is_some() || self.unhandled_check(r, &g).is_some() {
                stopped_at = Some("a check only the user can do is on screen".into());
                break;
            }
            let t_act = Instant::now();
            r.protect(&d, &g, false);
            let (output, path) = match self.act(&d, &g, &r.privacy, None).await {
                Ok(x) => x,
                Err(e) => {
                    let _ = e;
                    stopped_at = Some("the step failed".into());
                    break;
                }
            };
            self.perceptor.settle(Duration::from_secs(3)).await;
            let output = output.map(|text| r.privacy.text(&text));
            let timings = Timings {
                tree_ms,
                act_ms: t_act.elapsed().as_millis() as u64,
                ..Default::default()
            };
            let step = r.step;
            self.sink.emit(&EngineEvent::Acted { step, timings });
            self.emit_output(step, &d, output.as_deref());
            let mut rec = StepRecord::new(step, &d, timings, PROCEDURE);
            rec.path = Some(path.to_string());
            rec.output = output.clone();
            rec.target = st.target.clone();
            if matches!(g, Grounding::Marks(ref els) if !els.is_empty()) {
                rec.elements = g.as_text();
            }
            r.push(rec);
            let mut item = HistoryItem::from_step(&d, &g);
            if let Some(o) = &output {
                item.push_output(o);
            }
            r.history.push(item);
            r.step += 1;
            done += 1;
        }
        r.procedure = Some(b.shape.clone());
        r.notes
            .push(handover_note(done, &b.steps, stopped_at.as_deref()));
        // Verify with a full observation before the model goes on.
        r.force_full = true;
        r.pending = None::<Pending>;
        None
    }

    /// The step's decision with its target found again, and the grounding
    /// it was found on. Every step reads the screen first, untargeted ones
    /// too: a CAPTCHA, suspicious text or a focused password field there
    /// must stop the replay just as it would a model step.
    async fn resolve(
        &self,
        r: &Run<'_>,
        st: &ProcStep,
    ) -> Result<(crate::schema::Decision, Grounding), String> {
        let mut d = st.decision.clone();
        let mut obs = self
            .perceptor
            .observe_tree_only()
            .await
            .map_err(|e| format!("reading the screen failed: {e}"))?;
        r.redact(&mut obs.grounding);
        let Some(sig) = &st.target else {
            return Ok((d, obs.grounding));
        };
        let Grounding::Marks(els) = &obs.grounding else {
            return Err("no accessibility tree on this screen".into());
        };
        let hits: Vec<&Element> = els
            .iter()
            .filter(|e| e.role == sig.role && e.name.trim().eq_ignore_ascii_case(&sig.name))
            .collect();
        match hits.as_slice() {
            [e] if e.enabled => d.target = Some(e.mark),
            [_] => return Err(format!("{} \"{}\" is disabled", sig.role.label(), sig.name)),
            [] => {
                return Err(format!(
                    "{} \"{}\" is not on screen",
                    sig.role.label(),
                    sig.name
                ));
            }
            _ => {
                return Err(format!(
                    "{} {} \"{}\" are on screen",
                    hits.len(),
                    sig.role.label(),
                    sig.name
                ));
            }
        }
        Ok((d, obs.grounding))
    }
}

/// What the model is told once the replay ends.
fn handover_note(done: usize, steps: &[ProcStep], stopped: Option<&str>) -> String {
    let n = steps.len();
    match stopped {
        None if done == n => format!(
            "All {n} steps of a remembered procedure for this task already ran (see your \
             last steps). Check the screen: if the task is complete, reply done; otherwise \
             continue."
        ),
        why => {
            let rest: Vec<String> = steps[done..]
                .iter()
                .take(6)
                .map(|s| {
                    let mut p = HistoryItem::from_decision(&s.decision).summary;
                    if let Some(t) = &s.target {
                        p.push_str(&format!(" {} \"{}\"", t.role.label(), t.name));
                    }
                    p
                })
                .collect();
            format!(
                "{done} of {n} steps of a remembered procedure for this task ran, then it \
                 stopped ({}). The remaining steps were: {}. Use them as a guide, not as \
                 orders.",
                why.unwrap_or("step limit"),
                rest.join("; ")
            )
        }
    }
}
