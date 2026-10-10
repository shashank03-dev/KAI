//! `kai data stats`: what a dataset root holds.

use super::export::{load, run_failure};
use kai_core::failure;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct DataStats {
    pub runs: usize,
    pub passed: usize,
    pub success_rate: f64,
    pub steps: usize,
    /// Steps that carry a recorded model call (exportable rows).
    pub model_calls: usize,
    pub avg_steps: f64,
    /// Runs per failure category.
    pub failures: BTreeMap<String, usize>,
    /// Runs per source (`bench`, `run`, `app`).
    pub sources: BTreeMap<String, usize>,
    /// Runs per `backend · model`.
    pub models: BTreeMap<String, usize>,
}

pub fn stats(root: &Path) -> anyhow::Result<DataStats> {
    let runs = load(root)?;
    let mut s = DataStats {
        runs: runs.len(),
        ..Default::default()
    };
    for r in &runs {
        s.steps += r.trace.steps.len();
        s.model_calls += r
            .trace
            .steps
            .iter()
            .filter(|st| st.call.reply_raw.is_some())
            .count();
        if failure::passed(&r.trace, r.meta.check) {
            s.passed += 1;
        } else if let Some(f) = run_failure(r) {
            *s.failures.entry(f.category.to_string()).or_insert(0) += 1;
        }
        *s.sources.entry(r.meta.source.clone()).or_insert(0) += 1;
        *s.models
            .entry(format!("{} · {}", r.meta.backend, r.meta.model))
            .or_insert(0) += 1;
    }
    if s.runs > 0 {
        s.success_rate = round3(s.passed as f64 / s.runs as f64);
        s.avg_steps = round3(s.steps as f64 / s.runs as f64);
    }
    Ok(s)
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn hist(m: &BTreeMap<String, usize>) -> String {
    if m.is_empty() {
        return "  (none)\n".into();
    }
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut out = String::new();
    for (k, n) in v {
        let _ = writeln!(out, "  {k:<28} {n}");
    }
    out
}

pub fn text(s: &DataStats) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "runs {} · passed {} ({:.1}%) · steps {} (avg {:.2}/run) · model calls {}",
        s.runs,
        s.passed,
        s.success_rate * 100.0,
        s.steps,
        s.avg_steps,
        s.model_calls
    );
    let _ = write!(out, "failure categories:\n{}", hist(&s.failures));
    let _ = write!(out, "sources:\n{}", hist(&s.sources));
    let _ = write!(out, "backend · model:\n{}", hist(&s.models));
    out
}
