//! Consolidation off the hot path (`docs/MEMORY.md`, "Consolidation and the
//! exam"). Live learning sees one run at a time. This job sees many: it
//! re-reads saved runs, re-applies the fixed learning rules to each, and
//! keeps the lessons that recur across at least [`MIN_SUPPORT`] runs and are
//! not already known. They are stored as **pending**: never shown to the
//! model until they pass the exam ([`super::exam`]) or the user accepts them.
//! No model call.

use super::learn::learn;
use super::{Lesson, Source, similarity};
use crate::trace::Trace;

/// Distinct runs a lesson must come from.
pub const MIN_SUPPORT: usize = 2;
/// Proposals per consolidation at most.
pub const MAX_PROPOSALS: usize = 10;
/// Texts this similar (same kind) are the same lesson.
const SAME: f32 = 0.8;

/// Lessons that recur across `traces` and are not near-duplicates of any of
/// `known`, most supported first, marked pending.
pub fn propose(traces: &[Trace], known: &[Lesson]) -> Vec<Lesson> {
    let mut groups: Vec<(Lesson, Vec<String>)> = Vec::new();
    for t in traces {
        for l in learn(t, None) {
            // The user's own words were kept when they said them.
            if l.source == Source::User {
                continue;
            }
            match groups
                .iter_mut()
                .find(|(g, _)| g.kind == l.kind && similarity(&g.text, &l.text) >= SAME)
            {
                Some((g, runs)) => {
                    g.scope.merge(&l.scope);
                    if !runs.contains(&t.started) {
                        runs.push(t.started.clone());
                    }
                }
                None => groups.push((l, vec![t.started.clone()])),
            }
        }
    }
    let mut out: Vec<(Lesson, usize)> = groups
        .into_iter()
        .filter(|(_, runs)| runs.len() >= MIN_SUPPORT)
        .filter(|(g, _)| {
            !known
                .iter()
                .any(|k| k.kind == g.kind && similarity(&k.text, &g.text) >= SAME)
        })
        .map(|(mut g, runs)| {
            let n = runs.len();
            g.evidence = runs.into_iter().take(20).collect();
            g.source = Source::Consolidated;
            g.pending = true;
            (g, n)
        })
        .collect();
    out.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    out.into_iter()
        .take(MAX_PROPOSALS)
        .map(|(g, _)| g)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::tests::{EDITOR, step, trace};
    use crate::trace::Outcome;

    /// The same dead end (clicking Save twice did nothing), on its own run.
    fn dead_end(started: &str) -> Trace {
        let click = r#"{"thought":"x","action":"click","target":1,"risk":"low","expect":"saved"}"#;
        let mut t = trace(
            "save it",
            vec![
                step(0, click, false, None, EDITOR),
                step(1, click, false, None, EDITOR),
            ],
            Outcome::AskedUser,
        );
        t.started = started.into();
        t
    }

    #[test]
    fn a_lesson_seen_in_two_runs_is_proposed_pending() {
        let p = propose(&[dead_end("run-1"), dead_end("run-2")], &[]);
        assert_eq!(p.len(), 1);
        assert!(p[0].pending);
        assert_eq!(p[0].source, Source::Consolidated);
        assert_eq!(p[0].evidence, vec!["run-1", "run-2"]);
    }

    #[test]
    fn one_run_is_not_a_pattern() {
        assert!(propose(&[dead_end("run-1")], &[]).is_empty());
        // The same run read twice is still one run.
        assert!(propose(&[dead_end("run-1"), dead_end("run-1")], &[]).is_empty());
    }

    #[test]
    fn what_memory_already_knows_is_not_proposed_again() {
        let known = propose(&[dead_end("a"), dead_end("b")], &[]);
        let mut live = known[0].clone();
        live.pending = false;
        assert!(propose(&[dead_end("c"), dead_end("d")], &[live]).is_empty());
    }

    #[test]
    fn pending_lessons_are_never_recalled_until_accepted() {
        let store = crate::memory::MemoryStore::in_memory();
        let p = store
            .propose(propose(&[dead_end("a"), dead_end("b")], &[]))
            .unwrap();
        let cx = crate::memory::recall::Context::new("save it", &["mousepad".into()], &[]);
        assert!(store.recall_in(&cx).is_empty());
        assert_eq!(store.pending().len(), 1);
        assert_eq!(store.accept(&p[0].id).unwrap(), 1);
        assert_eq!(store.recall_in(&cx).len(), 1);
        assert!(store.pending().is_empty());
    }

    #[test]
    fn a_live_lesson_merged_into_a_pending_twin_is_live() {
        let store = crate::memory::MemoryStore::in_memory();
        let p = store
            .propose(propose(&[dead_end("a"), dead_end("b")], &[]))
            .unwrap();
        let mut live = p[0].clone();
        live.pending = false;
        store.add(live).unwrap();
        let cx = crate::memory::recall::Context::new("save it", &["mousepad".into()], &[]);
        assert_eq!(store.recall_in(&cx).len(), 1);
        assert!(store.pending().is_empty());
    }
}
