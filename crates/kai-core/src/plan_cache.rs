//! Plan cache ("auto-skills"): after a successful run, keep the deterministic
//! prefix of its trace (open_url / open_app / low-risk shell / key / untargeted
//! type) keyed by the normalised task. A later run of the same (or nearly the
//! same) task replays that prefix with no model calls, then hands over to the
//! model, which verifies from a fresh full observation.
//!
//! OS-free: JSON files in a directory the caller passes in. Matching is
//! deliberately strict — exact normalised text, or a high token overlap with
//! identical "content" tokens (numbers, URLs, names with dots) — so a cached
//! plan for "email alice" never runs for "email bob".

use crate::risk;
use crate::schema::{Action, Decision, Risk};
use crate::trace::{Outcome, Trace};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Minimum Jaccard overlap of normalised tokens for a near match.
pub const MATCH_THRESHOLD: f32 = 0.85;
/// Longest prefix kept.
const MAX_PREFIX: usize = 8;

/// One stored plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedPlan {
    /// The original task text.
    pub task: String,
    /// The normalised key it was stored under.
    pub key: String,
    /// The deterministic prefix, replayed in order.
    pub steps: Vec<Decision>,
    /// When it was recorded.
    pub recorded: String,
}

/// Plans on disk, loaded once and updated as runs succeed.
pub struct PlanCache {
    dir: PathBuf,
    plans: Mutex<Vec<CachedPlan>>,
}

impl PlanCache {
    /// Load every `*.json` plan in `dir` (created if missing). Unreadable
    /// files are skipped.
    pub fn load(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let mut plans = Vec::new();
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "json")
                && let Ok(text) = std::fs::read_to_string(&p)
                && let Ok(plan) = serde_json::from_str::<CachedPlan>(&text)
            {
                plans.push(plan);
            }
        }
        Ok(Self {
            dir,
            plans: Mutex::new(plans),
        })
    }

    /// An empty, in-memory-only cache (records are not written).
    pub fn in_memory() -> Self {
        Self {
            dir: PathBuf::new(),
            plans: Mutex::new(Vec::new()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn len(&self) -> usize {
        self.plans.lock().map(|p| p.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every stored plan (for memory's skill suggestions).
    pub fn plans(&self) -> Vec<CachedPlan> {
        self.plans.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// The plan for `task`, if one matches closely enough.
    pub fn lookup(&self, task: &str) -> Option<CachedPlan> {
        let key = normalise(task);
        if key.is_empty() {
            return None;
        }
        let plans = self.plans.lock().ok()?;
        if let Some(p) = plans.iter().find(|p| p.key == key) {
            return Some(p.clone());
        }
        plans
            .iter()
            .map(|p| (p, similarity(&key, &p.key)))
            .filter(|(_, s)| *s >= MATCH_THRESHOLD)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(p, _)| p.clone())
    }

    /// Store the deterministic prefix of a successful run. Returns whether a
    /// plan was stored (false for failed runs or runs with no such prefix).
    pub fn record(&self, trace: &Trace) -> std::io::Result<bool> {
        if trace.outcome != Some(Outcome::Success) {
            return Ok(false);
        }
        let steps = deterministic_prefix(trace);
        let key = normalise(&trace.task);
        if steps.is_empty() || key.is_empty() {
            return Ok(false);
        }
        let plan = CachedPlan {
            task: trace.task.clone(),
            key: key.clone(),
            steps,
            recorded: jiff::Timestamp::now().to_string(),
        };
        if !self.dir.as_os_str().is_empty() {
            let path = self.dir.join(format!("{:016x}.json", fnv1a(&key)));
            let json = serde_json::to_string_pretty(&plan).map_err(std::io::Error::other)?;
            std::fs::write(path, json)?;
        }
        if let Ok(mut plans) = self.plans.lock() {
            plans.retain(|p| p.key != key);
            plans.push(plan);
        }
        Ok(true)
    }
}

/// Whether a decision can be replayed blind: it reaches the OS directly or
/// types/presses keys into whatever has focus, and is not high risk.
pub fn is_replayable(d: &Decision) -> bool {
    match d.action {
        Action::OpenUrl | Action::OpenApp => d.risk != Risk::High,
        Action::Shell => {
            risk::classify_shell(d.text.as_deref().unwrap_or("")) != Risk::High
                && d.risk != Risk::High
        }
        Action::Key | Action::Type => d.target.is_none() && d.risk != Risk::High,
        _ => false,
    }
}

/// The leading run of replayable steps that executed without an approval
/// pause or an error.
pub fn deterministic_prefix(trace: &Trace) -> Vec<Decision> {
    trace
        .steps
        .iter()
        .take_while(|s| {
            s.approval.is_none()
                && is_replayable(&s.decision)
                && !s.output.as_deref().is_some_and(|o| o.starts_with("ERROR"))
        })
        .take(MAX_PREFIX)
        .map(|s| s.decision.clone())
        .collect()
}

/// Lowercase alphanumeric tokens (dots, slashes, `@`, `-` kept inside tokens
/// so URLs and file names survive), stopwords dropped, joined by spaces.
pub fn normalise(task: &str) -> String {
    task.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || "./@-_:".contains(c)))
        .map(|w| w.trim_matches(|c: char| ".:-".contains(c)))
        .filter(|w| !w.is_empty() && !STOPWORDS.contains(w))
        .collect::<Vec<_>>()
        .join(" ")
}

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "please", "and", "then", "to", "for", "of", "in", "on", "me", "my", "it",
    "can", "you", "kai",
];

/// Token Jaccard similarity, but 0 unless the "content" tokens (anything with
/// a digit, a dot, `/` or `@`) are identical: those carry the specifics.
fn similarity(a: &str, b: &str) -> f32 {
    let ta: BTreeSet<&str> = a.split(' ').collect();
    let tb: BTreeSet<&str> = b.split(' ').collect();
    let content = |s: &BTreeSet<&str>| -> BTreeSet<String> {
        s.iter()
            .filter(|w| w.chars().any(|c| c.is_ascii_digit() || "./@".contains(c)))
            .map(|w| w.to_string())
            .collect()
    };
    if content(&ta) != content(&tb) {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f32;
    let union = ta.union(&tb).count() as f32;
    if union == 0.0 { 0.0 } else { inter / union }
}

/// Stable 64-bit FNV-1a, for file names that survive Rust upgrades.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{StepRecord, Timings};

    fn d(raw: &str) -> Decision {
        Decision::parse_checked(raw).unwrap()
    }

    fn trace(task: &str, steps: &[Decision], outcome: Outcome) -> Trace {
        let mut t = Trace::new(task);
        for (i, s) in steps.iter().enumerate() {
            t.push(StepRecord::new(i as u32, s, Timings::default(), "Mock"));
        }
        t.finish(outcome);
        t
    }

    fn sample() -> Vec<Decision> {
        vec![
            d(r#"{"thought":"","action":"open_url","text":"https://example.com","risk":"none"}"#),
            d(r#"{"thought":"","action":"key","keys":["ctrl","l"],"risk":"none"}"#),
            d(r#"{"thought":"","action":"click","target":3,"risk":"low"}"#),
            d(r#"{"thought":"","action":"shell","text":"ls","risk":"none"}"#),
            d(r#"{"thought":"","action":"done","risk":"none"}"#),
        ]
    }

    #[test]
    fn prefix_stops_at_first_grounded_step() {
        let t = trace("x", &sample(), Outcome::Success);
        let p = deterministic_prefix(&t);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].action, Action::OpenUrl);
    }

    #[test]
    fn destructive_shell_is_not_replayable() {
        let rm = d(r#"{"thought":"","action":"shell","text":"rm -rf /tmp/x","risk":"none"}"#);
        assert!(!is_replayable(&rm));
    }

    #[test]
    fn records_and_looks_up_on_disk() {
        let dir = std::env::temp_dir().join(format!("kai-plan-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = PlanCache::load(&dir).unwrap();
        assert!(
            !c.record(&trace("Open example.com", &sample(), Outcome::Failed))
                .unwrap()
        );
        assert!(
            c.record(&trace(
                "Open example.com please",
                &sample(),
                Outcome::Success
            ))
            .unwrap()
        );
        let again = PlanCache::load(&dir).unwrap();
        assert_eq!(again.len(), 1);
        let hit = again.lookup("open  Example.com").unwrap();
        assert_eq!(hit.steps.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn matching_is_strict() {
        let c = PlanCache::in_memory();
        c.record(&trace(
            "open example.com and search for rust async book",
            &sample(),
            Outcome::Success,
        ))
        .unwrap();
        // Same words, different order and filler: match.
        assert!(
            c.lookup("please open example.com then search for rust async book")
                .is_some()
        );
        // A different site: content tokens differ, no match.
        assert!(
            c.lookup("open example.org and search for rust async book")
                .is_none()
        );
        // A different query: overlap too low.
        assert!(c.lookup("open example.com and search for cats").is_none());
    }

    #[test]
    fn normalise_keeps_urls_and_drops_filler() {
        assert_eq!(
            normalise("Please open https://Example.com/a, then type Hi!"),
            "open https://example.com/a type hi"
        );
    }
}
