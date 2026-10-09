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
}

impl Timings {
    pub fn total_ms(&self) -> u64 {
        // Capture and the tree read run in parallel; encoding follows them.
        let observe = self.capture_ms.max(self.tree_ms) + self.encode_ms;
        observe + self.model_ms + self.act_ms
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
}

impl Trace {
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            started: jiff::Zoned::now().to_string(),
            steps: Vec::new(),
            outcome: None,
            total_ms: 0,
        }
    }

    pub fn push(&mut self, step: StepRecord) {
        self.total_ms += step.timings.total_ms();
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
            });
        }
        let (p50, p95) = tr.step_percentiles();
        assert_eq!(p50, 300);
        assert_eq!(p95, 500);
    }
}
