//! `--executor` through the mock bench: the counts reach the results file.

use super::machine::AbRecord;
use super::report;
use super::stats::{self, Meta};
use super::tasks::parse_tasks;
use kai_core::client::{Answer, Question};
use std::collections::BTreeMap;

/// Always confident: the first offered target, ready, not yet done, met.
struct SureJevos;

#[async_trait::async_trait]
impl kai_core::executor::Decider for SureJevos {
    async fn ask(
        &self,
        _state: &serde_json::Value,
        qs: &[(String, Question)],
    ) -> Result<BTreeMap<String, Answer>, kai_core::ClientError> {
        Ok(qs
            .iter()
            .map(|(id, q)| {
                let a = match q {
                    Question::Criteria { criteria, .. } => Answer::Choice {
                        option: criteria[0].0.clone(),
                        probabilities: BTreeMap::new(),
                        confidence: Some(0.9),
                    },
                    _ if id == "already_done" => Answer::Noul(0.05),
                    _ => Answer::Noul(0.95),
                };
                (id.clone(), a)
            })
            .collect())
    }
}

#[tokio::test]
async fn executor_counts_reach_the_results_file() {
    let t = parse_tasks(
        r#"
- id: form
  title: Form
  prompt: fill the form
  mock:
    - '{"thought":"name","action":"type","target":5,"text":"Ada","risk":"none","expect":"typed","plan":[{"do":"type","target":"Email field","text":"a@b.c","expect":"Email holds a@b.c"},{"do":"done","target":"","expect":""}]}'
    - '{"thought":"done","action":"done","risk":"none"}'
"#,
    )
    .unwrap();
    let config = kai_core::Config {
        executor: true,
        ..Default::default()
    };
    let runs = super::runner::run_mock(&t, &[], config, None, Some(&SureJevos)).await;
    assert_eq!(runs[0].calls.len(), 2, "plan + done");
    let m = Meta {
        backend: "mock".into(),
        model: "recorded".into(),
        note: "test".into(),
        command: "kai bench --executor".into(),
        machine: None,
        ab: Some(AbRecord {
            flags: crate::ab::Ab {
                executor: true,
                ..Default::default()
            },
            applied: true,
        }),
        prices: None,
        recorded: true,
        replay: false,
        date: "2026-10-10".into(),
    };
    let s = stats::summarise(m, runs);
    assert!(s.executor);
    assert_eq!(s.executor_available_tasks, 1);
    assert_eq!((s.executor_steps, s.executor_escalations), (1, 0));
    // One grounding request plus the final check of the last subgoal.
    assert_eq!(s.executor_decisions, 2);
    assert_eq!(s.tasks[0].executor_steps, 1);
    let md = report::single_markdown(&s);
    assert!(md.contains("on · 2 decisions · 0 escalations"), "{md}");
    assert!(md.contains("--executor"));
}
