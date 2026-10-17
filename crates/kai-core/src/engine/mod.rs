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
mod computer;
mod effect;
mod execute;
pub(crate) mod fresh;
mod gate;
mod human;
mod injection_monitor;
mod memory;
mod model_step;
mod native;
mod oai;
mod observe;
mod privacy;
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
    /// A grounding model for screens without marks on OpenAI's contract
    /// (`--grounder`): the planner describes the target, this finds it.
    pub grounder: Option<Grounder<'a>>,
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

/// A model that turns a description into a point on the screenshot, and
/// the coordinate frame it answers in.
#[derive(Clone, Copy)]
pub struct Grounder<'a> {
    pub client: &'a dyn ModelClient,
    pub frame: crate::oai_contract::CoordFrame,
}

/// Per-run extras that are not collaborators. `Default` is a plain run.
#[derive(Clone, Copy, Default)]
pub struct RunOptions<'a> {
    /// Separate, explicitly supplied screenshot injection classifier.
    /// `None` (default) leaves Phase 5b off with zero model calls.
    pub injection_monitor: Option<&'a dyn ModelClient>,
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
        self.ledger_seed(&mut r);
        self.executor_start(&mut r).await;
        let outcome = self.drive(&mut r, opts).await;
        for rec in &mut r.trace.steps {
            r.privacy.record(rec);
        }
        self.finish_ledger(&mut r);
        r.privacy.trace(&mut r.trace);
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
        let (mut system, mut schema) = if r.decider.is_some() {
            let s = prompt::system_prompt_with_plan(self.skills);
            (s, StepReply::json_schema_with_plan())
        } else {
            (prompt::system_prompt(self.skills), StepReply::json_schema())
        };
        // Eyes that can read pages (the browser lane) offer `read_page`.
        if self.perceptor.reads_pages() {
            system.push_str(crate::web::READ_RULE);
            schema = StepReply::with_read_page(&schema);
        }
        if self.perceptor.mac() {
            system.push_str(prompt::MAC_RULE);
        }
        if self.config.memo {
            system.push_str(crate::memo::RULE);
            schema = crate::memo::with_memo(&schema);
        }
        if self.config.computer_tools_v1 {
            schema = crate::computer::with_schema(&schema, &self.actuator.computer_capabilities());
            system.push_str("\nShared computer tools v1: use action=computer and put exactly one tool in computer. Use the current observation_id for every target. Point coordinates refer to the supplied image. Only advertised actions, buttons and scroll units are available. Never guess a missing target. Execution is not evidence of success.\n");
        }
        let deadline = Instant::now() + self.config.max_time;
        r.deadline = deadline;
        // Native/contract clients construct a different prompt per frame.
        // Record their actual prompt only after the model has answered.
        r.system_hash = if self.config.native_model.is_none() && self.config.oai_contract.is_none()
        {
            r.recorder.map(|rec| rec.record_system(&system))
        } else {
            None
        };

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
        let choice = tokio::select! {
            biased;
            () = crate::ports::wait_cancelled(self.interrupt) => ApprovalChoice::Stop,
            choice = self.approver.approve(d, g) => choice,
        };
        r.human.waited += t.elapsed();
        choice
    }
}
