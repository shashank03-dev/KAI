//! Per-step timings and the full run trace. Stored locally only; nothing leaves
//! the machine except the model call itself.

use crate::client::Backend;
use crate::schema::Decision;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Where the time went in one step. Shown in the UI and saved in the trace.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Timings {
    pub capture_ms: u64,
    pub tree_ms: u64,
    pub encode_ms: u64,
    pub model_ms: u64,
    pub first_token_ms: Option<u64>,
    pub act_ms: u64,
    /// Time spent in the cheap text-only verifier, if one ran.
    #[serde(default)]
    pub verify_ms: u64,
}

impl Timings {
    pub fn total_ms(&self) -> u64 {
        // Capture and the tree read run in parallel; encoding follows them.
        let observe = self.capture_ms.max(self.tree_ms) + self.encode_ms;
        observe + self.model_ms + self.act_ms + self.verify_ms
    }

    pub fn set_model(&mut self, model: Duration, first_token: Option<Duration>) {
        self.model_ms = model.as_millis() as u64;
        self.first_token_ms = first_token.map(|d| d.as_millis() as u64);
    }
}

/// One recorded step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub index: u32,
    pub decision: Decision,
    pub timings: Timings,
    pub backend: String,
    /// Element list text shown to the model this step.
    pub elements: String,
    /// Path (relative to the trace dir) of the screenshot for this step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    /// Whether this step paused for approval, and the outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<String>,
    pub verified: bool,
    /// Terminal output of a `shell` step (truncated), for sub-agent reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// How the action reached the OS: `a11y` (accessibility pattern, no
    /// mouse), `mouse`, `keyboard`, `os` (open_app/open_url), `shell`, `wait`,
    /// or `none` (done/ask_user/skipped). Absent in old traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The model call behind this step, filled only when the run is recorded
    /// as training data (see `docs/DATASET.md`). Empty for steps with no model
    /// call (batched, cached, skill). Flattened: the fields sit on the step.
    #[serde(flatten, default)]
    pub call: CallRecord,
    /// What the executor's decision model said before it ran this step
    /// (`docs/EXECUTOR.md`). Absent on model, batched, cached and skill steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<ExecutorRecord>,
    /// What this action did, judged with no model call from the tree diff,
    /// accessibility events and the screen hash (`docs/VERIFICATION.md`).
    /// Absent in old traces and on steps that end the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<crate::ledger::Effect>,
    /// The element the step acted on, by role and name, so a procedure can
    /// find it again on a later screen (`procedure.rs`). Absent for untargeted
    /// steps, grid cells, unnamed and secure elements, and in old traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetSig>,
    /// A coarse signature of the screen the step was decided on
    /// ([`crate::memory::scope::screen_sig`]), so lessons learned here are
    /// recalled here. Absent without a window title and in old traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
}

/// An element by what it is, not where it was: mark numbers change every
/// frame, role and name usually do not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetSig {
    pub role: crate::element::Role,
    pub name: String,
}

impl TargetSig {
    /// The signature of `d`'s target on `g`, if it has a usable one.
    pub fn of(d: &Decision, g: &crate::element::Grounding) -> Option<Self> {
        let crate::element::Grounding::Marks(_) = g else {
            return None;
        };
        let e = g.element(d.target?)?;
        let name = e.name.trim();
        (!name.is_empty() && !e.secure).then(|| Self {
            role: e.role,
            name: name.to_string(),
        })
    }
}

/// The decision-model numbers behind one executor step.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExecutorRecord {
    /// The planned subgoal, in words.
    pub subgoal: String,
    /// Confidence of the chosen target (target-needing subgoals only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_p: Option<f64>,
    /// Lead of the chosen target over the runner-up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_done: Option<f64>,
    /// P(expect met) after acting, from the next decision request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect_met: Option<f64>,
    /// Wall time of the decision request that chose this step.
    pub latency_ms: u64,
}

/// Executor counters for one run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutorStats {
    /// The executor was asked for (config) and its endpoint answered.
    pub enabled: bool,
    /// Why it ran without the executor (health check failed), if it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
    /// `/health` round trip at the start of the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_ms: Option<u64>,
    /// Plans the model handed over.
    pub plans: u32,
    /// Decision requests sent.
    pub decisions: u32,
    /// Subgoals carried out (actions taken).
    pub acted: u32,
    /// Subgoals skipped because they already held.
    pub skipped: u32,
    /// Times control went back to the model early (unsure, failed, steered).
    pub escalations: u32,
    /// Wall time of every decision request, in order.
    pub latencies_ms: Vec<u64>,
}

/// What exactly went to and came back from the model on one step. Every field
/// is optional and omitted when empty, so old traces read unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CallRecord {
    /// Hex FNV-1a of the system prompt (its text is in the run's `system.txt`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_system_hash: Option<String>,
    /// The exact user turn sent this step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_user: Option<String>,
    /// The exact raw reply text, before parsing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_raw: Option<String>,
    /// Model id that answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    /// `frames/step-NNN-crop.jpg` when a zoom crop was sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<String>,
}

impl StepRecord {
    /// A record with the common fields; the rest start empty/verified.
    pub fn new(index: u32, decision: &Decision, timings: Timings, backend: &str) -> Self {
        Self {
            index,
            decision: decision.clone(),
            timings,
            backend: backend.to_string(),
            elements: String::new(),
            screenshot: None,
            approval: None,
            verified: true,
            output: None,
            path: None,
            call: CallRecord::default(),
            executor: None,
            effect: None,
            target: None,
            screen: None,
        }
    }
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failed,
    Stopped,
    StepLimit,
    TimeLimit,
    AskedUser,
}

/// The full trace of a task run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub task: String,
    pub started: String,
    pub steps: Vec<StepRecord>,
    pub outcome: Option<Outcome>,
    pub total_ms: u64,
    /// Model-call and observation errors that produced no step, e.g.
    /// `malformed reply: ...` or `model error: ...`. Feeds the failure
    /// taxonomy ([`crate::failure`]). Absent in old traces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// Executor counters, when the run asked for the executor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<ExecutorStats>,
    /// Messages the user sent mid-run, in order (experience memory turns
    /// them into preferences; `docs/MEMORY.md`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steered: Vec<String>,
    /// Ids of the memory lessons shown to the model this run, for credit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lessons: Vec<String>,
    /// Every pause for the user (CAPTCHA, one-time code, sign-in approval,
    /// question): kind, evidence, result and wait. Never the answer or the
    /// code. Absent in old traces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub human: Vec<crate::human::HumanRecord>,
    /// The required outcomes the model named at the first call, and how
    /// each ended (`docs/VERIFICATION.md`). Absent when the ledger is off
    /// or empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outcomes: Vec<crate::ledger::OutcomeRecord>,
    /// Every `done` the gate refused, and a waiver by the user.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate: Vec<crate::ledger::GateRecord>,
    /// The plans sampled before the first action and the one followed
    /// (`Config::best_of`, `crate::best_of`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_of: Option<crate::best_of::BestOf>,
    /// Diagnosed recoveries (`Config::recover`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recoveries: Vec<crate::engine::RecoveryRecord>,
}

impl Trace {
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            started: jiff::Zoned::now().to_string(),
            steps: Vec::new(),
            outcome: None,
            total_ms: 0,
            errors: Vec::new(),
            executor: None,
            steered: Vec::new(),
            lessons: Vec::new(),
            human: Vec::new(),
            outcomes: Vec::new(),
            gate: Vec::new(),
            best_of: None,
            recoveries: Vec::new(),
        }
    }

    pub fn push(&mut self, mut step: StepRecord) {
        self.total_ms += step.timings.total_ms();
        // Sensitive data Kai typed never reaches the saved trace.
        if let Some(kind) = crate::risk::sensitive_typed(&step.decision) {
            step.decision.text = Some(format!("[{kind}]"));
        }
        self.steps.push(step);
    }

    /// Median and p95 of per-step wall time, in ms.
    pub fn step_percentiles(&self) -> (u64, u64) {
        let mut v: Vec<u64> = self.steps.iter().map(|s| s.timings.total_ms()).collect();
        if v.is_empty() {
            return (0, 0);
        }
        v.sort_unstable();
        (percentile(&v, 0.50), percentile(&v, 0.95))
    }

    pub fn finish(&mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Backend label for a trace record.
pub fn backend_label(b: Backend) -> String {
    b.label().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_and_tree_overlap_then_encode() {
        let t = Timings {
            capture_ms: 30,
            tree_ms: 50,
            encode_ms: 20,
            model_ms: 400,
            first_token_ms: Some(120),
            act_ms: 10,
            verify_ms: 0,
        };
        // max(30,50) + 20 + 400 + 10 = 480
        assert_eq!(t.total_ms(), 480);
    }

    #[test]
    fn percentiles_over_steps() {
        let mut tr = Trace::new("x");
        for ms in [100u64, 200, 300, 400, 500] {
            tr.push(StepRecord {
                index: 0,
                decision: Decision {
                    thought: String::new(),
                    action: crate::schema::Action::Wait,
                    target: None,
                    text: None,
                    keys: vec![],
                    direction: None,
                    amount: None,
                    risk: crate::schema::Risk::None,
                    expect: String::new(),
                },
                timings: Timings {
                    model_ms: ms,
                    ..Default::default()
                },
                backend: "Mock".into(),
                elements: String::new(),
                screenshot: None,
                approval: None,
                verified: true,
                output: None,
                path: None,
                call: Default::default(),
                executor: None,
                effect: None,
                target: None,
                screen: None,
            });
        }
        let (p50, p95) = tr.step_percentiles();
        assert_eq!(p50, 300);
        assert_eq!(p95, 500);
    }
}
