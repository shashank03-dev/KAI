//! The harness loop: observe → ground → decide → act → verify, one action per
//! step. Core stays OS-free: the screen and the mouse/keyboard are behind the
//! [`Perceptor`] and [`Actuator`] traits, which `kai-os` implements and tests
//! fake. See `docs/HARNESS.md`.

use crate::client::{Images, ModelClient, ModelRequest};
use crate::element::Grounding;
use crate::prompt::{self, HistoryItem};
use crate::risk::{self, ApprovalLevel};
use crate::schema::{Action, Decision};
use crate::skill::{DeterministicStep, Skill};
use crate::trace::{Outcome, StepRecord, Timings, Trace};
use async_trait::async_trait;
use std::time::{Duration, Instant};

/// What one observation produced.
pub struct Observation {
    pub images: Images,
    pub grounding: Grounding,
    /// Perceptual hash of the screen, to detect "nothing changed".
    pub screen_hash: u64,
    /// Where the observe time went (capture and tree run in parallel).
    pub timing: ObserveTiming,
    /// The captured display in global screen pixels, when known.
    pub screen: Option<crate::element::Rect>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObserveTiming {
    pub capture_ms: u64,
    pub tree_ms: u64,
    pub encode_ms: u64,
}

/// Looks at the screen and grounds it (Set-of-Marks or grid).
#[async_trait]
pub trait Perceptor: Send + Sync {
    async fn observe(&self) -> anyhow::Result<Observation>;
    /// Wait for the screen to settle after an action, up to a limit.
    async fn settle(&self, _max: Duration) {}
}

/// Moves the mouse/keyboard and reaches the OS for fast paths.
#[async_trait]
pub trait Actuator: Send + Sync {
    async fn act(&self, decision: &Decision, grounding: &Grounding) -> anyhow::Result<()>;
    async fn open_app(&self, app: &str) -> anyhow::Result<()>;
    async fn open_url(&self, url: &str) -> anyhow::Result<()>;
    /// Run a terminal command and return its (truncated) combined output.
    async fn shell(&self, cmd: &str) -> anyhow::Result<String>;
}

/// Approval UI (or an auto-policy in headless runs).
#[async_trait]
pub trait Approver: Send + Sync {
    /// Return true to proceed, false to skip/stop the action.
    async fn approve(&self, decision: &Decision, grounding: &Grounding) -> ApprovalChoice;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    Approve,
    Skip,
    Stop,
}

/// Lets the loop be stopped within one step (kill switch / interrupt button).
pub trait Interrupt: Send + Sync {
    fn stopped(&self) -> bool;
}

/// A no-op interrupt for headless runs.
pub struct NeverStops;
impl Interrupt for NeverStops {
    fn stopped(&self) -> bool {
        false
    }
}

/// Streams live state to the UI. No-op by default.
pub trait EventSink: Send + Sync {
    fn emit(&self, _ev: &EngineEvent) {}
}
pub struct NullSink;
impl EventSink for NullSink {}

/// Events the UI renders as they happen.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Observed {
        step: u32,
        elements: String,
        /// The marked JPEG the model sees (empty for terminal sub-agents).
        screenshot: std::sync::Arc<[u8]>,
        screen: Option<crate::element::Rect>,
    },
    Thinking {
        step: u32,
    },
    Decided {
        step: u32,
        decision: Box<Decision>,
        /// The box the action points at, in global screen pixels.
        target: Option<crate::element::Rect>,
    },
    AwaitingApproval {
        step: u32,
    },
    Acted {
        step: u32,
        timings: Timings,
    },
    Finished {
        outcome: Outcome,
    },
}

/// Run limits and policy.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub max_steps: u32,
    pub max_time: Duration,
    pub approval: ApprovalLevel,
    pub max_tokens: u32,
    /// Stop and ask after this many no-progress / failed steps in a row.
    pub max_failures: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_steps: 40,
            max_time: Duration::from_secs(600),
            approval: ApprovalLevel::HighRiskOnly,
            max_tokens: 192,
            max_failures: 3,
        }
    }
}

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
    pub skills: &'a [Skill],
    pub config: Config,
}

impl Engine<'_> {
    /// Run a task to completion, returning the trace.
    pub async fn run(&self, task: &str) -> Trace {
        let mut trace = Trace::new(task);
        let system = prompt::system_prompt(self.skills);
        let active_skill = prompt::select_skill(task, self.skills);
        let schema = Decision::json_schema();
        let deadline = Instant::now() + self.config.max_time;

        let mut history: Vec<HistoryItem> = Vec::new();
        let mut last_hash: Option<u64> = None;
        let mut no_progress_streak = 0u32;
        let mut failure_streak = 0u32;

        // A skill's deterministic steps run first, with no model call.
        let mut first_step = 0u32;
        if let Some(sk) = active_skill {
            for st in &sk.steps {
                if self.interrupt.stopped() {
                    return finish(&mut trace, self.sink, Outcome::Stopped);
                }
                let decision = fixed_decision(st);
                let t = Instant::now();
                if let Err(e) = self.act(&decision, &Grounding::Marks(Vec::new())).await {
                    log::warn!("skill step {st:?} failed: {e}; handing over to the model");
                    break;
                }
                self.perceptor.settle(Duration::from_secs(3)).await;
                let timings = Timings {
                    act_ms: t.elapsed().as_millis() as u64,
                    ..Default::default()
                };
                self.sink.emit(&EngineEvent::Acted {
                    step: first_step,
                    timings,
                });
                trace.push(StepRecord {
                    index: first_step,
                    decision: decision.clone(),
                    timings,
                    backend: "skill (no model call)".to_string(),
                    elements: String::new(),
                    screenshot: None,
                    approval: None,
                    verified: true,
                    output: None,
                });
                history.push(HistoryItem::from_decision(&decision));
                first_step += 1;
            }
        }

        for step in first_step..self.config.max_steps {
            if self.interrupt.stopped() {
                return finish(&mut trace, self.sink, Outcome::Stopped);
            }
            if Instant::now() >= deadline {
                return finish(&mut trace, self.sink, Outcome::TimeLimit);
            }

            // ---- Observe + Ground ----
            let t_obs = Instant::now();
            let obs = match self.perceptor.observe().await {
                Ok(o) => o,
                Err(e) => {
                    log::error!("observe failed: {e}");
                    failure_streak += 1;
                    if failure_streak >= self.config.max_failures {
                        return finish(&mut trace, self.sink, Outcome::Failed);
                    }
                    continue;
                }
            };
            let observe_ms = t_obs.elapsed().as_millis() as u64;
            let elements = obs.grounding.as_text();
            self.sink.emit(&EngineEvent::Observed {
                step,
                elements: elements.clone(),
                screenshot: obs.images.screenshot_jpeg.as_slice().into(),
                screen: obs.screen,
            });

            // No-progress detection: same screen two steps running.
            let no_progress = last_hash == Some(obs.screen_hash);
            if no_progress {
                no_progress_streak += 1;
            } else {
                no_progress_streak = 0;
            }

            // ---- Decide ----
            self.sink.emit(&EngineEvent::Thinking { step });
            let user_text = prompt::user_turn(
                task,
                &obs.grounding,
                &history,
                active_skill,
                no_progress_streak >= 1,
            );
            let escalate = failure_streak > 0 || no_progress_streak > 0;
            let client = match (escalate, self.careful) {
                (true, Some(c)) => c,
                _ => self.client,
            };
            let (decision, model_time, first_token) = match self
                .decide(client, &system, &user_text, &obs.images, &schema)
                .await
            {
                Ok(d) => d,
                Err(e) => {
                    log::warn!("undecodable reply at step {step}: {e}");
                    failure_streak += 1;
                    if failure_streak >= self.config.max_failures {
                        return finish(&mut trace, self.sink, Outcome::AskedUser);
                    }
                    continue;
                }
            };
            failure_streak = 0;

            let mut timings =
                if obs.timing.capture_ms + obs.timing.tree_ms + obs.timing.encode_ms > 0 {
                    Timings {
                        capture_ms: obs.timing.capture_ms,
                        tree_ms: obs.timing.tree_ms,
                        encode_ms: obs.timing.encode_ms,
                        ..Default::default()
                    }
                } else {
                    Timings {
                        capture_ms: observe_ms,
                        ..Default::default()
                    }
                };
            timings.set_model(model_time, first_token);

            self.sink.emit(&EngineEvent::Decided {
                step,
                decision: Box::new(decision.clone()),
                target: decision.target.and_then(|t| obs.grounding.target_rect(t)),
            });

            // ---- done / ask_user terminate ----
            match decision.action {
                Action::Done => {
                    record(
                        &mut trace,
                        step,
                        &decision,
                        timings,
                        self.client,
                        &elements,
                        true,
                    );
                    return finish(&mut trace, self.sink, Outcome::Success);
                }
                Action::AskUser => {
                    record(
                        &mut trace,
                        step,
                        &decision,
                        timings,
                        self.client,
                        &elements,
                        true,
                    );
                    return finish(&mut trace, self.sink, Outcome::AskedUser);
                }
                _ => {}
            }

            // ---- Approval for high-risk ----
            let final_risk = risk::classify(&decision, &obs.grounding);
            let pw = risk::is_payment_or_password(&decision, &obs.grounding);
            let mut approval_note = None;
            if risk::needs_approval(self.config.approval, final_risk, pw) {
                self.sink.emit(&EngineEvent::AwaitingApproval { step });
                match self.approver.approve(&decision, &obs.grounding).await {
                    ApprovalChoice::Approve => approval_note = Some("approved".to_string()),
                    ApprovalChoice::Skip => {
                        approval_note = Some("skipped".to_string());
                        push_record(
                            &mut trace,
                            step,
                            &decision,
                            timings,
                            self.client,
                            &elements,
                            false,
                            approval_note,
                        );
                        history.push(HistoryItem {
                            summary: format!("skipped {:?}", decision.action),
                        });
                        last_hash = Some(obs.screen_hash);
                        continue;
                    }
                    ApprovalChoice::Stop => {
                        return finish(&mut trace, self.sink, Outcome::Stopped);
                    }
                }
            }

            // ---- Act ----
            let t_act = Instant::now();
            let act_result = self.act(&decision, &obs.grounding).await;
            timings.act_ms = t_act.elapsed().as_millis() as u64;
            let output = match act_result {
                Ok(o) => o,
                Err(e) => {
                    log::error!("act failed at step {step}: {e}");
                    failure_streak += 1;
                    Some(format!("ERROR: {e}"))
                }
            };
            self.sink.emit(&EngineEvent::Acted { step, timings });

            // ---- Verify (next observation checks `expect`) ----
            self.perceptor.settle(Duration::from_secs(3)).await;

            push_record(
                &mut trace,
                step,
                &decision,
                timings,
                self.client,
                &elements,
                true,
                approval_note,
            );
            if let Some(last) = trace.steps.last_mut() {
                last.output = output.clone();
            }
            let mut item = HistoryItem::from_decision(&decision);
            if let Some(out) = output {
                item.push_output(&out);
                // Terminal work changes no pixels; it is progress by itself.
                no_progress_streak = 0;
            }
            if decision.action == Action::Wait {
                // Waiting is deliberate; an unchanged screen is expected.
                no_progress_streak = 0;
            }
            history.push(item);
            last_hash = Some(obs.screen_hash);

            if no_progress_streak >= 2 {
                // Two steps with no change despite acting: stop and ask.
                return finish(&mut trace, self.sink, Outcome::AskedUser);
            }
        }

        finish(&mut trace, self.sink, Outcome::StepLimit)
    }

    /// Returns the decision plus the model wall time and time-to-first-token.
    async fn decide(
        &self,
        client: &dyn ModelClient,
        system: &str,
        user_text: &str,
        images: &Images,
        schema: &serde_json::Value,
    ) -> anyhow::Result<(Decision, Duration, Option<Duration>)> {
        let req = ModelRequest {
            system,
            user_text,
            images,
            schema,
            max_tokens: self.config.max_tokens,
            temperature: 0.0,
        };
        let reply = client.decide(&req).await?;

        match Decision::parse_checked(&reply.raw) {
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
                    system,
                    user_text: &repair_text,
                    images,
                    schema,
                    max_tokens: self.config.max_tokens,
                    temperature: 0.0,
                };
                let reply2 = client.decide(&req2).await?;
                let total = reply.total + reply2.total;
                Ok((
                    Decision::parse_checked(&reply2.raw)?,
                    total,
                    reply2.first_token,
                ))
            }
        }
    }

    /// Returns command output for `shell`, so the model sees it next step.
    async fn act(
        &self,
        decision: &Decision,
        grounding: &Grounding,
    ) -> anyhow::Result<Option<String>> {
        let text = decision.text.as_deref().unwrap_or("");
        // Fast paths reach the OS directly, no grounding needed.
        match decision.action {
            Action::OpenApp => self.actuator.open_app(text).await.map(|_| None),
            Action::OpenUrl => self.actuator.open_url(text).await.map(|_| None),
            Action::Shell => self.actuator.shell(text).await.map(Some),
            Action::Wait => {
                self.perceptor.settle(Duration::from_secs(2)).await;
                Ok(None)
            }
            _ => self.actuator.act(decision, grounding).await.map(|_| None),
        }
    }
}

/// The decision a deterministic skill step stands for.
fn fixed_decision(st: &DeterministicStep) -> Decision {
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

fn record(
    trace: &mut Trace,
    step: u32,
    decision: &Decision,
    timings: Timings,
    client: &dyn ModelClient,
    elements: &str,
    verified: bool,
) {
    push_record(
        trace, step, decision, timings, client, elements, verified, None,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_record(
    trace: &mut Trace,
    step: u32,
    decision: &Decision,
    timings: Timings,
    client: &dyn ModelClient,
    elements: &str,
    verified: bool,
    approval: Option<String>,
) {
    trace.push(StepRecord {
        index: step,
        decision: decision.clone(),
        timings,
        backend: client.backend().label().to_string(),
        elements: elements.to_string(),
        screenshot: None,
        approval,
        verified,
        output: None,
    });
}

fn finish(trace: &mut Trace, sink: &dyn EventSink, outcome: Outcome) -> Trace {
    trace.finish(outcome);
    sink.emit(&EngineEvent::Finished { outcome });
    trace.clone()
}
