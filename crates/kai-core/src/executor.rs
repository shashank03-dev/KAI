//! The executor: a fast local decision model (jevos-v4) that carries out the
//! routine subgoals of the model's `plan`. **Gemma decides what, jevos
//! decides where/whether, code acts, Kai's rules decide safety.**
//!
//! This module is the pure part: which elements may be the target, the one
//! request asked per subgoal, and the gates that turn the answers into act /
//! skip / escalate. The loop that drives it is `engine/execute.rs`. Approvals
//! never depend on anything here: every executor action goes through
//! [`crate::risk`] and the approver exactly like a model step. See
//! `docs/EXECUTOR.md`.

use crate::client::{Answer, ClientError, Question};
use crate::element::{Element, Grounding, Role};
use crate::plan::{Subgoal, SubgoalKind};
use crate::trace::ExecutorRecord;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::time::Duration;

/// Trace backend label for executor steps.
pub const BACKEND: &str = "executor (jevos)";
/// Question ids of the per-subgoal request.
pub const Q_TARGET: &str = "target";
pub const Q_DONE: &str = "already_done";
pub const Q_READY: &str = "ready";
pub const Q_PREV: &str = "expect_met";
/// The "no candidate fits" option.
pub const NONE: &str = "none";
/// Element text in the state is capped here (~3K tokens of jevos' 8,192).
const MAX_STATE_CHARS: usize = 12_000;

/// Answers typed questions about a text/JSON state. Implemented by
/// [`crate::client::DecisionClient`] (`DecisionClient::local` for jevos);
/// tests fake it.
#[async_trait]
pub trait Decider: Send + Sync {
    async fn ask(
        &self,
        state: &serde_json::Value,
        questions: &[(String, Question)],
    ) -> Result<BTreeMap<String, Answer>, ClientError>;

    /// Reachability check before a run; the round trip when reachable.
    async fn health(&self) -> Result<Duration, ClientError> {
        Ok(Duration::ZERO)
    }
}

/// Thresholds. Defaults are first guesses (no real jevos accuracy has been
/// measured yet) chosen to escalate rather than act when in doubt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gates {
    /// Act only if P(subgoal can be done now) is at least this.
    pub ready_min: f64,
    /// Act only if the chosen target's confidence is at least this.
    pub target_min: f64,
    /// ...and it leads the runner-up by at least this.
    pub margin_min: f64,
    /// Skip a subgoal whose `expect` already holds with at least this.
    pub skip_done: f64,
    /// After acting: at least this means the subgoal worked.
    pub met_ok: f64,
    /// After acting: at most this means it did not (escalate, screenshot).
    pub met_bad: f64,
    /// Candidate elements offered as target options (plus `none`).
    pub max_candidates: usize,
    /// Subgoals run per plan.
    pub max_subgoals: usize,
    /// After two escalations in a row, ignore plans for this many model steps.
    pub cooldown_steps: u32,
}

impl Default for Gates {
    fn default() -> Self {
        Self {
            ready_min: 0.7,
            target_min: 0.6,
            margin_min: 0.2,
            // Never skip on jevos' word alone: measured, it said p=0.97 that an
            // empty Email field already held the address. > 1 disables Skip;
            // the answer is still recorded for tuning.
            skip_done: 1.01,
            met_ok: 0.8,
            met_bad: 0.3,
            max_candidates: 12,
            max_subgoals: crate::plan::MAX_PLAN,
            cooldown_steps: 3,
        }
    }
}

/// Roles a subgoal kind may act on.
pub fn role_fits(kind: SubgoalKind, role: Role) -> bool {
    use Role::*;
    match kind {
        SubgoalKind::Type => matches!(role, TextField | PasswordField | ComboBox),
        SubgoalKind::Click => matches!(
            role,
            Button | Link | Checkbox | Radio | Tab | MenuItem | ListItem | ComboBox
        ),
        SubgoalKind::Select => matches!(role, ComboBox | ListItem | MenuItem | Radio),
        _ => false,
    }
}

fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(str::to_lowercase)
        .filter(|t| !matches!(t.as_str(), "the" | "field" | "button" | "box" | "link"))
        .collect()
}

/// Share of the target's words found in the element's name/value (prefix
/// matches count: "msg" ~ "message" does not, "mess" ~ "message" does).
fn overlap(want: &[String], e: &Element) -> f64 {
    if want.is_empty() {
        return 0.0;
    }
    let have = tokens(&format!("{} {}", e.name, e.value.as_deref().unwrap_or("")));
    let hit = want
        .iter()
        .filter(|w| {
            have.iter()
                .any(|h| h == *w || (w.len() >= 4 && h.starts_with(w.as_str())))
        })
        .count();
    hit as f64 / want.len() as f64
}

/// Enabled elements whose role fits the subgoal, ranked by word overlap with
/// its `target`, best first (ties keep screen order); at most `k`.
pub fn candidates<'g>(g: &'g Grounding, sg: &Subgoal, k: usize) -> Vec<&'g Element> {
    let Grounding::Marks(els) = g else {
        return Vec::new();
    };
    let want = tokens(&sg.target);
    let mut v: Vec<(f64, &Element)> = els
        .iter()
        .filter(|e| e.enabled && role_fits(sg.kind, e.role))
        .map(|e| (overlap(&want, e), e))
        .collect();
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    v.into_iter().take(k).map(|(_, e)| e).collect()
}

/// `#3 textfield "Email" = "ada@x.org"` (never a secure field's value).
pub fn label(e: &Element) -> String {
    let mut s = format!("#{} {} \"{}\"", e.mark, e.role.label(), e.name.trim());
    if let Some(v) = e.shown_value() {
        s.push_str(&format!(" = \"{v}\""));
    }
    if e.secure {
        s.push_str(" (secure)");
    }
    s
}

/// The request state: only the element list, with a one-line legend. The
/// subgoal goes in the questions. Measured on jevos-v4 (2026-10-10): with the
/// task text in the state it picked the wrong field ("Full name" for an email,
/// p=0.74, because the task mentions the name); with the bare element list it
/// picked the right one (p=0.92–0.96).
pub fn state(
    _task: &str,
    _sg: &Subgoal,
    g: &Grounding,
    _prev: Option<&Subgoal>,
) -> serde_json::Value {
    let mut elements = g.as_compact_text();
    if elements.len() > MAX_STATE_CHARS {
        let cut = (0..=MAX_STATE_CHARS)
            .rev()
            .find(|i| elements.is_char_boundary(*i))
            .unwrap_or(0);
        elements.truncate(cut);
        elements.push_str("\n…");
    }
    serde_json::Value::String(format!(
        "Elements on screen (a value after = is what a field contains; no = means \
         empty):\n{elements}"
    ))
}

/// The one request for a subgoal: `target` (when it needs one),
/// `already_done` (not for shell), `ready`, and `expect_met` for the
/// previous subgoal (verification piggybacks on the next grounding request).
pub fn questions(
    sg: &Subgoal,
    cands: &[&Element],
    prev: Option<&Subgoal>,
) -> Vec<(String, Question)> {
    let mut q = Vec::new();
    if sg.kind.needs_target() {
        let mut criteria: Vec<(String, String)> = cands
            .iter()
            .map(|e| (e.mark.to_string(), label(e)))
            .collect();
        criteria.push((
            NONE.to_string(),
            format!("None of these is the {}", sg.target.trim()),
        ));
        q.push((
            Q_TARGET.to_string(),
            Question::Criteria {
                instructions: format!(
                    "Which element is the {} (to {} it)?",
                    sg.target.trim(),
                    sg.kind.label()
                ),
                criteria,
            },
        ));
    }
    if sg.kind != SubgoalKind::Shell && !sg.expect.trim().is_empty() {
        q.push((
            Q_DONE.to_string(),
            Question::Noul {
                instructions: format!("Is this already true: {}?", sg.expect.trim()),
            },
        ));
    }
    q.push((
        Q_READY.to_string(),
        Question::Noul {
            instructions: format!("Can '{}' be done on this screen right now?", sg.describe()),
        },
    ));
    if let Some(p) = prev.filter(|p| !p.expect.trim().is_empty()) {
        q.push((Q_PREV.to_string(), met_question(p)));
    }
    q
}

/// "Is this true now: <expect>?" for a subgoal that just ran.
pub fn met_question(p: &Subgoal) -> Question {
    Question::Noul {
        instructions: format!("Is this true now: {}?", p.expect.trim()),
    }
}

/// What to do with a subgoal.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Act, on this mark when the subgoal needs a target.
    Act(Option<u32>),
    /// Its expectation already holds.
    Skip,
    /// Hand control back to the model, with this note.
    Escalate(String),
}

/// The mark a `target` answer names: an option key, its description, or a
/// leading `#N`. `None` for `none` or anything not offered.
fn resolve(option: &str, cands: &[&Element]) -> Option<u32> {
    let o = option.trim();
    let n = o
        .trim_start_matches('#')
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|d| d.parse::<u32>().ok())
        .or_else(|| cands.iter().find(|e| label(e) == o).map(|e| e.mark))?;
    cands.iter().any(|e| e.mark == n).then_some(n)
}

fn p(x: Option<f64>) -> String {
    x.map(|v| format!("{v:.2}")).unwrap_or_else(|| "?".into())
}

/// Apply the gates. The record carries the numbers for the trace (latency
/// is filled by the caller).
pub fn judge(
    sg: &Subgoal,
    answers: &BTreeMap<String, Answer>,
    cands: &[&Element],
    gates: &Gates,
) -> (Verdict, ExecutorRecord) {
    let prob = |id: &str| answers.get(id).and_then(Answer::probability);
    let target = answers.get(Q_TARGET);
    let rec = ExecutorRecord {
        subgoal: sg.describe(),
        target_p: target.and_then(Answer::confidence),
        margin: target.and_then(Answer::margin),
        ready: prob(Q_READY),
        already_done: prob(Q_DONE),
        ..Default::default()
    };
    if rec.already_done.is_some_and(|d| d >= gates.skip_done) {
        return (Verdict::Skip, rec);
    }
    let unsure = |why: &str, rec: &ExecutorRecord| {
        Verdict::Escalate(format!(
            "Executor was unsure about '{}' ({why}; target p={}, margin {}, ready {}). \
             Decide this step yourself.",
            sg.describe(),
            p(rec.target_p),
            p(rec.margin),
            p(rec.ready)
        ))
    };
    if !rec.ready.is_some_and(|r| r >= gates.ready_min) {
        return (unsure("not ready", &rec), rec);
    }
    if !sg.kind.needs_target() {
        return (Verdict::Act(None), rec);
    }
    let Some(mark) = target
        .and_then(Answer::option)
        .and_then(|o| resolve(o, cands))
    else {
        return (unsure("no matching element", &rec), rec);
    };
    if !rec.target_p.is_some_and(|c| c >= gates.target_min) {
        return (unsure("low confidence", &rec), rec);
    }
    if !rec.margin.is_some_and(|m| m >= gates.margin_min) {
        return (unsure("close runner-up", &rec), rec);
    }
    (Verdict::Act(Some(mark)), rec)
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
