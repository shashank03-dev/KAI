//! Procedures (`docs/PROCEDURES.md`): a whole successful run, kept so the
//! same task, or the same task with different specifics, replays with no
//! model call. This generalises the plan cache, which keeps only the
//! untargeted opening of a run.
//!
//! - **Steps** keep the action and its target's signature (role and name),
//!   never a mark number, so a replay finds the element again on a new frame.
//! - **Slots**: the task's specifics (quoted text, anything with a digit, a
//!   dot, `/` or `@`) are slots. A new task with the same words and different
//!   specifics rebinds them in the steps' text. A specific that no step uses
//!   cannot be rebound, so the procedure does not match.
//! - **Lifecycle**: `candidate` → `active` (the run's outcomes were all
//!   verified by probes, or a second run succeeded) → `quarantined` (a replay
//!   did not end in success) → `retired` (a second failure). Only `active`
//!   procedures replay.
//!
//! OS-free: JSON files in a directory the caller passes in.

use crate::ledger::{Status as OutcomeStatus, VerifiedBy};
use crate::plan_cache::is_stopword;
use crate::risk;
use crate::schema::{Action, Decision, Risk};
use crate::trace::{Outcome, TargetSig, Trace};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// Longest procedure kept.
pub const MAX_STEPS: usize = 20;
/// The word that stands for a slot in a task's shape.
const SLOT: &str = "<slot>";

/// Where a procedure is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Candidate,
    Active,
    Quarantined,
    Retired,
}

/// One stored step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcStep {
    /// The action as recorded; `target` is filled in again at replay.
    pub decision: Decision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetSig>,
}

/// One stored procedure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Procedure {
    /// The task it was recorded from.
    pub task: String,
    /// The task's words with every specific replaced by `<slot>`.
    pub shape: String,
    /// The recorded task's specifics, in order.
    pub slots: Vec<String>,
    pub steps: Vec<ProcStep>,
    pub status: Status,
    /// Runs that succeeded with it, recorded or replayed.
    pub successes: u32,
    /// Replays that did not end in success.
    pub failures: u32,
    /// The recording run's outcomes were all verified by a probe.
    pub verified: bool,
    pub recorded: String,
}

/// A procedure with its slots bound to a new task, ready to replay.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub shape: String,
    pub steps: Vec<ProcStep>,
}

/// Procedures on disk, loaded once and updated as runs end.
pub struct ProcedureStore {
    dir: PathBuf,
    procs: Mutex<Vec<Procedure>>,
}

impl ProcedureStore {
    /// Load every `*.json` procedure in `dir` (created if missing).
    pub fn load(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let mut procs = Vec::new();
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "json")
                && let Ok(text) = std::fs::read_to_string(&p)
                && let Ok(proc) = serde_json::from_str::<Procedure>(&text)
            {
                procs.push(proc);
            }
        }
        Ok(Self {
            dir,
            procs: Mutex::new(procs),
        })
    }

    /// An empty store that writes nothing to disk.
    pub fn in_memory() -> Self {
        Self {
            dir: PathBuf::new(),
            procs: Mutex::new(Vec::new()),
        }
    }

    /// Every stored procedure.
    pub fn list(&self) -> Vec<Procedure> {
        self.procs.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// The active procedure for `task`, bound to its specifics.
    pub fn lookup(&self, task: &str) -> Option<Bound> {
        let (shape, slots) = shape(task);
        if shape.is_empty() {
            return None;
        }
        let procs = self.procs.lock().ok()?;
        let p = procs
            .iter()
            .find(|p| p.status == Status::Active && p.shape == shape)?;
        Some(Bound {
            shape: p.shape.clone(),
            steps: bind(&p.steps, &p.slots, &slots)?,
        })
    }

    /// Store a successful run that did not replay a procedure. `verified`:
    /// every outcome of the run was verified by a probe ([`all_verified`]).
    /// Returns whether anything was stored.
    pub fn record(&self, trace: &Trace, verified: bool) -> std::io::Result<bool> {
        if trace.outcome != Some(Outcome::Success) {
            return Ok(false);
        }
        let steps = steps_of(trace);
        let (shape, slots) = shape(&trace.task);
        if steps.is_empty() || shape.is_empty() {
            return Ok(false);
        }
        let mut procs = self
            .procs
            .lock()
            .map_err(|_| std::io::Error::other("poisoned"))?;
        let old = procs.iter().position(|p| p.shape == shape);
        let mut p = Procedure {
            task: trace.task.clone(),
            shape,
            slots,
            steps,
            status: Status::Candidate,
            successes: 1,
            failures: 0,
            verified,
            recorded: jiff::Timestamp::now().to_string(),
        };
        if let Some(i) = old {
            let prev = &procs[i];
            if prev.status == Status::Retired {
                return Ok(false);
            }
            // Failures always carry over (two in all retire it); after a
            // quarantine, successes start again.
            p.failures = prev.failures;
            if prev.status != Status::Quarantined {
                p.successes = prev.successes + 1;
            }
        }
        if p.verified || p.successes >= 2 {
            p.status = Status::Active;
        }
        self.write(&p)?;
        match old {
            Some(i) => procs[i] = p,
            None => procs.push(p),
        }
        Ok(true)
    }

    /// How a replay of the procedure with this shape ended.
    pub fn report(&self, shape: &str, success: bool) -> std::io::Result<()> {
        let mut procs = self
            .procs
            .lock()
            .map_err(|_| std::io::Error::other("poisoned"))?;
        let Some(p) = procs.iter_mut().find(|p| p.shape == shape) else {
            return Ok(());
        };
        if success {
            p.successes += 1;
        } else {
            p.failures += 1;
            p.status = if p.failures >= 2 {
                Status::Retired
            } else {
                Status::Quarantined
            };
        }
        let p = p.clone();
        self.write(&p)
    }

    /// Delete a procedure. Returns whether one was found.
    pub fn forget(&self, shape: &str) -> std::io::Result<bool> {
        let mut procs = self
            .procs
            .lock()
            .map_err(|_| std::io::Error::other("poisoned"))?;
        let before = procs.len();
        procs.retain(|p| p.shape != shape);
        if !self.dir.as_os_str().is_empty() {
            let _ = std::fs::remove_file(self.path(shape));
        }
        Ok(procs.len() != before)
    }

    fn path(&self, shape: &str) -> PathBuf {
        self.dir
            .join(format!("{:016x}.json", crate::plan_cache::fnv1a(shape)))
    }

    fn write(&self, p: &Procedure) -> std::io::Result<()> {
        if self.dir.as_os_str().is_empty() {
            return Ok(());
        }
        let json = serde_json::to_string_pretty(p).map_err(std::io::Error::other)?;
        std::fs::write(self.path(&p.shape), json)
    }
}

/// Whether every outcome of the run was verified by a probe (not waived).
pub fn all_verified(trace: &Trace) -> bool {
    !trace.outcomes.is_empty()
        && trace
            .outcomes
            .iter()
            .all(|o| o.status == OutcomeStatus::Verified && o.by == Some(VerifiedBy::Probe))
}

/// The leading steps of a run that can replay: no approval pause, no human
/// check, no error, nothing high risk, and every targeted step with a
/// signature. Stops before `done`.
pub fn steps_of(trace: &Trace) -> Vec<ProcStep> {
    trace
        .steps
        .iter()
        .take_while(|s| {
            s.approval.is_none()
                && s.path.as_deref() != Some("human")
                && !s.output.as_deref().is_some_and(|o| o.starts_with("ERROR"))
                && replayable(&s.decision, s.target.is_some())
        })
        .take(MAX_STEPS)
        .map(|s| ProcStep {
            decision: Decision {
                target: None,
                ..s.decision.clone()
            },
            target: s.target.clone(),
        })
        .collect()
}

fn replayable(d: &Decision, has_sig: bool) -> bool {
    if d.risk == Risk::High {
        return false;
    }
    let targeted_ok = d.target.is_none() || has_sig;
    match d.action {
        Action::OpenUrl | Action::OpenApp | Action::Wait => true,
        Action::Shell => risk::classify_shell(d.text.as_deref().unwrap_or("")) != Risk::High,
        Action::Key | Action::Scroll => targeted_ok,
        Action::Click | Action::DoubleClick | Action::RightClick | Action::Type => {
            (d.target.is_some() && has_sig) || (d.action == Action::Type && d.target.is_none())
        }
        Action::AskUser | Action::Done | Action::Computer => false,
    }
}

/// A task's shape (its words, specifics as `<slot>`) and its specifics.
pub fn shape(task: &str) -> (String, Vec<String>) {
    let mut words: Vec<String> = Vec::new();
    let mut slots: Vec<String> = Vec::new();
    let quotes = task.matches(['"', '“', '”']).count();
    for (i, piece) in task.split(['"', '“', '”']).enumerate() {
        // Odd pieces sit between quotes; an unpaired last quote leaves the
        // rest of the task as plain words.
        if i % 2 == 1 && quotes > i {
            let v = piece.trim();
            if !v.is_empty() {
                words.push(SLOT.into());
                slots.push(v.to_string());
            }
            continue;
        }
        for w in piece
            .split(|c: char| !(c.is_alphanumeric() || "./@-_:".contains(c)))
            .map(|w| w.trim_matches(|c: char| ".:-".contains(c)))
            .filter(|w| !w.is_empty() && !is_stopword(&w.to_lowercase()))
        {
            if w.chars().any(|c| c.is_ascii_digit() || "./@".contains(c)) {
                words.push(SLOT.into());
                slots.push(w.to_string());
            } else {
                words.push(w.to_lowercase());
            }
        }
    }
    (words.join(" "), slots)
}

/// The steps with `old` specifics replaced by `new` ones, or `None` when a
/// changed specific is used by no step, or would land in a shell command
/// with characters a shell treats specially.
pub fn bind(steps: &[ProcStep], old: &[String], new: &[String]) -> Option<Vec<ProcStep>> {
    if old.len() != new.len() {
        return None;
    }
    let changed: Vec<usize> = (0..old.len()).filter(|&i| old[i] != new[i]).collect();
    let mut out = steps.to_vec();
    for &i in &changed {
        let used = steps.iter().any(|s| {
            s.decision
                .text
                .as_deref()
                .is_some_and(|t| t.contains(&old[i]))
                || s.target.as_ref().is_some_and(|t| t.name.contains(&old[i]))
        });
        if !used {
            return None;
        }
        let in_shell = steps.iter().any(|s| {
            s.decision.action == Action::Shell
                && s.decision
                    .text
                    .as_deref()
                    .is_some_and(|t| t.contains(&old[i]))
        });
        if in_shell && !shell_safe(&new[i]) {
            return None;
        }
    }
    if changed.is_empty() {
        return Some(out);
    }
    for s in &mut out {
        if let Some(t) = &s.decision.text {
            s.decision.text = Some(rebind(t, old, new, &changed));
        }
        if let Some(sig) = &mut s.target {
            sig.name = rebind(&sig.name, old, new, &changed);
        }
    }
    Some(out)
}

/// Replace every changed specific at once (longest first, through markers),
/// so a new value is never itself replaced again.
fn rebind(text: &str, old: &[String], new: &[String], changed: &[usize]) -> String {
    let mut order = changed.to_vec();
    order.sort_by_key(|&i| std::cmp::Reverse(old[i].len()));
    // One private-use character per slot: it holds no digit or letter a
    // later value could match.
    let mark = |i: usize| char::from_u32(0xE000 + i as u32).unwrap_or('\u{E000}');
    let mut s = text.to_string();
    for &i in &order {
        s = s.replace(&old[i], &mark(i).to_string());
    }
    for &i in &order {
        s = s.replace(mark(i), &new[i]);
    }
    s
}

/// A value that means the same to a shell as it reads: no spaces, quotes,
/// globs, variables, redirections or command separators.
fn shell_safe(v: &str) -> bool {
    !v.is_empty()
        && v.chars()
            .all(|c| c.is_alphanumeric() || "./@-_:+,=%".contains(c))
        && !v.starts_with('-')
}

#[cfg(test)]
#[path = "procedure_tests.rs"]
mod tests;
