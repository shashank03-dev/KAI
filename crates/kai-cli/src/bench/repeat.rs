//! `kai bench --memory --repeat 2`: each task runs twice back to back with a
//! fresh in-memory lesson store, so the second run can use what the first
//! taught. This compares first runs with second runs.

use super::runner::Run;
use kai_core::Outcome;

/// Totals for one pass.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pass {
    pub ran: usize,
    pub passed: usize,
    pub steps: usize,
    pub calls: usize,
    /// Lessons shown to the model, summed over the pass's runs.
    pub lessons_shown: usize,
}

/// Did a run pass? The check decides when there is one.
fn ok(r: &Run) -> bool {
    r.check.unwrap_or_else(|| {
        r.trace
            .as_ref()
            .is_some_and(|t| t.outcome == Some(Outcome::Success))
    })
}

/// Per pass, in order. Skipped runs are left out.
pub fn passes(runs: &[Run]) -> Vec<Pass> {
    let n = runs.iter().map(|r| r.pass as usize + 1).max().unwrap_or(0);
    let mut out = vec![Pass::default(); n];
    for r in runs.iter().filter(|r| r.skipped.is_none()) {
        let p = &mut out[r.pass as usize];
        p.ran += 1;
        p.passed += usize::from(ok(r));
        p.steps += r.trace.as_ref().map_or(0, |t| t.steps.len());
        p.calls += r.calls.len();
        p.lessons_shown += r.trace.as_ref().map_or(0, |t| t.lessons.len());
    }
    out
}

/// The Markdown comparison table.
pub fn table(runs: &[Run], lessons_stored: usize) -> String {
    let mut s = String::from(
        "## Experience memory: first run vs repeat\n\n\
         | Pass | Tasks | Passed | Mean steps | Mean model calls | Lessons shown |\n\
         |---|---|---|---|---|---|\n",
    );
    for (i, p) in passes(runs).iter().enumerate() {
        let mean = |x: usize| {
            if p.ran == 0 {
                0.0
            } else {
                x as f64 / p.ran as f64
            }
        };
        s.push_str(&format!(
            "| {} | {} | {} | {:.1} | {:.1} | {} |\n",
            i + 1,
            p.ran,
            p.passed,
            mean(p.steps),
            mean(p.calls),
            p.lessons_shown
        ));
    }
    s.push_str(&format!(
        "\nLessons in the bench's own store at the end: {lessons_stored} (fresh, in memory; \
         the user's lessons are never used by `kai bench`).\n"
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::runner::MemoryOpts;
    use crate::bench::runner::run_mock_with;

    #[tokio::test]
    async fn repeat_runs_each_task_twice_with_shared_memory() {
        let tasks = crate::bench::tasks::load_tasks(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../bench/tasks.yaml"
        )))
        .unwrap();
        let some: Vec<_> = tasks.into_iter().take(3).collect();
        let store = kai_core::MemoryStore::in_memory();
        let mem = MemoryOpts {
            store: Some(&store),
            repeat: 2,
        };
        let runs = run_mock_with(&some, &[], kai_core::Config::default(), None, None, mem).await;
        assert_eq!(runs.len(), 6);
        assert_eq!(
            runs.iter().map(|r| r.pass).collect::<Vec<_>>(),
            [0, 1, 0, 1, 0, 1]
        );
        let p = passes(&runs);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].ran, 3);
        let md = table(&runs, store.len());
        assert!(md.contains("| 2 | 3 |"), "{md}");
    }
}
