//! Turning runs into numbers: per-task results and the run summary. Only what
//! was measured is reported; anything the trace or backend did not give us is
//! listed in `Summary::missing` instead of being guessed.

use super::machine::{AbRecord, Machine};
use super::runner::{Call, Run};
use super::tasks::{Kind, ReplayMode};
use kai_core::trace::{Outcome, Timings};
use serde::{Deserialize, Serialize};

/// Nearest-rank percentile on an already-sorted slice (`p` in 0..=1).
pub fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p.clamp(0.0, 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// p50 and p95 of unsorted values.
pub fn p50_p95(mut v: Vec<u64>) -> (u64, u64) {
    v.sort_unstable();
    (percentile(&v, 0.5), percentile(&v, 0.95))
}

/// Observe time of a step: capture and tree read overlap, encode follows.
pub fn observe_ms(t: &Timings) -> u64 {
    t.capture_ms.max(t.tree_ms) + t.encode_ms
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// US$ per million tokens, given on the command line (never guessed).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Prices {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl Prices {
    pub fn cost(&self, prompt: u64, completion: u64) -> f64 {
        (prompt as f64 * self.input_per_mtok + completion as f64 * self.output_per_mtok) / 1e6
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskResult {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplayMode>,
    pub outcome: Option<Outcome>,
    pub pass: bool,
    /// How pass was decided: "check", "outcome" or "skipped".
    pub judged_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
    pub steps: usize,
    pub seconds: f64,
    pub step_p50_ms: u64,
    pub step_p95_ms: u64,
    pub model_p50_ms: u64,
    pub model_calls: usize,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub date: String,
    pub backend: String,
    /// Model id as passed to the backend ("recorded" for mock/replay).
    pub model: String,
    pub os: String,
    pub note: String,
    /// The exact command line that produced this file.
    pub command: String,
    pub machine: Option<Machine>,
    pub ab: Option<AbRecord>,
    pub prices: Option<Prices>,
    pub tasks: Vec<TaskResult>,
    pub success_rate: f64,
    pub passed: usize,
    /// Tasks that ran (skipped ones excluded).
    pub total: usize,
    pub skipped: usize,
    pub step_p50_ms: u64,
    pub step_p95_ms: u64,
    pub model_p50_ms: u64,
    pub model_p95_ms: u64,
    pub observe_p50_ms: u64,
    pub observe_p95_ms: u64,
    pub act_p50_ms: u64,
    pub act_p95_ms: u64,
    pub mean_seconds_per_task: f64,
    pub mean_steps_per_task: f64,
    pub mean_model_calls_per_task: f64,
    pub model_calls: usize,
    /// Calls whose reply carried token usage.
    pub calls_with_usage: usize,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub total_seconds: f64,
    /// What could not be measured on this run, in words.
    pub missing: Vec<String>,
}

fn token_sums(calls: &[Call]) -> (Option<u64>, Option<u64>, usize) {
    let with: Vec<&Call> = calls
        .iter()
        .filter(|c| c.prompt_tokens.is_some() || c.completion_tokens.is_some())
        .collect();
    if with.is_empty() {
        return (None, None, 0);
    }
    let p = with
        .iter()
        .map(|c| c.prompt_tokens.unwrap_or(0) as u64)
        .sum();
    let c = with
        .iter()
        .map(|c| c.completion_tokens.unwrap_or(0) as u64)
        .sum();
    (Some(p), Some(c), with.len())
}

/// Context for [`summarise`] that is not in the runs themselves.
pub struct Meta {
    pub backend: String,
    pub model: String,
    pub note: String,
    pub command: String,
    pub machine: Option<Machine>,
    pub ab: Option<AbRecord>,
    pub prices: Option<Prices>,
    /// Recorded replies (mock or replay): no model was called.
    pub recorded: bool,
    pub replay: bool,
    pub date: String,
}

pub fn summarise(meta: Meta, runs: Vec<Run>) -> Summary {
    let mut results = Vec::new();
    let (mut steps_all, mut model_all, mut observe_all, mut act_all) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut all_calls: Vec<Call> = Vec::new();
    let mut total_secs = 0.0;
    for mut run in runs {
        if meta.recorded {
            // The mock client makes up usage numbers; no model ran.
            for c in &mut run.calls {
                c.prompt_tokens = None;
                c.completion_tokens = None;
            }
        }
        let t = run.task;
        let mut r = TaskResult {
            id: t.id,
            title: t.title,
            kind: t.kind,
            replay: meta.replay.then_some(t.replay),
            ..Default::default()
        };
        let Some(trace) = run.trace else {
            r.judged_by = "skipped".into();
            r.skip_reason = run.skipped;
            results.push(r);
            continue;
        };
        let (pass, judged_by) = match run.check {
            Some(ok) => (ok, "check"),
            None => (trace.outcome == Some(Outcome::Success), "outcome"),
        };
        let tm: Vec<&Timings> = trace.steps.iter().map(|s| &s.timings).collect();
        let (p50, p95) = p50_p95(tm.iter().map(|t| t.total_ms()).collect());
        r.model_p50_ms = p50_p95(tm.iter().map(|t| t.model_ms).collect()).0;
        steps_all.extend(tm.iter().map(|t| t.total_ms()));
        model_all.extend(tm.iter().map(|t| t.model_ms));
        observe_all.extend(tm.iter().map(|t| observe_ms(t)));
        act_all.extend(tm.iter().map(|t| t.act_ms));
        let (pt, ct, _) = token_sums(&run.calls);
        r.cost_usd = match (meta.prices, pt, ct) {
            (Some(pr), Some(p), Some(c)) => Some(pr.cost(p, c)),
            _ => None,
        };
        r.prompt_tokens = pt;
        r.completion_tokens = ct;
        r.model_calls = run.calls.len();
        all_calls.extend(run.calls);
        r.outcome = trace.outcome;
        r.pass = pass;
        r.judged_by = judged_by.into();
        r.steps = trace.steps.len();
        r.seconds = round2(run.took.as_secs_f64());
        r.step_p50_ms = p50;
        r.step_p95_ms = p95;
        total_secs += run.took.as_secs_f64();
        results.push(r);
    }
    let ran: Vec<&TaskResult> = results
        .iter()
        .filter(|r| r.judged_by != "skipped")
        .collect();
    let n = ran.len();
    let passed = ran.iter().filter(|r| r.pass).count();
    let mean = |f: &dyn Fn(&TaskResult) -> f64| {
        if n == 0 {
            0.0
        } else {
            round2(ran.iter().map(|r| f(r)).sum::<f64>() / n as f64)
        }
    };
    let (prompt_tokens, completion_tokens, with_usage) = token_sums(&all_calls);
    let cost_usd = match (meta.prices, prompt_tokens, completion_tokens) {
        (Some(pr), Some(p), Some(c)) => Some(pr.cost(p, c)),
        _ => None,
    };
    let mut missing = Vec::new();
    if meta.recorded {
        missing.push("tokens and cost: none (recorded replies, no model was called)".into());
    } else if with_usage == 0 {
        missing.push("tokens: the backend reported no usage on any call".into());
    } else if with_usage < all_calls.len() {
        missing.push(format!(
            "tokens: only {with_usage} of {} calls reported usage; sums cover those",
            all_calls.len()
        ));
    }
    if !meta.recorded && meta.prices.is_none() {
        missing.push("cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)".into());
    }
    let (step_p50_ms, step_p95_ms) = p50_p95(steps_all);
    let (model_p50_ms, model_p95_ms) = p50_p95(model_all);
    let (observe_p50_ms, observe_p95_ms) = p50_p95(observe_all);
    let (act_p50_ms, act_p95_ms) = p50_p95(act_all);
    Summary {
        date: meta.date,
        backend: meta.backend,
        model: meta.model,
        os: std::env::consts::OS.into(),
        note: meta.note,
        command: meta.command,
        machine: meta.machine,
        ab: meta.ab,
        prices: meta.prices,
        success_rate: if n == 0 {
            0.0
        } else {
            passed as f64 / n as f64
        },
        passed,
        total: n,
        skipped: results.len() - n,
        step_p50_ms,
        step_p95_ms,
        model_p50_ms,
        model_p95_ms,
        observe_p50_ms,
        observe_p95_ms,
        act_p50_ms,
        act_p95_ms,
        mean_seconds_per_task: mean(&|r| r.seconds),
        mean_steps_per_task: mean(&|r| r.steps as f64),
        mean_model_calls_per_task: mean(&|r| r.model_calls as f64),
        model_calls: all_calls.len(),
        calls_with_usage: with_usage,
        prompt_tokens,
        completion_tokens,
        cost_usd,
        total_seconds: round2(total_secs),
        missing,
        tasks: results,
    }
}
