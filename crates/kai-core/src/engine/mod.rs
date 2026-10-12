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
mod effect;
mod execute;
mod gate;
mod human;
mod memory;
mod observe;
mod record;

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
use crate::trace::{Outcome, StepRecord, Trace};
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

/// What the last action expected, waiting for the next observation to check.
struct Pending {
    summary: String,
    action: Action,
    expect: String,
    /// Text typed by this step (or batch), to look for in a tree-only read.
    typed: Option<String>,
    /// Keys pressed last; Enter usually submits/navigates, so look properly.
    keys: Vec<String>,
    /// The last click hit a dropdown/menu: a popup needs a real look.
    popup: bool,
    /// Indices into `trace.steps` this check confirms.
    records: Vec<usize>,
}

/// Mutable state of one run.
struct Run<'t> {
    task: &'t str,
    trace: Trace,
    history: Vec<HistoryItem>,
    step: u32,
    /// (screen hash, element-list hash, was tree-only) of the last observation.
    last_sig: Option<(u64, u64, bool)>,
    no_progress_streak: u32,
    failure_streak: u32,
    /// The next observation must include a screenshot.
    force_full: bool,
    /// Use the careful model for the next decision.
    careful_next: bool,
    tree_only_streak: u32,
    /// The last observation had a rich Set-of-Marks list.
    last_rich: bool,
    pending: Option<Pending>,
    /// One-shot lines for the next user turn.
    notes: Vec<String>,
    /// A grid click (cell number, screen rect) that may have missed.
    grid_click: Option<(u32, crate::element::Rect)>,
    /// URLs already opened in this run (normalised), to refuse a reload that
    /// would wipe what was typed.
    opened: Vec<String>,
    /// Where this run is recorded, if anywhere, and the system prompt hash.
    recorder: Option<&'t TraceRecorder>,
    system_hash: Option<String>,
    /// The executor's decision model, while it is usable this run.
    decider: Option<&'t dyn Decider>,
    /// Escalations in a row, and model steps left before plans run again.
    exec_streak: u32,
    exec_cooldown: u32,
    /// Experience memory, the apps/sites seen so far and the lesson block.
    memory: Option<&'t MemoryStore>,
    mem: memory::Recalled,
    /// Human checks this run: handled ones, codes typed, time waited.
    human: human::HumanState,
    /// Required outcomes, gate refusals, and the OS probe port.
    ledger: gate::LedgerState,
    probe: Option<&'t dyn Probe>,
    /// Accessibility-event count taken just before the last action.
    events_mark: Option<u64>,
}

impl Engine<'_> {
    /// Run a task to completion, returning the trace.
    pub async fn run(&self, task: &str) -> Trace {
        self.run_with(task, RunOptions::default()).await
    }

    /// [`Engine::run`] with per-run options (plan cache).
    pub async fn run_with(&self, task: &str, opts: RunOptions<'_>) -> Trace {
        let mut r = Run {
            task,
            trace: Trace::new(task),
            history: Vec::new(),
            step: 0,
            last_sig: None,
            no_progress_streak: 0,
            failure_streak: 0,
            force_full: true,
            careful_next: false,
            tree_only_streak: 0,
            last_rich: false,
            pending: None,
            opened: Vec::new(),
            notes: Vec::new(),
            grid_click: None,
            recorder: opts.recorder,
            system_hash: None,
            decider: opts.executor.filter(|_| self.config.executor),
            exec_streak: 0,
            exec_cooldown: 0,
            memory: opts.memory,
            mem: Default::default(),
            human: Default::default(),
            ledger: Default::default(),
            probe: opts.probe,
            events_mark: None,
        };
        self.executor_start(&mut r).await;
        let outcome = self.drive(&mut r, opts).await;
        self.finish_ledger(&mut r);
        r.trace.finish(outcome);
        if let Some(store) = opts.memory
            && !opts.learn_later
        {
            self.learn_from(store, &r.trace).await;
        }
        if let Some(rec) = opts.recorder
            && let Err(e) = rec.finish(&r.trace)
        {
            log::warn!("recording the run failed: {e}");
        }
        self.sink.emit(&EngineEvent::Finished { outcome });
        log::info!("run finished: {outcome:?}, {} steps", r.trace.steps.len());
        if outcome == Outcome::Success
            && let Some(cache) = opts.plan_cache
            && let Err(e) = cache.record(&r.trace)
        {
            log::warn!("plan cache record failed: {e}");
        }
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

        // A skill's deterministic steps, or a cached plan's prefix, run first
        // with no model call.
        let prefix = match active_skill {
            Some(sk) if !sk.steps.is_empty() => Some((
                sk.steps.iter().map(act::fixed_decision).collect::<Vec<_>>(),
                SKILL,
            )),
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
        let first = self.outcome_request(r, &mut notes, schema);
        let lessons = self.lessons_for(r, &obs.grounding);
        let user_text = prompt::build_turn(&TurnInput {
            lessons: lessons.as_deref(),
            task: r.task,
            grounding: &obs.grounding,
            history: &r.history,
            active_skill,
            no_progress: r.no_progress_streak >= 1,
            notes: &notes,
            has_screenshot: !obs.images.screenshot_jpeg.is_empty(),
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
            match self.approver.approve(&decision, &obs.grounding).await {
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
        let guarded = act::reopen_guard(&mut r.opened, &decision);
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
        let mut item = HistoryItem::from_decision(&decision);
        let acted_ok = path != "error";
        if let Some(out) = &output {
            item.push_output(out);
            // Terminal work changes no pixels; it is progress by itself.
            r.no_progress_streak = 0;
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

        if r.no_progress_streak >= 2 {
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
