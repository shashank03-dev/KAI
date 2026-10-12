//! Experience memory: Kai learns from its own runs on this computer without
//! training the model. After a run, short imperative **lessons** are drawn
//! from the trace ("In Mousepad, start it with open_app, not shell: a shell
//! step blocks."), stored locally, and the few that fit the next task and the
//! app or site in view are shown to the model as hints in its user turn.
//!
//! OS-free and model-agnostic: lessons are plain text, so any backend (a local
//! Gemma, a hosted model) benefits. Lessons are hints only — approvals, risk
//! and the kill switch never read them. See `docs/MEMORY.md`.
//!
//! - [`learn`]: deterministic extraction from a trace (recovery, dead end,
//!   steering, false done);
//! - [`reflect`]: the optional one-call reflection after a failed run;
//! - [`recall`]: ranking under a small word budget;
//! - [`MemoryStore`]: the local JSONL store with merging, caps and credit;
//! - [`promote`]: skill-draft suggestions from proven lessons.

pub mod facts;
pub mod learn;
pub mod promote;
pub mod recall;
pub mod reflect;
pub mod scope;
pub mod scrub;
mod store;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_store;

pub use store::{AfterRun, MemoryStore};

use crate::failure::Category;
use serde::{Deserialize, Serialize};

/// Longest lesson text, in words.
pub const MAX_WORDS: usize = 30;

/// What a lesson says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Something that failed here: do not do it again.
    Avoid,
    /// What worked after something else failed.
    Prefer,
    /// A fact about this computer (where a file is, which app opens PDFs).
    Fact,
    /// How the user wants things done.
    Preference,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Avoid => "avoid",
            Kind::Prefer => "prefer",
            Kind::Fact => "fact",
            Kind::Preference => "preference",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "avoid" => Some(Kind::Avoid),
            "prefer" => Some(Kind::Prefer),
            "fact" => Some(Kind::Fact),
            "preference" => Some(Kind::Preference),
            _ => None,
        }
    }
}

/// Where a lesson came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Drawn from a trace by fixed rules.
    Derived,
    /// Written by the model in the optional reflection call.
    Reflection,
    /// Typed by the user (`kai memory add`, the app, or a mid-run message).
    User,
}

/// Where a lesson applies. All lowercase.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scope {
    /// Apps, e.g. `mousepad`, `firefox`.
    pub apps: Vec<String>,
    /// Sites, e.g. `github.com` (no `www.`).
    pub domains: Vec<String>,
    /// Stopword-filtered words of the task it was learned on.
    pub task_words: Vec<String>,
    /// Coarse signatures of the screens it was learned on
    /// ([`scope::screen_sig`]): recall ranks it higher there.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub screens: Vec<String>,
}

impl Scope {
    /// No app and no site: the lesson applies by task words alone.
    pub fn is_global(&self) -> bool {
        self.apps.is_empty() && self.domains.is_empty()
    }

    /// Union, keeping order and at most `cap` entries per list.
    pub fn merge(&mut self, other: &Scope) {
        fn join(a: &mut Vec<String>, b: &[String], cap: usize) {
            for x in b {
                if a.len() >= cap {
                    break;
                }
                if !a.contains(x) {
                    a.push(x.clone());
                }
            }
        }
        join(&mut self.apps, &other.apps, 6);
        join(&mut self.domains, &other.domains, 6);
        join(&mut self.task_words, &other.task_words, 16);
        join(&mut self.screens, &other.screens, 6);
    }
}

/// One thing Kai learned on this computer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lesson {
    /// Short hex id, stable across merges.
    pub id: String,
    pub kind: Kind,
    /// At most [`MAX_WORDS`] words, imperative.
    pub text: String,
    #[serde(default)]
    pub scope: Scope,
    /// Run ids (trace start times) that produced or confirmed it.
    #[serde(default)]
    pub evidence: Vec<String>,
    /// Unix seconds.
    pub created: i64,
    #[serde(default)]
    pub last_used: i64,
    /// Runs it was shown in.
    #[serde(default)]
    pub uses: u32,
    #[serde(default)]
    pub helped: u32,
    #[serde(default)]
    pub hurt: u32,
    pub source: Source,
    /// The failure it warns about, for credit (`hurt` when a run that saw
    /// it fails this same way).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Category>,
}

/// Runs a lesson must have been shown in before it can be retired.
pub const RETIRE_USES: u32 = 3;
/// Confidence under which a used lesson is retired.
pub const RETIRE_BELOW: f32 = 0.3;

impl Lesson {
    /// A new lesson created now. The text is cut to [`MAX_WORDS`] words.
    pub fn new(kind: Kind, text: &str, scope: Scope, source: Source) -> Self {
        let text = clip_words(text.trim(), MAX_WORDS);
        let now = now();
        Self {
            id: format!("{:08x}", fnv1a(&format!("{text}|{now}")) as u32),
            kind,
            text,
            scope,
            evidence: Vec::new(),
            created: now,
            last_used: 0,
            uses: 0,
            helped: 0,
            hurt: 0,
            source,
            targets: None,
        }
    }

    pub fn with_evidence(mut self, run: &str) -> Self {
        if !run.is_empty() && !self.evidence.iter().any(|e| e == run) {
            self.evidence.push(run.to_string());
        }
        self
    }

    pub fn targeting(mut self, c: Option<Category>) -> Self {
        self.targets = c;
        self
    }

    /// (helped + 1) / (helped + hurt + 2): 0.5 for a new lesson.
    pub fn confidence(&self) -> f32 {
        (self.helped as f32 + 1.0) / ((self.helped + self.hurt) as f32 + 2.0)
    }

    /// Shown often and mostly unhelpful. The user's own lessons are never
    /// retired automatically.
    pub fn retired(&self) -> bool {
        self.source != Source::User && self.uses >= RETIRE_USES && self.confidence() < RETIRE_BELOW
    }

    pub fn words(&self) -> usize {
        self.text.split_whitespace().count()
    }

    /// When it was last relevant: its last use, else its creation.
    pub fn touched(&self) -> i64 {
        self.last_used.max(self.created)
    }

    /// Ranking weight: confidence, halved for every [`HALF_LIFE_DAYS`] it
    /// went unused (age decay), floored so an old, proven lesson still counts.
    pub fn weight(&self, now: i64) -> f32 {
        let days = (now - self.touched()).max(0) as f32 / 86_400.0;
        self.confidence() * 0.5f32.powf(days / HALF_LIFE_DAYS).max(0.2)
    }
}

/// Days for a lesson's ranking weight to halve while it goes unused.
pub const HALF_LIFE_DAYS: f32 = 90.0;

/// Unix seconds now.
pub fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// The first `max` words of `s`, with a trailing `…` when cut.
pub fn clip_words(s: &str, max: usize) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() <= max {
        words.join(" ")
    } else {
        format!("{}…", words[..max].join(" "))
    }
}

/// Lowercase alphanumeric tokens, for duplicate detection.
pub fn norm_tokens(s: &str) -> std::collections::BTreeSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Token Jaccard similarity of two texts.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (ta, tb) = (norm_tokens(a), norm_tokens(b));
    let union = ta.union(&tb).count();
    if union == 0 {
        return 0.0;
    }
    ta.intersection(&tb).count() as f32 / union as f32
}

/// Two lessons at or above this token overlap (same kind) are merged.
pub const MERGE_SIMILARITY: f32 = 0.8;

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
