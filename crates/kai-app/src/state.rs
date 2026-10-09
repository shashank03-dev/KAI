//! What the UI knows about the current run. Both the main window and the mini
//! window read one `Entity<RunState>`; the agent's messages update it.

use crate::agent::{Pending, Ui};
use ely_gpui_component::chat::StepState;
use kai_core::engine::EngineEvent;
use kai_core::orchestrator::Plan;
use kai_core::trace::{Outcome, Timings};
use kai_core::{Action, Decision, Rect, Trace};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Running,
    /// Waiting on the user: an approval.
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
    pub plan: Option<Plan>,
    pub report: Option<String>,
    pub trace: Option<Trace>,
    pub error: Option<String>,
    pub started: Option<Instant>,
    pub elapsed: Duration,
    pub last_model_ms: Option<u64>,
}

impl RunState {
    pub fn start(&mut self, task: String, team: bool, max_steps: u32) {
        *self = Self { phase: Phase::Running, task, team, max_steps, started: Some(Instant::now()), ..Self::default() };
    }

    pub fn current_step(&self) -> u32 {
        self.steps.len() as u32
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.trace.as_ref().and_then(|t| t.outcome)
    }

    /// Apply one message from the agent.
    pub fn apply(&mut self, msg: Ui) {
        if let Some(t) = self.started {
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
                self.caption = format!("Waiting for you: {}", describe(&p.decision, p.element.as_deref()));
                self.approval = Some(p);
            }
            Ui::Plan(p) => self.plan = Some(p),
            Ui::Report(r) => self.report = Some(r),
            Ui::Finished(trace) => {
                self.phase = Phase::Done;
                self.thinking = false;
                self.approval = None;
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
                self.trace = Some(*trace);
            }
            Ui::Failed(e) => {
                self.phase = Phase::Done;
                self.thinking = false;
                self.error = Some(e);
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
            EngineEvent::Decided { step, decision, target } => {
                self.thinking = false;
                self.pointer = match (target, self.screen) {
                    (Some(t), Some(s)) if s.w > 0.0 && s.h > 0.0 => {
                        let (cx, cy) = t.center();
                        Some((((cx - s.x) / s.w).clamp(0.0, 1.0), ((cy - s.y) / s.h).clamp(0.0, 1.0)))
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
                });
            }
            EngineEvent::AwaitingApproval { .. } => self.phase = Phase::Waiting,
            EngineEvent::Acted { step, timings } => {
                self.phase = Phase::Running;
                self.approval = None;
                self.last_model_ms = Some(timings.model_ms).filter(|ms| *ms > 0).or(self.last_model_ms);
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
                    });
                }
            }
            EngineEvent::Finished { .. } => self.thinking = false,
        }
    }
}

/// An action in plain words: "Clicking “Save”", "Typing “hello”".
pub fn describe(d: &Decision, element: Option<&str>) -> String {
    let on = element.map(|e| format!(" “{e}”")).unwrap_or_else(|| d.target.map(|t| format!(" #{t}")).unwrap_or_default());
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
mod tests {
    use super::*;
    use std::sync::Arc;

    fn decision(json: &str) -> Box<Decision> {
        Box::new(Decision::parse_checked(json).unwrap())
    }

    #[test]
    fn pointer_is_a_share_of_the_screen() {
        let mut s = RunState::default();
        s.start("t".into(), false, 40);
        s.apply(Ui::Event(EngineEvent::Observed { step: 0, elements: String::new(), screenshot: Arc::from(&[][..]), screen: Some(Rect::new(0.0, 0.0, 1000.0, 500.0)) }));
        s.apply(Ui::Event(EngineEvent::Decided {
            step: 0,
            decision: decision(r#"{"thought":"x","action":"click","target":1,"risk":"none"}"#),
            target: Some(Rect::new(240.0, 240.0, 20.0, 20.0)),
        }));
        assert_eq!(s.pointer, Some((0.25, 0.5)));
        assert_eq!(s.steps.len(), 1);
        assert_eq!(s.steps[0].state, StepState::Working);
    }

    #[test]
    fn frames_follow_the_newest_unless_scrubbed() {
        let mut s = RunState::default();
        s.apply(Ui::Frame { step: 0, path: "a".into() });
        s.apply(Ui::Frame { step: 1, path: "b".into() });
        assert_eq!(s.shown, Some(1));
        s.shown = Some(0);
        s.apply(Ui::Frame { step: 2, path: "c".into() });
        assert_eq!(s.shown, Some(0), "a scrubbed viewer stays put");
    }

    #[test]
    fn a_failed_run_marks_its_last_step() {
        let mut s = RunState::default();
        s.apply(Ui::Event(EngineEvent::Decided { step: 0, decision: decision(r#"{"thought":"x","action":"wait","risk":"none"}"#), target: None }));
        let mut t = Trace::new("x");
        t.finish(Outcome::Stopped);
        s.apply(Ui::Finished(Box::new(t)));
        assert_eq!(s.phase, Phase::Done);
        assert_eq!(s.steps[0].state, StepState::Failed);
    }

    #[test]
    fn describes_in_plain_words() {
        let d = decision(r#"{"thought":"x","action":"click","target":3,"risk":"none"}"#);
        assert_eq!(describe(&d, Some("Save")), "Clicking “Save”");
        assert_eq!(describe(&d, None), "Clicking #3");
    }
}
