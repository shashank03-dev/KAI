//! Stats and report-merging tests.

use super::machine::AbRecord;
use super::report;
use super::runner::{Call, Run};
use super::stats::{self, Meta, Prices, Summary, percentile};
use super::tasks::parse_tasks;
use kai_core::trace::{Outcome, StepRecord, Timings, Trace};
use std::time::Duration;

fn meta(recorded: bool, prices: Option<Prices>) -> Meta {
    Meta {
        backend: "gateway".into(),
        model: "google/gemma-4-26b-a4b-it".into(),
        note: "test".into(),
        command: "kai bench --backend gateway".into(),
        machine: None,
        ab: Some(AbRecord::default()),
        prices,
        recorded,
        replay: false,
        date: "2026-10-09".into(),
    }
}

fn trace(steps: &[(u64, u64, u64)], outcome: Outcome) -> Trace {
    let mut t = Trace::new("x");
    for (i, &(capture, model, act)) in steps.iter().enumerate() {
        t.push(StepRecord {
            index: i as u32,
            decision: kai_core::Decision::parse_checked(
                r#"{"thought":"x","action":"wait","risk":"none"}"#,
            )
            .unwrap(),
            timings: Timings {
                capture_ms: capture,
                model_ms: model,
                act_ms: act,
                ..Default::default()
            },
            backend: "test".into(),
            elements: String::new(),
            screenshot: None,
            approval: None,
            verified: true,
            path: None,
            output: None,
            call: Default::default(),
            executor: None,
            effect: None,
        });
    }
    t.finish(outcome);
    t
}

fn tasks() -> Vec<super::tasks::Task> {
    parse_tasks(
        r#"
- id: a
  title: A
  prompt: a
  kind: gui
  mock: ['{"thought":"x","action":"done","risk":"none"}']
- id: b
  title: B
  prompt: b
  mock: ['{"thought":"x","action":"done","risk":"none"}']
- id: c
  title: C
  prompt: c
  mock: ['{"thought":"x","action":"done","risk":"none"}']
"#,
    )
    .unwrap()
}

fn call(p: Option<u32>, c: Option<u32>) -> Call {
    Call {
        ms: 100,
        ok: true,
        prompt_tokens: p,
        completion_tokens: c,
    }
}

#[test]
fn percentiles_are_nearest_rank() {
    assert_eq!(percentile(&[], 0.5), 0);
    assert_eq!(percentile(&[7], 0.95), 7);
    let v: Vec<u64> = (1..=100).collect();
    assert_eq!(percentile(&v, 0.5), 51); // index round(99 * 0.5) = 50
    assert_eq!(percentile(&v, 0.95), 95); // index round(99 * 0.95) = 94
    assert_eq!(stats::p50_p95(vec![30, 10, 20]), (20, 30));
}

#[test]
fn summary_breaks_down_time_tokens_cost_and_skips() {
    let t = tasks();
    let runs = vec![
        Run {
            task: t[0].clone(),
            trace: Some(trace(&[(10, 400, 5), (20, 600, 15)], Outcome::Success)),
            took: Duration::from_millis(1500),
            calls: vec![call(Some(1000), Some(50)), call(Some(1200), Some(40))],
            skipped: None,
            check: Some(true),
            setup_failed: false,
            pass: 0,
        },
        Run {
            task: t[1].clone(),
            trace: Some(trace(&[(30, 800, 25)], Outcome::Success)),
            took: Duration::from_millis(900),
            calls: vec![call(None, None)],
            skipped: None,
            check: Some(false),
            setup_failed: false,
            pass: 0,
        },
        Run {
            task: t[2].clone(),
            trace: None,
            took: Duration::ZERO,
            calls: vec![],
            skipped: Some("needs browser".into()),
            check: None,
            setup_failed: false,
            pass: 0,
        },
    ];
    let prices = Prices {
        input_per_mtok: 1.0,
        output_per_mtok: 10.0,
    };
    let s = stats::summarise(meta(false, Some(prices)), runs);
    assert_eq!((s.passed, s.total, s.skipped), (1, 2, 1));
    // Said done, check failed: labelled, counted, and shown in the report.
    assert!(s.tasks[0].failure.is_none());
    let f = s.tasks[1].failure.as_ref().unwrap();
    assert_eq!(f.category, kai_core::Category::FalseDone);
    assert_eq!(s.failures.get("false_done"), Some(&1));
    let md = super::report::single_markdown(&s);
    assert!(md.contains("| Failure |") && md.contains("Failure categories: false_done 1"));
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["tasks"][1]["failure"]["category"], "false_done");
    assert!(json["tasks"][0].get("failure").is_none());
    assert!((s.success_rate - 0.5).abs() < 1e-9);
    // b passed its own outcome but failed its check: the check wins.
    assert_eq!(s.tasks[1].judged_by, "check");
    assert!(!s.tasks[1].pass);
    assert_eq!(s.tasks[2].judged_by, "skipped");
    assert_eq!(s.tasks[2].skip_reason.as_deref(), Some("needs browser"));
    assert_eq!((s.model_p50_ms, s.model_p95_ms), (600, 800));
    assert_eq!((s.observe_p50_ms, s.observe_p95_ms), (20, 30));
    assert_eq!((s.act_p50_ms, s.act_p95_ms), (15, 25));
    assert_eq!(s.step_p50_ms, 20 + 600 + 15);
    assert_eq!(s.model_calls, 3);
    assert_eq!(s.calls_with_usage, 2);
    assert_eq!(
        (s.prompt_tokens, s.completion_tokens),
        (Some(2200), Some(90))
    );
    let cost = s.cost_usd.unwrap();
    assert!((cost - (2200.0 * 1.0 + 90.0 * 10.0) / 1e6).abs() < 1e-12);
    assert!((s.mean_model_calls_per_task - 1.5).abs() < 1e-9);
    assert!((s.mean_steps_per_task - 1.5).abs() < 1e-9);
    assert!((s.mean_seconds_per_task - 1.2).abs() < 1e-9);
    assert!(s.missing.iter().any(|m| m.contains("only 2 of 3")));
    assert_eq!(s.tasks[1].prompt_tokens, None);
}

#[test]
fn recorded_runs_say_tokens_are_not_measured() {
    let t = tasks();
    let runs = vec![Run {
        task: t[1].clone(),
        trace: Some(trace(&[(1, 0, 1)], Outcome::Success)),
        took: Duration::from_millis(5),
        calls: vec![call(Some(100), Some(20))],
        skipped: None,
        check: None,
        setup_failed: false,
        pass: 0,
    }];
    let s = stats::summarise(meta(true, None), runs);
    assert_eq!(s.tasks[0].judged_by, "outcome");
    assert!(s.tasks[0].pass);
    assert_eq!(s.cost_usd, None);
    assert_eq!(s.prompt_tokens, None);
    assert_eq!(s.tasks[0].prompt_tokens, None);
    assert!(s.missing.iter().any(|m| m.contains("recorded replies")));
}

fn summary(backend: &str, model: &str, date: &str, pass: bool) -> Summary {
    let t = tasks();
    let runs = vec![Run {
        task: t[0].clone(),
        trace: Some(trace(&[(10, 100, 10)], Outcome::Success)),
        took: Duration::from_millis(200),
        calls: vec![call(Some(10), Some(2))],
        skipped: None,
        check: Some(pass),
        setup_failed: false,
        pass: 0,
    }];
    let mut m = meta(false, None);
    m.backend = backend.into();
    m.model = model.into();
    m.date = date.into();
    stats::summarise(m, runs)
}

#[test]
fn file_names_slug_the_model() {
    let s = summary("gateway", "google/gemma-4-26b-a4b-it", "2026-10-09", true);
    assert_eq!(
        report::file_stem(&s),
        "2026-10-09-gateway-google-gemma-4-26b-a4b-it"
    );
    assert_eq!(report::slug("openai/GPT 5 mini"), "openai-gpt-5-mini");
}

#[test]
fn report_merges_latest_per_backend_and_model_from_files_only() {
    let dir = tempfile::tempdir().unwrap();
    for s in [
        summary("gateway", "google/gemma-4-26b-a4b-it", "2026-10-08", false),
        summary("gateway", "google/gemma-4-26b-a4b-it", "2026-10-09", true),
        summary("gateway", "openai/gpt-5-mini", "2026-10-09", false),
    ] {
        report::write(&s, dir.path()).unwrap();
    }
    // An old-format file (no model, fewer fields) still loads.
    std::fs::write(
        dir.path().join("2026-10-01-mock.json"),
        r#"{"date":"2026-10-01","backend":"mock","os":"linux","note":"old","tasks":[{"id":"a","title":"A","outcome":"success","pass":true,"judged_by":"outcome","steps":2,"seconds":0.1,"step_p50_ms":1,"step_p95_ms":2}],"success_rate":1.0,"passed":1,"total":1,"step_p50_ms":1,"step_p95_ms":2,"total_seconds":0.1}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("junk.json"), "{not json").unwrap();
    std::fs::write(dir.path().join("other.json"), r#"{"hello":1}"#).unwrap();

    let runs = report::load_latest(dir.path()).unwrap();
    assert_eq!(runs.len(), 3, "gemma (latest only), gpt-5-mini, old mock");
    let gemma = runs
        .iter()
        .find(|(_, s)| s.model == "google/gemma-4-26b-a4b-it")
        .unwrap();
    assert_eq!(gemma.1.date, "2026-10-09");

    let md = report::merged_markdown(&runs);
    assert!(md.contains(
        "| | gateway · google/gemma-4-26b-a4b-it | gateway · openai/gpt-5-mini | mock |"
    ));
    assert!(md.contains("| Tasks passed | 1 / 1 (100%) | 0 / 1 (0%) | 1 / 1 (100%) |"));
    assert!(md.contains("| a | gui | pass (1 steps, 0.2 s) | FAIL (1 steps, 0.2 s) |"));
    // The old file had no tokens: shown as not measured, never zero.
    assert!(md.contains("| Prompt / completion tokens | 10 / 2 | 10 / 2 | — / — |"));
    assert!(!md.contains("junk"));
}

#[test]
fn empty_report_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let runs = report::load_latest(dir.path()).unwrap();
    assert!(report::merged_markdown(&runs).contains("No results files found."));
}

#[test]
fn single_markdown_lists_tasks_and_missing() {
    let s = summary("gateway", "openai/gpt-5-mini", "2026-10-09", true);
    let md = report::single_markdown(&s);
    assert!(md.starts_with("# Kai bench: gateway · openai/gpt-5-mini"));
    assert!(md.contains("Command: `kai bench --backend gateway`"));
    assert!(md.contains("| a | Gui | pass | check |"));
    assert!(md.contains("cost: no prices given"));
}
