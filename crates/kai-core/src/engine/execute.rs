//! Running the model's `plan` with the executor (a local decision model):
//! per subgoal, one fresh tree-only observation, one decision request
//! (target / already done / ready, plus "did the previous subgoal work?"),
//! the gates in [`crate::executor::judge`], then the normal approval and act
//! path. Control goes back to the model (with a note) on any doubt, a failed
//! or unconfirmed subgoal, no progress, a steering message, a `done` subgoal,
//! or when the plan runs out. See `docs/EXECUTOR.md`.

use super::{EXECUTOR, Engine, Pending, Run, observe};
use crate::client::{Answer, ClientError};
use crate::executor::{self as ex, Verdict};
use crate::plan::{Subgoal, SubgoalKind};
use crate::ports::{ApprovalChoice, EngineEvent, Observation};
use crate::prompt::HistoryItem;
use crate::risk;
use crate::trace::{ExecutorStats, Outcome, StepRecord, Timings};
use std::time::{Duration, Instant};

fn stats<'a>(r: &'a mut Run<'_>) -> &'a mut ExecutorStats {
    r.trace.executor.get_or_insert_with(ExecutorStats::default)
}

impl Engine<'_> {
    /// Once per run: is the executor's endpoint there? If not, run without it.
    pub(super) async fn executor_start(&self, r: &mut Run<'_>) {
        if !self.config.executor {
            return;
        }
        let Some(d) = r.decider else {
            stats(r).unavailable = Some("no executor configured for this run".into());
            return;
        };
        match d.health().await {
            Ok(t) => {
                let s = stats(r);
                s.enabled = true;
                s.health_ms = Some(t.as_millis() as u64);
            }
            Err(e) => {
                log::warn!("executor unreachable, running without it: {e}");
                stats(r).unavailable = Some(e.to_string());
                r.decider = None;
            }
        }
    }

    /// Hand control back to the model with `note`. `full`: its next look
    /// must be a screenshot (it has not seen what the executor did).
    fn escalate(&self, r: &mut Run<'_>, note: String, full: bool) {
        log::info!("executor escalates: {note}");
        r.notes.push(note);
        r.force_full |= full;
        stats(r).escalations += 1;
        r.exec_streak += 1;
        if r.exec_streak >= 2 {
            r.exec_streak = 0;
            r.exec_cooldown = self.config.executor_gates.cooldown_steps;
        }
    }

    /// Ask the decision model; the round trip is logged in the stats. A
    /// transport/availability error turns the executor off for the run.
    async fn exec_ask(
        &self,
        r: &mut Run<'_>,
        state: &serde_json::Value,
        qs: &[(String, crate::client::Question)],
    ) -> Option<(std::collections::BTreeMap<String, Answer>, u64)> {
        let d = r.decider?;
        let t = Instant::now();
        let res = d.ask(state, qs).await;
        let ms = t.elapsed().as_millis() as u64;
        let s = stats(r);
        s.decisions += 1;
        s.latencies_ms.push(ms);
        match res {
            Ok(a) => Some((a, ms)),
            Err(e) => {
                if matches!(
                    e,
                    ClientError::Transport(_)
                        | ClientError::Unavailable(_)
                        | ClientError::MissingKey(_)
                ) {
                    log::warn!("executor off for this run: {e}");
                    r.decider = None;
                }
                None
            }
        }
    }

    /// Record P(expect met) for the executor step at `idx`.
    fn mark_met(&self, r: &mut Run<'_>, idx: usize, met: Option<f64>) {
        let ok = self.config.executor_gates.met_ok;
        if let Some(s) = r.trace.steps.get_mut(idx) {
            if let Some(x) = s.executor.as_mut() {
                x.expect_met = met;
            }
            if let Some(m) = met {
                s.verified = m >= ok;
            }
        }
    }

    /// Did the subgoal that just ran work? `met` decides when it is clear;
    /// in between (or unknown), an unchanged screen means it did not. True
    /// when control went back to the model.
    fn check_prev(&self, r: &mut Run<'_>, p: &Subgoal, idx: usize, met: Option<f64>) -> bool {
        let g = self.config.executor_gates;
        self.mark_met(r, idx, met);
        let note = match met {
            Some(m) if m >= g.met_ok => {
                // Confirmed: an unchanged element list is not "no progress".
                r.no_progress_streak = 0;
                return false;
            }
            Some(m) if m <= g.met_bad => format!(
                "After '{}' the expected result (\"{}\") does not seem to hold (p={m:.2}). \
                 Look carefully.",
                p.describe(),
                p.expect
            ),
            _ if r.no_progress_streak > 0 && p.kind != SubgoalKind::Shell => format!(
                "Nothing changed after '{}' and its result (\"{}\") is unconfirmed.",
                p.describe(),
                p.expect
            ),
            _ => return false,
        };
        self.escalate(r, note, true);
        true
    }

    /// Run the plan's subgoals. `Some` ends the run (stop only).
    pub(super) async fn run_plan(&self, r: &mut Run<'_>, plan: &[Subgoal]) -> Option<Outcome> {
        if r.decider.is_none() || r.exec_cooldown > 0 {
            return None;
        }
        let gates = self.config.executor_gates;
        stats(r).plans += 1;
        // The last executed subgoal and its trace index, awaiting its check.
        let mut prev: Option<(Subgoal, usize)> = None;
        let mut reuse: Option<(Observation, u64)> = None;
        let mut acted_any = false;
        let mut done_next = false;
        for sg in plan.iter().take(gates.max_subgoals) {
            if self.interrupt.stopped() {
                return Some(Outcome::Stopped);
            }
            if r.step >= self.config.max_steps || r.decider.is_none() {
                return None;
            }
            if self.drain_steering(r, r.step) {
                stats(r).escalations += 1;
                return None;
            }
            if sg.kind == SubgoalKind::Done {
                done_next = true;
                break;
            }
            let (obs, tree_ms) = match reuse.take() {
                Some(o) => o,
                None => {
                    let t = Instant::now();
                    match self.observe_step(r, true).await {
                        Ok(s) => (s.obs, t.elapsed().as_millis() as u64),
                        Err(e) => {
                            self.escalate(r, format!("Executor could not look: {e}"), true);
                            return None;
                        }
                    }
                }
            };
            let cands = ex::candidates(&obs.grounding, sg, gates.max_candidates);
            if sg.kind.needs_target() && cands.is_empty() {
                let note = format!(
                    "Executor found no element that could be the {} for '{}'.",
                    sg.target.trim(),
                    sg.describe()
                );
                self.escalate(r, note, acted_any);
                return None;
            }
            let prev_sg = prev.as_ref().map(|(p, _)| p);
            let state = ex::state(r.task, sg, &obs.grounding, prev_sg);
            let qs = ex::questions(sg, &cands, prev_sg);
            let Some((answers, ms)) = self.exec_ask(r, &state, &qs).await else {
                let note = format!("Executor unavailable for '{}'.", sg.describe());
                self.escalate(r, note, acted_any);
                return None;
            };
            if let Some((p, idx)) = prev.take() {
                let met = answers.get(ex::Q_PREV).and_then(Answer::probability);
                if self.check_prev(r, &p, idx, met) {
                    return None;
                }
            }
            let (verdict, mut xrec) = ex::judge(sg, &answers, &cands, &gates);
            xrec.latency_ms = ms;
            let mark = match verdict {
                Verdict::Skip => {
                    stats(r).skipped += 1;
                    reuse = Some((obs, 0));
                    continue;
                }
                Verdict::Escalate(note) => {
                    self.escalate(r, note, acted_any);
                    return None;
                }
                Verdict::Act(m) => m,
            };
            let decision = match sg.to_decision(mark) {
                Ok(d) => d,
                Err(e) => {
                    let note = format!("Executor could not run '{}': {e}", sg.describe());
                    self.escalate(r, note, acted_any);
                    return None;
                }
            };
            let g = &obs.grounding;
            let step = r.step;
            r.step += 1;
            let timings = Timings {
                tree_ms,
                verify_ms: ms,
                ..Default::default()
            };
            let mut rec = StepRecord::new(step, &decision, timings, EXECUTOR);
            rec.elements = g.as_text();
            rec.executor = Some(xrec);
            self.sink.emit(&EngineEvent::Decided {
                step,
                decision: Box::new(decision.clone()),
                target: decision.target.and_then(|t| g.target_rect(t)),
            });

            // ---- Approval: Kai's rules, exactly as for a model step ----
            let pw = risk::is_payment_or_password(&decision, g);
            if risk::needs_approval(self.config.approval, risk::classify(&decision, g), pw) {
                self.sink.emit(&EngineEvent::AwaitingApproval { step });
                match self.approver.approve(&decision, g).await {
                    ApprovalChoice::Approve => rec.approval = Some("approved".into()),
                    ApprovalChoice::Skip => {
                        rec.approval = Some("skipped".into());
                        rec.verified = false;
                        rec.path = Some("none".into());
                        r.trace.push(rec);
                        r.pending = None;
                        let note = format!("You (the user) skipped '{}'.", sg.describe());
                        self.escalate(r, note, acted_any);
                        return None;
                    }
                    ApprovalChoice::Stop => return Some(Outcome::Stopped),
                }
            }

            // ---- Act ----
            let t_act = Instant::now();
            let acted = match super::act::reopen_guard(&mut r.opened, &decision) {
                Some(note) => Ok((Some(note), "none")),
                None => self.act(&decision, g).await,
            };
            let (output, path) = match acted {
                Ok(x) => x,
                Err(e) => {
                    log::error!("executor act failed at step {step}: {e}");
                    r.failure_streak += 1;
                    (Some(format!("ERROR: {e}")), "error")
                }
            };
            rec.timings.act_ms = t_act.elapsed().as_millis() as u64;
            self.sink.emit(&EngineEvent::Acted {
                step,
                timings: rec.timings,
            });
            self.emit_output(step, &decision, output.as_deref());
            self.perceptor.settle(Duration::from_secs(3)).await;
            rec.path = Some(path.to_string());
            rec.output = output.clone();
            rec.verified = false; // confirmed by the next request
            r.trace.push(rec);
            let idx = r.trace.steps.len() - 1;
            let mut item = HistoryItem::from_decision(&decision);
            item.summary.push_str(" (executor)");
            if let Some(o) = &output {
                item.push_output(o);
                r.no_progress_streak = 0;
            }
            r.history.push(item);
            r.pending = Some(Pending {
                summary: HistoryItem::from_decision(&decision).summary,
                action: decision.action,
                expect: decision.expect.clone(),
                typed: decision
                    .text
                    .clone()
                    .filter(|_| decision.action == crate::schema::Action::Type),
                keys: decision.keys.clone(),
                popup: observe::opens_popup(&decision, g),
                records: vec![idx],
            });
            r.grid_click = None;
            stats(r).acted += 1;
            acted_any = true;
            if path == "error" {
                let note = format!("Executor's '{}' failed.", sg.describe());
                self.escalate(r, note, true);
                return None;
            }
            prev = Some((sg.clone(), idx));
        }
        // The plan ran out: check the last subgoal, then the model re-plans.
        if let Some((p, idx)) = prev
            && !p.expect.trim().is_empty()
            && let Ok(seen) = self.observe_step(r, true).await
        {
            let state = serde_json::json!({
                "task": r.task,
                "previous": { "subgoal": p.describe(), "expect": p.expect },
                "elements": seen.obs.grounding.as_compact_text(),
            });
            let qs = [(ex::Q_PREV.to_string(), ex::met_question(&p))];
            if let Some((a, _)) = self.exec_ask(r, &state, &qs).await {
                let met = a.get(ex::Q_PREV).and_then(Answer::probability);
                if self.check_prev(r, &p, idx, met) {
                    return None;
                }
            }
        }
        if done_next {
            r.notes.push(
                "The executor finished your plan, which ends with `done`. Check the \
                 screen and reply `done` only if the task is complete."
                    .into(),
            );
        }
        // The model has not seen what the executor did: give it a real look.
        r.force_full |= acted_any;
        r.exec_streak = 0;
        None
    }
}
