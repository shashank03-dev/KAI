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
//!   and [`crate::plan_cache`].

mod act;
mod batch;
mod observe;

pub use crate::ports::*;

use crate::client::{Images, ModelClient, ModelRequest};
use crate::plan_cache::PlanCache;
use crate::prompt::{self, HistoryItem, TurnInput};
use crate::reply::StepReply;
use crate::risk;
use crate::schema::Action;
use crate::skill::Skill;
use crate::trace::{Outcome, StepRecord, Timings, Trace};
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
    pub skills: &'a [Skill],
    pub config: Config,
}

/// Per-run extras that are not collaborators. `Default` is a plain run.
#[derive(Clone, Copy, Default)]
pub struct RunOptions<'a> {
    /// Replay a cached deterministic prefix for a known task, and record the
    /// prefix of this run if it succeeds.
    pub plan_cache: Option<&'a PlanCache>,
}

/// Trace backend label for actions run from a model reply's `then` list.
pub const BATCHED: &str = "batched (no model call)";
/// Trace backend label for skill steps.
pub const SKILL: &str = "skill (no model call)";
/// Trace backend label for plan-cache steps.
pub const CACHED: &str = "plan cache (no model call)";

/// What the last action expected, waiting for the next observation to check.
struct Pending {
    summary: String,
    action: Action,
    expect: String,
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
            notes: Vec::new(),
            grid_click: None,
        };
        let outcome = self.drive(&mut r, opts).await;
        r.trace.finish(outcome);
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
        let system = prompt::system_prompt(self.skills);
        let active_skill = prompt::select_skill(r.task, self.skills);
        let schema = StepReply::json_schema();
        let deadline = Instant::now() + self.config.max_time;

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
            if Instant::now() >= deadline {
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

        // ---- Observe + Ground (+ cheap verify of the last expect) ----
        let seen = match self.observe_step(r).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("observe failed: {e}");
                r.step += 1;
                r.failure_streak += 1;
                return (r.failure_streak >= self.config.max_failures).then_some(Outcome::Failed);
            }
        };
        let obs = &seen.obs;
        let elements = obs.grounding.as_text();

        // ---- Decide ----
        self.sink.emit(&EngineEvent::Thinking { step });
        let notes = std::mem::take(&mut r.notes);
        let user_text = prompt::build_turn(&TurnInput {
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
        let client = match (escalate, self.careful) {
            (true, Some(c)) => c,
            _ => self.client,
        };
        let decided = self
            .decide(client, system, &user_text, &obs.images, schema)
            .await;
        r.step += 1;
        let (reply, model_time, first_token) = match decided {
            Ok(d) => d,
            Err(e) => {
                log::warn!("undecodable reply at step {step}: {e}");
                r.failure_streak += 1;
                r.force_full = true;
                return (r.failure_streak >= self.config.max_failures)
                    .then_some(Outcome::AskedUser);
            }
        };
        r.failure_streak = 0;
        if reply.need_screen {
            r.force_full = true;
        }
        let decision = reply.decision;

        let mut timings = seen.timings;
        timings.set_model(model_time, first_token);
        let backend = client.backend().label();
        let mut rec = StepRecord::new(step, &decision, timings, backend);
        rec.elements = elements;

        self.sink.emit(&EngineEvent::Decided {
            step,
            decision: Box::new(decision.clone()),
            target: decision.target.and_then(|t| obs.grounding.target_rect(t)),
        });

        // ---- done / ask_user terminate ----
        match decision.action {
            Action::Done | Action::AskUser => {
                rec.path = Some("none".into());
                r.trace.push(rec);
                return Some(if decision.action == Action::Done {
                    Outcome::Success
                } else {
                    Outcome::AskedUser
                });
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
        let (output, path) = match self.act(&decision, &obs.grounding).await {
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
        None
    }

    /// Fold mid-run user messages into the next turn.
    fn drain_steering(&self, r: &mut Run<'_>, step: u32) -> bool {
        let msgs = self.steering.take();
        let any = !msgs.is_empty();
        for m in msgs {
            self.sink.emit(&EngineEvent::Steered {
                step,
                message: m.clone(),
            });
            r.notes.push(format!("User says (mid-run): {m}"));
        }
        if any {
            // The user may refer to what they see: look again properly.
            r.force_full = true;
        }
        any
    }

    fn emit_output(&self, step: u32, d: &crate::schema::Decision, output: Option<&str>) {
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

    /// Returns the reply plus the model wall time and time-to-first-token.
    async fn decide(
        &self,
        client: &dyn ModelClient,
        system: &str,
        user_text: &str,
        images: &Images,
        schema: &serde_json::Value,
    ) -> anyhow::Result<(StepReply, Duration, Option<Duration>)> {
        let req = ModelRequest {
            system,
            user_text,
            images,
            schema,
            max_tokens: self.config.max_tokens,
            temperature: 0.0,
        };
        let reply = client.decide(&req).await?;

        match StepReply::parse_checked(&reply.raw) {
            Ok(d) => Ok((d, reply.total, reply.first_token)),
            Err(first) => {
                // One repair retry (needed on the Gemini JSON-mode backend; a
                // no-op improvement on vLLM structured outputs).
                log::warn!("repairing malformed reply: {first}");
                let repair_text = format!(
                    "{user_text}\n\nYour previous reply was not valid: {first}. \
                     Reply with ONLY the JSON object."
                );
                let req2 = ModelRequest {
                    user_text: &repair_text,
                    ..req
                };
                let reply2 = client.decide(&req2).await?;
                let total = reply.total + reply2.total;
                Ok((
                    StepReply::parse_checked(&reply2.raw)?,
                    total,
                    reply2.first_token,
                ))
            }
        }
    }
}

/// Timings for a step whose observation took `observe_ms` in total.
fn observe_timings(t: ObserveTiming, observe_ms: u64) -> Timings {
    if t.capture_ms + t.tree_ms + t.encode_ms > 0 {
        Timings {
            capture_ms: t.capture_ms,
            tree_ms: t.tree_ms,
            encode_ms: t.encode_ms,
            ..Default::default()
        }
    } else {
        Timings {
            capture_ms: observe_ms,
            ..Default::default()
        }
    }
}
