//! Plan-level best-of-N for long tasks (ranked plan #9,
//! `docs/RESEARCH_2026_10_FOUNDATIONS.md` §5).
//!
//! Running N attempts on the user's live desktop is impossible (side
//! effects), but sampling N *plans* is not: before the first action Kai asks
//! the model for N short plans at a non-zero temperature, in parallel, and a
//! cheap judge with no model call picks one. The judge is self-consistency:
//! the plan whose steps the other plans agree with most wins, with a small
//! bonus for being shorter. The chosen plan rides along in every turn as a
//! hint; the model still decides each step from the screen.

use crate::plan_cache::is_stopword;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Plans asked for at most.
pub const MAX_SAMPLES: u32 = 5;
/// Steps per plan at most.
pub const MAX_PLAN_STEPS: usize = 8;
/// Words per step at most.
const STEP_WORDS: usize = 12;
/// Sampling temperature: different enough to disagree, sane enough to plan.
pub const TEMPERATURE: f32 = 0.7;
/// Reply budget for one plan.
pub const MAX_TOKENS: u32 = 320;

/// The planning call's system prompt.
pub const SYSTEM: &str = "\
You plan computer tasks for Kai, an agent that operates a desktop by clicking, \
typing, pressing keys, opening apps or URLs and running shell commands. Given \
the task and what is on screen, reply with ONE JSON object: {\"plan\": [...]} \
with 2 to 8 short steps in order, each at most 12 words, in plain words \
(name elements by their labels, never by number). No explanations.";

/// Whether a task is long enough to be worth several plans: two or more
/// clauses ("… then …", "… and then …", "…; …") or at least 14 words.
pub fn is_long(task: &str) -> bool {
    let t = task.to_lowercase();
    let clauses = [" then ", "; ", ", and ", " after that ", " afterwards "]
        .iter()
        .map(|sep| t.matches(sep).count())
        .sum::<usize>();
    clauses >= 1 || t.split_whitespace().count() >= 14
}

/// The planning call's user text.
pub fn user_text(task: &str, elements: &str) -> String {
    format!("On screen:\n{elements}\n\nTask: {task}\n\nReply with the plan as JSON.")
}

/// The JSON schema of a plan reply.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "plan": {
                "type": "array",
                "items": {"type": "string"},
                "maxItems": MAX_PLAN_STEPS
            }
        },
        "required": ["plan"]
    })
}

/// The steps of a plan reply, cleaned; `None` for a reply that is not a
/// plan (fewer than 2 usable steps).
pub fn parse(raw: &str) -> Option<Vec<String>> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    let v: Value = serde_json::from_str(raw.get(start..=end)?).ok()?;
    let steps: Vec<String> = v
        .get("plan")?
        .as_array()?
        .iter()
        .filter_map(|s| s.as_str())
        .map(|s| {
            let words: Vec<&str> = s.split_whitespace().take(STEP_WORDS).collect();
            words.join(" ")
        })
        .filter(|s| !s.is_empty())
        .take(MAX_PLAN_STEPS)
        .collect();
    (steps.len() >= 2).then_some(steps)
}

/// What the judge saw and chose, kept in the trace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BestOf {
    /// Every usable plan, in the order the calls returned.
    pub plans: Vec<Vec<String>>,
    /// The judge's score for each plan.
    pub scores: Vec<f32>,
    /// Index of the plan the run followed.
    pub chosen: usize,
    /// Planning calls made (including ones that returned no plan).
    pub calls: u32,
}

/// Pick a plan: highest agreement with the others, then the shorter one,
/// then the first. `None` without plans.
pub fn judge(plans: Vec<Vec<String>>, calls: u32) -> Option<BestOf> {
    if plans.is_empty() {
        return None;
    }
    let toks: Vec<Vec<Vec<String>>> = plans
        .iter()
        .map(|p| p.iter().map(|s| tokens(s)).collect())
        .collect();
    let scores: Vec<f32> = (0..plans.len())
        .map(|i| {
            let others: Vec<f32> = (0..plans.len())
                .filter(|&j| j != i)
                .map(|j| agreement(&toks[i], &toks[j]))
                .collect();
            let consensus = if others.is_empty() {
                0.0
            } else {
                others.iter().sum::<f32>() / others.len() as f32
            };
            consensus - 0.02 * plans[i].len() as f32
        })
        .collect();
    let chosen = (0..plans.len())
        .max_by(|&a, &b| {
            scores[a]
                .total_cmp(&scores[b])
                .then(plans[b].len().cmp(&plans[a].len()))
                .then(b.cmp(&a))
        })
        .unwrap_or(0);
    Some(BestOf {
        plans,
        scores,
        chosen,
        calls,
    })
}

/// How much of plan `a` plan `b` also does: for each step of `a`, its best
/// word overlap with any step of `b`, averaged.
fn agreement(a: &[Vec<String>], b: &[Vec<String>]) -> f32 {
    if a.is_empty() {
        return 0.0;
    }
    let best = |s: &Vec<String>| -> f32 { b.iter().map(|t| jaccard(s, t)).fold(0.0f32, f32::max) };
    a.iter().map(best).sum::<f32>() / a.len() as f32
}

fn jaccard(a: &[String], b: &[String]) -> f32 {
    let inter = a.iter().filter(|w| b.contains(w)).count() as f32;
    let union = (a.len() + b.len()) as f32 - inter;
    if union == 0.0 { 0.0 } else { inter / union }
}

fn tokens(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !is_stopword(w))
        .map(String::from)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The line the model sees in every turn while it follows the plan.
pub fn note(b: &BestOf) -> String {
    let steps: Vec<String> = b.plans[b.chosen]
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {s}", i + 1))
        .collect();
    format!(
        "Plan for this task (a guide; the screen decides): {}",
        steps.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(steps: &[&str]) -> Vec<String> {
        steps.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn long_tasks_are_the_ones_with_several_clauses() {
        assert!(is_long("Open the report, then export it as PDF"));
        assert!(is_long("download the file; unzip it"));
        assert!(!is_long("open firefox"));
        assert!(!is_long("Write hello in Mousepad"));
    }

    #[test]
    fn the_plan_the_others_agree_with_wins() {
        let a = p(&["open LibreOffice Calc", "open report.ods", "export as PDF"]);
        let b = p(&[
            "open the file report.ods in Calc",
            "use File > Export as PDF",
        ]);
        let odd = p(&[
            "open a terminal",
            "run libreoffice --convert-to pdf",
            "check the folder",
        ]);
        let c = p(&[
            "start Calc",
            "open report.ods",
            "File menu, Export as PDF",
            "save",
        ]);
        let best = judge(vec![odd, a.clone(), b, c], 4).unwrap();
        assert_ne!(best.chosen, 0, "the outlier lost: {:?}", best.scores);
        assert_eq!(best.calls, 4);
    }

    #[test]
    fn ties_go_to_the_shorter_plan() {
        let short = p(&["open settings", "turn on dark mode"]);
        let long = p(&[
            "open settings",
            "turn on dark mode",
            "open settings",
            "turn on dark mode",
        ]);
        let best = judge(vec![long, short.clone()], 2).unwrap();
        assert_eq!(best.plans[best.chosen], short);
    }

    #[test]
    fn replies_that_are_not_plans_are_dropped() {
        assert_eq!(parse(r#"{"plan":["only one step"]}"#), None);
        assert_eq!(parse("I would open the app"), None);
        let ok = parse(r#"Sure: {"plan":["open the app","click Save"]}"#).unwrap();
        assert_eq!(ok, vec!["open the app", "click Save"]);
        assert!(judge(Vec::new(), 3).is_none());
    }

    #[test]
    fn the_note_numbers_the_chosen_plan() {
        let b = judge(vec![p(&["open settings", "turn on dark mode"])], 1).unwrap();
        assert_eq!(
            note(&b),
            "Plan for this task (a guide; the screen decides): 1. open settings 2. turn on dark mode"
        );
    }
}
