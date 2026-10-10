//! Failure taxonomy for recorded runs: every run that did not pass gets one
//! category, assigned by heuristics from the trace and (for bench runs) the
//! automatic check. Categories and their order are part of the dataset
//! contract (`docs/DATASET.md`); the first rule that matches wins.

use crate::schema::{Action, Decision};
use crate::trace::{Outcome, Trace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Why a run failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    StepLimit,
    TimeLimit,
    /// The model asked the user (or Kai stopped and asked).
    GaveUp,
    /// The model said done but the check failed.
    FalseDone,
    /// The same action on the same target three or more times.
    Loop,
    /// Two or more undecodable replies.
    Malformed,
    /// Setup failed, an action errored, or a shell command timed out.
    EnvError,
    RateLimited,
    /// Clicked a mark whose name does not match what the thought said.
    GroundingMiss,
    Other,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::StepLimit => "step_limit",
            Category::TimeLimit => "time_limit",
            Category::GaveUp => "gave_up",
            Category::FalseDone => "false_done",
            Category::Loop => "loop",
            Category::Malformed => "malformed",
            Category::EnvError => "env_error",
            Category::RateLimited => "rate_limited",
            Category::GroundingMiss => "grounding_miss",
            Category::Other => "other",
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A labelled failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub category: Category,
    pub detail: String,
}

impl Failure {
    pub fn new(category: Category, detail: impl Into<String>) -> Self {
        Self {
            category,
            detail: detail.into(),
        }
    }
}

/// Same action + target this many times counts as a loop.
pub const LOOP_REPEATS: usize = 3;
/// This many undecodable replies count as malformed output.
pub const MALFORMED_REPLIES: usize = 2;

/// Did the run pass? The check decides when there is one, else the outcome.
pub fn passed(trace: &Trace, check: Option<bool>) -> bool {
    check.unwrap_or(trace.outcome == Some(Outcome::Success))
}

/// Label a run. `None` when it passed.
pub fn classify(trace: &Trace, check: Option<bool>) -> Option<Failure> {
    if passed(trace, check) {
        return None;
    }
    let errors = |pat: &[&str]| -> Vec<&String> {
        trace
            .errors
            .iter()
            .filter(|e| {
                let l = e.to_lowercase();
                pat.iter().any(|p| l.contains(p))
            })
            .collect()
    };
    if let Some(e) = errors(&[
        "429",
        "rate limit",
        "rate-limit",
        "quota",
        "too many requests",
    ])
    .first()
    {
        return Some(Failure::new(Category::RateLimited, short(e)));
    }
    if trace.outcome == Some(Outcome::Success) && check == Some(false) {
        let last = trace.steps.last().map(|s| s.decision.thought.as_str());
        return Some(Failure::new(
            Category::FalseDone,
            format!(
                "said done ({}) but the check failed",
                short(last.unwrap_or(""))
            ),
        ));
    }
    let malformed: Vec<&String> = trace
        .errors
        .iter()
        .filter(|e| e.starts_with("malformed reply"))
        .collect();
    if malformed.len() >= MALFORMED_REPLIES {
        return Some(Failure::new(
            Category::Malformed,
            format!(
                "{} undecodable replies; last: {}",
                malformed.len(),
                short(malformed[malformed.len() - 1])
            ),
        ));
    }
    if let Some(d) = env_error(trace) {
        return Some(Failure::new(Category::EnvError, d));
    }
    if let Some(d) = repeated_action(trace) {
        return Some(Failure::new(Category::Loop, d));
    }
    let n = trace.steps.len();
    match trace.outcome {
        Some(Outcome::StepLimit) => {
            return Some(Failure::new(
                Category::StepLimit,
                format!("hit the step limit after {n} steps"),
            ));
        }
        Some(Outcome::TimeLimit) => {
            return Some(Failure::new(
                Category::TimeLimit,
                format!("hit the time limit after {n} steps"),
            ));
        }
        Some(Outcome::AskedUser) => {
            let why = match trace.steps.last() {
                Some(s) if s.decision.action == Action::AskUser => {
                    format!("asked the user: {}", short(&s.decision.thought))
                }
                _ => "Kai stopped and asked (no progress or repeated errors)".to_string(),
            };
            return Some(Failure::new(Category::GaveUp, why));
        }
        _ => {}
    }
    if let Some(d) = grounding_miss(trace) {
        return Some(Failure::new(Category::GroundingMiss, d));
    }
    let detail = match trace.outcome {
        Some(Outcome::Stopped) => "stopped by the user".to_string(),
        Some(Outcome::Failed) => "run failed".to_string(),
        Some(Outcome::Success) => "outcome success but not passed".to_string(),
        None => "no outcome recorded".to_string(),
        Some(o) => format!("{o:?}"),
    };
    Some(Failure::new(Category::Other, detail))
}

fn short(s: &str) -> String {
    let s = s.trim().replace('\n', " ");
    if s.chars().count() > 160 {
        format!("{}…", s.chars().take(160).collect::<String>())
    } else {
        s
    }
}

/// Act errors and shell timeouts recorded on steps.
fn env_error(trace: &Trace) -> Option<String> {
    for s in &trace.steps {
        let out = s.output.as_deref().unwrap_or("");
        if s.path.as_deref() == Some("error") || out.starts_with("ERROR:") {
            return Some(format!("step {}: {}", s.index, short(out)));
        }
        if s.decision.action == Action::Shell && out.to_lowercase().contains("timed out") {
            return Some(format!("step {}: shell timed out: {}", s.index, short(out)));
        }
    }
    trace
        .errors
        .iter()
        .find(|e| e.starts_with("observe"))
        .map(|e| short(e))
}

/// What makes two actions "the same" for loop detection.
fn action_key(d: &Decision) -> Option<String> {
    match d.action {
        // Waiting and scrolling repeat legitimately.
        Action::Wait | Action::Scroll | Action::Done | Action::AskUser => None,
        _ => Some(format!(
            "{:?}|{:?}|{}|{}",
            d.action,
            d.target,
            d.text.as_deref().unwrap_or(""),
            d.keys.join("+")
        )),
    }
}

fn repeated_action(trace: &Trace) -> Option<String> {
    let mut counts: HashMap<String, (usize, &Decision)> = HashMap::new();
    for s in &trace.steps {
        if let Some(k) = action_key(&s.decision) {
            counts.entry(k).or_insert((0, &s.decision)).0 += 1;
        }
    }
    let (n, d) = counts
        .into_values()
        .filter(|(n, _)| *n >= LOOP_REPEATS)
        .max_by_key(|(n, _)| *n)?;
    let target = d.target.map(|t| format!(" #{t}")).unwrap_or_default();
    let what = d
        .text
        .as_deref()
        .map(|t| format!(" {}", short(t)))
        .unwrap_or_default();
    Some(format!("{:?}{target}{what} repeated {n} times", d.action).to_lowercase())
}

/// The name of mark `t` in an element list line `#t role "name" (...)`.
fn mark_name(elements: &str, t: u32) -> Option<String> {
    let prefix = format!("#{t} ");
    let line = elements.lines().find(|l| l.starts_with(&prefix))?;
    let start = line.find('"')? + 1;
    let end = start + line[start..].find('"')?;
    Some(line[start..end].to_string())
}

/// Quoted phrases in a thought: what the model said it was clicking.
fn quoted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Double quotes only: single quotes are mostly apostrophes.
    for q in ['"', '“'] {
        let close = if q == '“' { '”' } else { q };
        let mut rest = s;
        while let Some(i) = rest.find(q) {
            let after = &rest[i + q.len_utf8()..];
            let Some(j) = after.find(close) else { break };
            let phrase = after[..j].trim();
            if phrase.chars().count() >= 2 {
                out.push(phrase.to_lowercase());
            }
            rest = &after[j + close.len_utf8()..];
        }
    }
    out
}

/// Best effort: an unverified click whose thought quotes a name that the
/// clicked mark does not carry.
fn grounding_miss(trace: &Trace) -> Option<String> {
    for s in &trace.steps {
        let d = &s.decision;
        if s.verified
            || !matches!(
                d.action,
                Action::Click | Action::DoubleClick | Action::RightClick
            )
        {
            continue;
        }
        let Some(t) = d.target else { continue };
        let Some(name) = mark_name(&s.elements, t) else {
            continue;
        };
        let name = name.to_lowercase();
        let wanted = quoted(&d.thought);
        if !wanted.is_empty()
            && !wanted
                .iter()
                .any(|w| name.contains(w.as_str()) || (!name.is_empty() && w.contains(&name)))
        {
            return Some(format!(
                "step {}: clicked #{t} \"{name}\" but meant \"{}\"",
                s.index, wanted[0]
            ));
        }
    }
    None
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod tests;
