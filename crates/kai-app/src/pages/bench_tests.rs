//! Tests for the Benchmarks page (`bench.rs`).

use super::*;

#[test]
fn reads_the_current_format() {
    let r = parse(
        "2026-10-09-replay.json",
        r#"{"date":"2026-10-09","backend":"replay","os":"linux","note":"n",
           "tasks":[{"id":"a","title":"A","outcome":"success","pass":true,"judged_by":"check",
                     "steps":2,"seconds":0.47,"step_p50_ms":64,"step_p95_ms":64}],
           "success_rate":1.0,"step_p50_ms":64,"step_p95_ms":66,"total_seconds":3.5}"#,
    )
    .unwrap();
    assert_eq!(r.backend, "replay");
    assert_eq!(r.step_p95_ms, Some(66.0));
    assert_eq!(r.tasks[0].pass, Some(true));
}

#[test]
fn missing_numbers_stay_missing() {
    let r = parse(
        "x.json",
        r#"{"backend":"gateway","model":"google/gemma-4-26b-a4b-it",
           "rows":[{"task":"t1","outcome":"failed"},{"id":"t2","seconds":null}]}"#,
    )
    .unwrap();
    assert_eq!(r.name(), "gateway · google/gemma-4-26b-a4b-it");
    assert_eq!(r.success_rate, None);
    assert_eq!(r.tasks.len(), 2);
    assert_eq!(r.tasks[0].pass, Some(false));
    assert_eq!(r.tasks[1].seconds, None);
    assert_eq!(r.tasks[1].pass, None);
}

#[test]
fn the_cli_is_looked_for_by_its_platform_name() {
    assert_eq!(cli_name(true), "kai.exe");
    assert_eq!(cli_name(false), "kai");
    // Tests run from target/…/deps, where no `kai` sits beside them.
    assert!(cli().is_none_or(|p| p.is_file()));
}

#[test]
fn junk_is_skipped() {
    assert!(parse("a.json", "[1,2]").is_none());
    assert!(parse("a.json", "not json").is_none());
    let r = parse("b.json", "{}").unwrap();
    assert_eq!(r.backend, "b.json");
}
