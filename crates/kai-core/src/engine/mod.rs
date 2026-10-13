//! The harness loop: observe → ground → decide → act → verify. Core stays
//! OS-free: the screen and the mouse/keyboard are behind the [`Perceptor`] and
//! [`Actuator`] traits, which `kai-os` implements and tests fake. See
//! `docs/HARNESS.md`.
//!
//! Speed-ups that never trade away safety:
//! - a reply may carry up to 3 certain follow-ups (`then`), run without a new
//!   model call but each re-checked (target, risk, interrupt) — `batch.rs`;
//! - accessibility-first actions before the mouse — `act.rs`;
//! - tree-only steps (no screenshot) when the element list is rich and the
//!   last `expect` is checkable from text, plus an optional cheap verifier and
//!   a zoomed crop after a missed grid click — `observe.rs`;
//! - a plan cache that replays a known task's deterministic prefix — `act.rs`
//!   and [`crate::plan_cache`];
//! - the executor: a local decision model carries out the reply's `plan`,
//!   gated by confidence, approvals unchanged — `execute.rs`.
//!
//! Human checks (`human.rs`, `docs/HUMAN_CHECKS.md`): a CAPTCHA, one-time
//! code or sign-in approval on screen pauses the run until the user deals
//! with it; the model never touches them. `ask_user` becomes a real question
//! when someone can answer.
//!
//! Verification (`gate.rs`, `effect.rs`, `docs/VERIFICATION.md`): with
//! `Config::outcomes`, the first call names required outcomes with read-only
//! probes, and `done` is accepted only when every probed one holds; each
//! action is labelled confirmed / unverifiable / no_op with no model call.

mod act;
mod batch;
mod best_of;
mod effect;
mod execute;
mod gate;
mod human;
mod memory;
mod observe;
mod procedure;
mod record;
mod recover;
pub use recover::RecoveryRecord;
mod run;
pub use procedure::PROCEDURE;
use run::{Pending, Run};

pub use crate::ports::*;

use crate::client::ModelClient;
use crate::executor::Decider;
use crate::memory::MemoryStore;
use crate::plan_cache::PlanCache;
use crate::prompt::{self, HistoryItem, TurnInput};
use crate::recorder::TraceRecorder;
use crate::reply::StepReply;
use crate::risk;
use crate::schema::Action;
use crate::skill::Skill;
use crate::trace::{Outcome, StepRecord, TargetSig, Trace};
use std::time::{Duration, Instant};

/// All the collaborators the loop drives.
pub struct Engine<'a> {
    pub client: &'a dyn ModelClient,
    /// The careful model (31B). Used for the step after a failure, then Kai
    /// drops back to `client`.
    pub careful: Option<&'a dyn ModelClient>,
    pub perceptor: &'a dyn Perceptor,
    pub actuator: &'a dyn Actuator,
    pub approver: &'a dyn Approver,
    pub interrupt: &'a dyn Interrupt,
    pub sink: &'a dyn EventSink,
    /// User messages sent mid-run.
    pub steering: &'a dyn Steering,
    /// Optional cheap verifier (text-only decision model).
    pub verifier: Option<&'a dyn Verifier>,
    /// Who to ask when the run needs a person: a CAPTCHA, a one-time code, a
    /// sign-in approval or a question. `None` (headless, bench) ends the run
    /// as "asked user" at those moments instead.
    pub human: Option<&'a dyn crate::human::HumanHelper>,
    pub skills: &'a [Skill],
    pub config: Config,
}

/// Per-run extras that are not collaborators. `Default` is a plain run.
#[derive(Clone, Copy, Default)]
pub struct RunOptions<'a> {
    /// Replay a cached deterministic prefix for a known task, and record the
    /// prefix of this run if it succeeds.
    pub plan_cache: Option<&'a PlanCache>,
    /// Record this run as training data (prompts, raw replies, the JPEGs
    /// sent). Off unless set; see `docs/DATASET.md`.
    pub recorder: Option<&'a TraceRecorder>,
    /// The executor's decision model (jevos); used when `Config::executor`.
    pub executor: Option<&'a dyn Decider>,
    /// Experience memory: lessons from earlier runs are shown to the model
    /// as hints, and this run is learned from (`Config::learn`) at the end.
    /// See `docs/MEMORY.md`.
    pub memory: Option<&'a MemoryStore>,
    /// The caller judges the run after it ends (a bench check) and calls
    /// [`MemoryStore::after_run`] itself; the engine only recalls.
    pub learn_later: bool,
    /// Read-only OS probes for the outcome ledger (`Config::outcomes`).
    /// Without one, OS probes fail closed (`docs/VERIFICATION.md`).
    pub probe: Option<&'a dyn Probe>,
    /// Stored procedures: replay one for a known task and record this run
    /// when it succeeds (`Config::procedures`, `docs/PROCEDURES.md`).
    pub procedures: Option<&'a crate::procedure::ProcedureStore>,
}

/// Trace backend label for actions run from a model reply's `then` list.
pub const BATCHED: &str = "batched (no model call)";
/// Trace backend label for skill steps.
pub const SKILL: &str = "skill (no model call)";
/// Trace backend label for plan-cache steps.
pub const CACHED: &str = "plan cache (no model call)";
/// Trace backend label for executor steps.
pub const EXECUTOR: &str = crate::executor::BACKEND;
/// Trace backend label for steps the user settled (human checks).
pub use human::HUMAN;

impl Engine<'_> {
    /// Run a task to completion, returning the trace.
    pub async fn run(&self, task: &str) -> Trace {
        self.run_with(task, RunOptions::default()).await
    }

    /// [`Engine::run`] with per-run options (plan cache).
    pub async fn run_with(&self, task: &str, opts: RunOptions<'_>) -> Trace {
        let decider = opts.executor.filter(|_| self.config.executor);
        let mut r = Run::new(task, &opts, decider);
        self.executor_start(&mut r).await;
        let outcome = self.drive(&mut r, opts).await;
        self.finish_ledger(&mut r);
        r.trace.finish(outcome);
        if let Some(store) = opts.memory
            && !opts.learn_later
        {
            self.learn_from(store, &r.trace).await;
        }
        self.wrap_up(&r, opts, outcome);
        r.trace
    }

    async fn drive(&self, r: &mut Run<'_>, opts: RunOptions<'_>) -> Outcome {
        let active_skill = prompt::select_skill(r.task, self.skills);
        let (system, schema) = if r.decider.is_some() {
            let s = prompt::system_prompt_with_plan(self.skills);
            (s, StepReply::json_schema_with_plan())
        } else {
            (prompt::system_prompt(self.skills), StepReply::json_schema())
        };
        let deadline = Instant::now() + self.config.max_time;
        r.system_hash = r.recorder.map(|rec| rec.record_system(&system));

        // A skill's deterministic steps, a stored procedure, or a cached
        // plan's prefix run first with no model call.
        if let Some(o) = self.replay_procedure(r, opts, active_skill).await {
            return o;
        }
        let prefix = match active_skill {
            Some(sk) if !sk.steps.is_empty() => Some((
                sk.steps.iter().map(act::fixed_decision).collect::<Vec<_>>(),
                SKILL,
            )),
            _ if r.procedure.is_some() => None,
            _ => opts
                .plan_cache
                .and_then(|c| c.lookup(r.task))
                .map(|p| (p.steps, CACHED)),
        };
        if let Some((steps, label)) = prefix
            && let Some(o) = self.run_prefix(r, &steps, label).await
        {
            return o;
        }
        // A long task with nothing remembered: plan it a few ways first.
        if r.procedure.is_none() && active_skill.is_none() {
            self.best_of_plan(r).await;
        }

        while r.step < self.config.max_steps {
            if self.interrupt.stopped() {
                return Outcome::Stopped;
            }
            // Time spent waiting for the user is not Kai's time.
            if Instant::now() >= deadline + r.human.waited {
                return Outcome::TimeLimit;
            }
            if let Some(o) = self.model_step(r, &system, &schema, active_skill).await {
                return o;
            }
        }
        Outcome::StepLimit
    }

    /// Ask the user to approve a step. The wait is the user's time, not
    /// Kai's: it moves the run's deadline like a human check does (seen
    /// live: an approval answered after ten minutes ended the run at once
    /// on its time limit).
    async fn ask_approval(
        &self,
        r: &mut Run<'_>,
        d: &crate::schema::Decision,
        g: &crate::element::Grounding,
    ) -> ApprovalChoice {
        let t = Instant::now();
        let choice = self.approver.approve(d, g).await;
        r.human.waited += t.elapsed();
        choice
    }

    /// One observe → decide → act (→ batch) round. `Some` ends the run.
    async fn model_step(
        &self,
        r: &mut Run<'_>,
        system: &str,
        schema: &serde_json::Value,
        active_skill: Option<&Skill>,
    ) -> Option<Outcome> {
        let step = r.step;
        self.drain_steering(r, step);
        r.exec_cooldown = r.exec_cooldown.saturating_sub(1);

        // ---- Observe + Ground (+ cheap verify of the last expect) ----
        let seen = match self.observe_step(r, false).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("observe failed: {e}");
                r.trace
                    .errors
                    .push(format!("observe error at step {step}: {e}"));
                r.step += 1;
                r.failure_streak += 1;
                return (r.failure_streak >= self.config.max_failures).then_some(Outcome::Failed);
            }
        };
        let obs = &seen.obs;
        let elements = obs.grounding.as_text();
        self.ledger_observe(r, step, obs);

        // ---- Something only the user can get past (CAPTCHA, code, approval) ----
        if let Some(check) = self.unhandled_check(r, &obs.grounding) {
            return self.pause_for_human(r, check, None, &elements).await;
        }

        // ---- Decide ----
        self.sink.emit(&EngineEvent::Thinking { step });
        let mut notes = std::mem::take(&mut r.notes);
        notes.extend(r.plan_note.clone());
        let first = self.outcome_request(r, &mut notes, schema);
        let lessons = self.lessons_for(r, obs);
        let verified = self.config.macro_history.then(|| self.verified_outcomes(r));
        let user_text = prompt::build_turn(&TurnInput {
            lessons: lessons.as_deref(),
            task: r.task,
            grounding: &obs.grounding,
            history: &r.history,
            active_skill,
            no_progress: r.no_progress_streak >= 1,
            notes: &notes,
            has_screenshot: !obs.images.screenshot_jpeg.is_empty(),
            macro_history: verified.as_deref(),
        });
        let escalate = r.failure_streak > 0 || r.no_progress_streak > 0 || r.careful_next;
        r.careful_next = false;
        let (client, careful) = match (escalate, self.careful) {
            (true, Some(c)) => (c, true),
            _ => (self.client, false),
        };
        let (schema, extra) = first.as_ref().map_or((schema, 0), |(s, n)| (s, *n));
        let max_tokens = self.config.max_tokens + extra;
        let decided = self
            .decide(client, system, &user_text, &obs.images, schema, max_tokens)
            .await;
        r.step += 1;
        let decided = match decided {
            Ok(d) => d,
            Err(e) => {
                log::warn!("undecodable reply at step {step}: {e}");
                r.trace.errors.push(record::error_line(step, &e));
                r.failure_streak += 1;
                r.force_full = true;
                return (r.failure_streak >= self.config.max_failures)
                    .then_some(Outcome::AskedUser);
            }
        };
        r.failure_streak = 0;
        self.ledger_start(r, step, &decided.reply.outcomes);
        let call = self.record_call(r, step, &user_text, obs, &decided, careful);
        let (reply, model_time, first_token) = (decided.reply, decided.total, decided.first_token);
        if reply.need_screen {
            r.force_full = true;
        }
        let decision = reply.decision;

        let mut timings = seen.timings;
        timings.set_model(model_time, first_token);
        let backend = client.backend().label();
        let mut rec = StepRecord::new(step, &decision, timings, backend);
        rec.elements = elements;
        rec.target = TargetSig::of(&decision, &obs.grounding);
        rec.screen = crate::memory::scope::screen_sig(obs.window_title.as_deref(), &obs.grounding);
        if let Some((shot, call)) = call {
            rec.screenshot = shot;
            rec.call = call;
        }

        self.sink.emit(&EngineEvent::Decided {
            step,
            decision: Box::new(decision.clone()),
            target: decision.target.and_then(|t| obs.grounding.target_rect(t)),
        });

        // ---- ask_user: a real question when someone can answer ----
        if decision.action == Action::AskUser && self.human.is_some() {
            let q = crate::human::HumanCheck::question(
                decision.text.as_deref().unwrap_or(&decision.thought),
            );
            return self.pause_for_human(r, q, Some(rec), "").await;
        }

        // ---- The model never works a CAPTCHA or guesses a one-time code ----
        if let Some(check) = crate::human::guard(&decision, &obs.grounding) {
            return self.refuse_for_human(r, check, rec).await;
        }

        // ---- done (through the gate) / ask_user terminate ----
        match decision.action {
            Action::Done => return self.claim_done(r, rec, obs).await,
            Action::AskUser => {
                rec.path = Some("none".into());
                r.trace.push(rec);
                return Some(Outcome::AskedUser);
            }
            _ => {}
        }

        // ---- Approval for high-risk ----
        let final_risk = risk::classify(&decision, &obs.grounding);
        let pw = risk::is_payment_or_password(&decision, &obs.grounding);
        if risk::needs_approval(self.config.approval, final_risk, pw) {
            self.sink.emit(&EngineEvent::AwaitingApproval { step });
            match self.ask_approval(r, &decision, &obs.grounding).await {
                ApprovalChoice::Approve => rec.approval = Some("approved".to_string()),
                ApprovalChoice::Skip => {
                    rec.approval = Some("skipped".to_string());
                    rec.verified = false;
                    rec.path = Some("none".into());
                    r.trace.push(rec);
                    r.history
                        .push(HistoryItem::note(format!("skipped {:?}", decision.action)));
                    r.pending = None;
                    return None;
                }
                ApprovalChoice::Stop => return Some(Outcome::Stopped),
            }
        }

        // ---- Act ----
        let t_act = Instant::now();
        let guarded = act::reopen_guard(&mut r.opened, &decision)
            .or_else(|| act::repeat_guard(&r.history, &decision));
        let refused = guarded.is_some();
        r.events_mark = self.perceptor.ui_events();
        let acted = match guarded {
            Some(note) => Ok((Some(note), "none")),
            None => self.act(&decision, &obs.grounding).await,
        };
        let (output, path) = match acted {
            Ok((o, p)) => (o, p),
            Err(e) => {
                log::error!("act failed at step {step}: {e}");
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

        // ---- Verify (next observation checks `expect`) ----
        self.perceptor.settle(Duration::from_secs(3)).await;

        rec.path = Some(path.to_string());
        rec.output = output.clone();
        r.trace.push(rec);
        let mut pending = Pending {
            summary: HistoryItem::from_decision(&decision).summary,
            action: decision.action,
            expect: decision.expect.clone(),
            typed: (decision.action == Action::Type)
                .then(|| decision.text.clone())
                .flatten(),
            keys: decision.keys.clone(),
            popup: observe::opens_popup(&decision, &obs.grounding),
            records: vec![r.trace.steps.len() - 1],
        };
        let mut item = HistoryItem::from_step(&decision, &obs.grounding);
        let acted_ok = path != "error";
        if let Some(out) = &output {
            item.push_output(out);
            // Terminal work changes no pixels; it is progress by itself.
            // A refused repeat is not: it must not keep a loop alive.
            if !refused {
                r.no_progress_streak = 0;
            }
        }
        if decision.action == Action::Wait {
            // Waiting is deliberate; an unchanged screen is expected.
            r.no_progress_streak = 0;
        }
        r.history.push(item);
        r.grid_click = observe::grid_click(&decision, &obs.grounding);

        // ---- Follow-ups the model was certain about ----
        if acted_ok
            && !reply.then.is_empty()
            && self.config.max_batch > 0
            && let Some(o) = self
                .run_batch(r, &reply.then, &obs.grounding, &mut pending)
                .await
        {
            return Some(o);
        }
        r.pending = Some(pending);

        if r.no_progress_streak >= 2 && !self.recover(r) {
            // Two steps with no change despite acting: stop and ask.
            return Some(Outcome::AskedUser);
        }
        // ---- The routine subgoals after this step: the executor ----
        if acted_ok && !reply.plan.is_empty() {
            return self.run_plan(r, &reply.plan).await;
        }
        None
    }
}
