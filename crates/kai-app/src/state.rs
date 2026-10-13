//! What the UI knows about the current run. Both the main window and the mini
//! window read one `Entity<RunState>`; the agent's messages update it.

use crate::agent::{HumanPending, Pending, Ui};
use crate::team::{OutputView, SteerView, Team};
use ely_gpui_component::chat::StepState;
use kai_core::engine::{ApprovalChoice, EngineEvent};
use kai_core::orchestrator::Plan;
use kai_core::trace::{Outcome, Timings};
use kai_core::{Action, Decision, Rect, Trace};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// Waiting for an earlier run to finish with the screen.
    Queued,
    Running,
    /// Waiting on the user: an approval or a human check.
    Waiting,
    Done,
}

/// One step as the step list shows it.
#[derive(Debug, Clone)]
pub struct StepView {
    pub index: u32,
    pub title: String,
    pub thought: String,
    pub state: StepState,
    pub timings: Option<Timings>,
    pub json: String,
    /// How the action was carried out (a11y, mouse, keyboard, os, shell…)
    /// and which backend decided it; known once the run's trace arrives.
    pub path: Option<String>,
    pub backend: Option<String>,
}

#[derive(Default)]
pub struct RunState {
    pub phase: Phase,
    pub task: String,
    pub team: bool,
    pub steps: Vec<StepView>,
    pub frames: Vec<PathBuf>,
    /// The frame the viewer shows; follows the newest unless the user scrubs.
    pub shown: Option<usize>,
    pub screen: Option<Rect>,
    /// Where the next action points, as a share of the frame.
    pub pointer: Option<(f32, f32)>,
    pub caption: String,
    pub thinking: bool,
    pub max_steps: u32,
    pub approval: Option<Pending>,
    /// Approvals asked while another was showing (Team: sub-agents run side
    /// by side). Dropping a pending approval answers Stop, so none is ever
    /// replaced; each shows in turn ([`RunState::take_approval`]).
    pub approvals_waiting: std::collections::VecDeque<Pending>,
    /// A CAPTCHA, one-time code, sign-in approval or question waiting on
    /// the user (`docs/HUMAN_CHECKS.md`).
    pub human: Option<HumanPending>,
    /// Checks asked while another was showing, as for approvals.
    pub humans_waiting: std::collections::VecDeque<HumanPending>,
    /// When the approval being shown appeared: an Approve sooner than
    /// [`APPROVE_GRACE`] after is ignored ([`RunState::answer_approval`]).
    pub approval_since: Option<Instant>,
    pub plan: Option<Plan>,
    pub report: Option<String>,
    pub trace: Option<Trace>,
    pub error: Option<String>,
    pub started: Option<Instant>,
    pub elapsed: Duration,
    pub last_model_ms: Option<u64>,
    /// Model time of every step that called a model, in order (ms).
    pub model_ms: Vec<u64>,
    /// "Gemma 4 26B A4B · Gateway": what this run was started with.
    pub model: String,
    /// The user asked to pause; `held` once the engine actually waits.
    pub paused: bool,
    pub held: bool,
    pub outputs: Vec<OutputView>,
    pub steers: Vec<SteerView>,
    pub team_view: Team,
    /// The outcome checklist (`docs/VERIFICATION.md`), and the last time
    /// Kai refused `done`.
    pub outcomes: Vec<kai_core::OutcomeRecord>,
    pub gated: Option<String>,
}

/// An Approve this soon after an approval appears was meant for the one
/// before it (a double click as queued Team approvals take turns), or for
/// whatever was under the pointer: nobody reads a step that fast.
pub const APPROVE_GRACE: Duration = Duration::from_millis(600);

impl RunState {
    /// The approval being shown, for answering; the next queued one (if
    /// any) takes its place.
    pub fn take_approval(&mut self) -> Option<Pending> {
        let p = self.approval.take();
        self.approval = self.approvals_waiting.pop_front();
        self.approval_since = self.approval.as_ref().map(|_| Instant::now());
        if self.approval.is_some() {
            self.phase = Phase::Waiting;
        }
        p
    }

    /// The user's answer to the approval being shown: the pending approval
    /// to send `choice` to, or `None` when there is none or an Approve came
    /// within [`APPROVE_GRACE`] of it appearing (Skip and Stop always go).
    pub fn answer_approval(&mut self, choice: ApprovalChoice) -> Option<Pending> {
        self.approval.as_ref()?;
        if choice == ApprovalChoice::Approve
            && self
                .approval_since
                .is_some_and(|t| t.elapsed() < APPROVE_GRACE)
        {
            log::info!("approve ignored: the approval appeared a moment ago");
            return None;
        }
        self.phase = Phase::Running;
        self.take_approval()
    }

    /// Every approval and check, shown and queued, for stopping the run.
    pub fn take_all(&mut self) -> (Vec<Pending>, Vec<HumanPending>) {
        let mut approvals: Vec<Pending> = self.approval.take().into_iter().collect();
        approvals.extend(self.approvals_waiting.drain(..));
        let mut humans: Vec<HumanPending> = self.human.take().into_iter().collect();
        humans.extend(self.humans_waiting.drain(..));
        self.approval_since = None;
        (approvals, humans)
    }

    /// Drop approvals and checks nobody waits for any more (that sub-agent
    /// timed out or stopped), showing the next live one.
    fn drop_abandoned(&mut self) {
        self.approvals_waiting.retain(|p| !p.reply.is_canceled());
        if self
            .approval
            .as_ref()
            .is_some_and(|p| p.reply.is_canceled())
        {
            self.take_approval();
        }
        self.humans_waiting.retain(|h| !h.reply.is_canceled());
        if self.human.as_ref().is_some_and(|h| h.reply.is_canceled()) {
            self.take_human();
        }
    }

    /// The check being shown, for answering; the next queued one takes its
    /// place.
    pub fn take_human(&mut self) -> Option<HumanPending> {
        let h = self.human.take();
        self.human = self.humans_waiting.pop_front();
        if self.human.is_some() {
            self.phase = Phase::Waiting;
        }
        h
    }

    pub fn start(&mut self, task: String, team: bool, max_steps: u32) {
        *self = Self {
            phase: Phase::Running,
            task,
            team,
            max_steps,
            started: Some(Instant::now()),
            ..Self::default()
        };
    }

    /// Waiting in the queue; `begin` starts the clock.
    pub fn queue(&mut self, task: String, team: bool, max_steps: u32, model: String) {
        *self = Self {
            phase: Phase::Queued,
            task,
            team,
            max_steps,
            model,
            ..Self::default()
        };
    }

    pub fn begin(&mut self) {
        self.phase = Phase::Running;
        self.started = Some(Instant::now());
    }

    /// Cancelled before it started.
    pub fn cancel(&mut self) {
        self.phase = Phase::Done;
        self.error = Some("Cancelled before it started.".into());
    }

    pub fn is_live(&self) -> bool {
        matches!(self.phase, Phase::Running | Phase::Waiting)
    }

    /// The user sent a message to the running agent.
    pub fn sent(&mut self, text: String) {
        self.steers.push(SteerView {
            text,
            sent_at: self.current_step(),
            read_at: None,
        });
    }

    pub fn current_step(&self) -> u32 {
        self.steps.len() as u32
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.trace.as_ref().and_then(|t| t.outcome)
    }

    /// Apply one message from the agent.
    pub fn apply(&mut self, msg: Ui) {
        // The clock stops when the run is done.
        if let Some(t) = self.started.filter(|_| self.phase != Phase::Done) {
            self.elapsed = t.elapsed();
        }
        match msg {
            Ui::Frame { path, .. } => {
                let follow = self.shown.is_none_or(|s| s + 1 == self.frames.len());
                self.frames.push(path);
                if follow {
                    self.shown = Some(self.frames.len() - 1);
                }
            }
            Ui::Event(ev) => self.event(ev),
            Ui::Approval(p) => {
                self.phase = Phase::Waiting;
                if self.approval.is_some() {
                    self.approvals_waiting.push_back(p);
                } else {
                    self.caption = format!(
                        "Waiting for you: {}",
                        describe(&p.decision, p.element.as_deref())
                    );
                    self.approval = Some(p);
                    self.approval_since = Some(Instant::now());
                }
            }
            Ui::Human(h) => {
                self.phase = Phase::Waiting;
                self.thinking = false;
                self.caption = format!("Waiting for you: {}", h.check.kind.title());
                if self.human.is_some() {
                    self.humans_waiting.push_back(h);
                } else {
                    self.human = Some(h);
                }
            }
            Ui::Plan(p) => {
                self.team_view = Team::from_plan(&p);
                self.plan = Some(p);
            }
            Ui::Report {
                summary,
                plan,
                results,
            } => {
                self.team_view.report(&plan, &results);
                self.plan = Some(plan);
                self.report = Some(summary);
            }
            Ui::Paused(held) => self.held = held && self.paused,
            Ui::Finished(trace) => {
                self.phase = Phase::Done;
                self.thinking = false;
                self.take_all();
                for s in &mut self.steps {
                    if s.state == StepState::Working {
                        s.state = StepState::Done;
                    }
                }
                if trace.outcome != Some(Outcome::Success)
                    && let Some(last) = self.steps.last_mut()
                {
                    last.state = StepState::Failed;
                }
                for rec in &trace.steps {
                    if let Some(v) = self.steps.iter_mut().find(|v| v.index == rec.index) {
                        v.path = rec.path.clone();
                        v.backend = Some(rec.backend.clone());
                    }
                }
                self.trace = Some(*trace);
                self.paused = false;
                self.held = false;
                self.team_view.settle();
            }
            Ui::Failed(e) => {
                self.phase = Phase::Done;
                self.thinking = false;
                self.error = Some(e);
                self.human = None;
                self.team_view.settle();
            }
        }
    }

    fn event(&mut self, ev: EngineEvent) {
        match ev {
            EngineEvent::Observed { screen, .. } => {
                self.screen = screen.or(self.screen);
            }
            EngineEvent::Thinking { .. } => {
                self.thinking = true;
                self.caption = "Gemma is deciding…".into();
            }
            EngineEvent::Decided {
                step,
                decision,
                target,
            } => {
                self.thinking = false;
                self.pointer = match (target, self.screen) {
                    (Some(t), Some(s)) if s.w > 0.0 && s.h > 0.0 => {
                        let (cx, cy) = t.center();
                        Some((
                            ((cx - s.x) / s.w).clamp(0.0, 1.0),
                            ((cy - s.y) / s.h).clamp(0.0, 1.0),
                        ))
                    }
                    _ => None,
                };
                self.caption = describe(&decision, None);
                for s in &mut self.steps {
                    if s.state == StepState::Working {
                        s.state = StepState::Done;
                    }
                }
                self.steps.push(StepView {
                    index: step,
                    title: describe(&decision, None),
                    thought: decision.thought.clone(),
                    state: StepState::Working,
                    timings: None,
                    json: serde_json::to_string_pretty(&*decision).unwrap_or_default(),
                    path: None,
                    backend: None,
                });
            }
            EngineEvent::AwaitingApproval { .. } => self.phase = Phase::Waiting,
            EngineEvent::Acted { step, timings } => {
                // Not `approval = None`: in Team mode this can be another
                // sub-agent acting, and dropping a pending approval stops
                // the one that asked (seen live: "You stopped Kai").
                self.drop_abandoned();
                if self.approval.is_none() && self.human.is_none() {
                    self.phase = Phase::Running;
                }
                self.last_model_ms = Some(timings.model_ms)
                    .filter(|ms| *ms > 0)
                    .or(self.last_model_ms);
                if timings.model_ms > 0 {
                    self.model_ms.push(timings.model_ms);
                }
                if let Some(s) = self.steps.iter_mut().rev().find(|s| s.index == step) {
                    s.timings = Some(timings);
                    s.state = StepState::Done;
                } else {
                    // A skill step: no decision event came first.
                    self.steps.push(StepView {
                        index: step,
                        title: "Skill step (no model call)".into(),
                        thought: String::new(),
                        state: StepState::Done,
                        timings: Some(timings),
                        json: String::new(),
                        path: None,
                        backend: None,
                    });
                }
            }
            EngineEvent::Finished { .. } => {
                self.thinking = false;
                if self.team {
                    self.team_view.one_finished();
                }
            }
            EngineEvent::Output {
                step,
                command,
                output,
            } => self.outputs.push(OutputView {
                step,
                command,
                output,
            }),
            EngineEvent::Outcomes { outcomes, .. } => self.outcomes = outcomes,
            EngineEvent::Gated { reason, .. } => {
                self.caption = reason.clone();
                self.gated = Some(reason);
            }
            EngineEvent::NeedsHuman { .. } => self.phase = Phase::Waiting,
            EngineEvent::HumanDone { result, .. } => {
                // Not `human = None`: in Team mode another sub-agent's check
                // may be showing, and dropping it answers Stop. The one that
                // is done was answered (taken) or timed out (abandoned).
                self.drop_abandoned();
                if self.approval.is_none() && self.human.is_none() {
                    self.phase = Phase::Running;
                }
                self.caption = format!("Back to work ({})", result.replace('_', " "));
            }
            EngineEvent::Steered { step, message } => {
                match self
                    .steers
                    .iter_mut()
                    .find(|s| s.read_at.is_none() && s.text == message)
                {
                    Some(s) => s.read_at = Some(step),
                    None => self.steers.push(SteerView {
                        text: message,
                        sent_at: step,
                        read_at: Some(step),
                    }),
                }
            }
        }
    }
}

/// A run's outcome in a few plain words, for lists, toasts and badges.
pub fn outcome_text(o: Outcome) -> &'static str {
    match o {
        Outcome::Success => "Done",
        Outcome::Failed => "Couldn't finish",
        Outcome::Stopped => "Stopped",
        Outcome::StepLimit => "Hit the step limit",
        Outcome::TimeLimit => "Hit the time limit",
        Outcome::AskedUser => "Needs your answer",
    }
}

/// An action in plain words: "Clicking “Save”", "Typing “hello”".
pub fn describe(d: &Decision, element: Option<&str>) -> String {
    let on = element
        .map(|e| format!(" “{e}”"))
        .unwrap_or_else(|| d.target.map(|t| format!(" #{t}")).unwrap_or_default());
    let text = d.text.as_deref().unwrap_or("");
    let short: String = text.chars().take(48).collect();
    match d.action {
        Action::Click => format!("Clicking{on}"),
        Action::DoubleClick => format!("Double-clicking{on}"),
        Action::RightClick => format!("Right-clicking{on}"),
        Action::Type => format!("Typing “{short}”"),
        Action::Key => format!("Pressing {}", d.keys.join("+")),
        Action::Scroll => "Scrolling".into(),
        Action::OpenApp => format!("Opening {short}"),
        Action::OpenUrl => format!("Opening {short}"),
        Action::Wait => "Waiting for the screen".into(),
        Action::AskUser => format!("Asking you: {short}"),
        Action::Done => "Finished".into(),
        Action::Shell => format!("Running `{short}`"),
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
