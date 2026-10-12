use super::*;
use crate::trace::{StepRecord, Timings};

fn d(raw: &str) -> Decision {
    Decision::parse_checked(raw).unwrap()
}

fn trace(steps: &[&str], outcome: Outcome) -> Trace {
    let mut t = Trace::new("task");
    for (i, s) in steps.iter().enumerate() {
        t.push(StepRecord::new(i as u32, &d(s), Timings::default(), "Mock"));
    }
    t.finish(outcome);
    t
}

const DONE: &str = r#"{"thought":"all done","action":"done","risk":"none"}"#;
const CLICK3: &str = r#"{"thought":"click Save","action":"click","target":3,"risk":"none"}"#;
const SHELL: &str = r#"{"thought":"x","action":"shell","text":"ls","risk":"none"}"#;
const ASK: &str = r#"{"thought":"which file do you mean?","action":"ask_user","risk":"none"}"#;
const WAIT: &str = r#"{"thought":"x","action":"wait","risk":"none"}"#;

fn cat(t: &Trace, check: Option<bool>) -> Option<Category> {
    classify(t, check).map(|f| f.category)
}

#[test]
fn passing_runs_have_no_failure() {
    let t = trace(&[SHELL, DONE], Outcome::Success);
    assert_eq!(classify(&t, None), None);
    assert_eq!(classify(&t, Some(true)), None);
    // A check that passes overrides a bad outcome.
    let t = trace(&[SHELL], Outcome::StepLimit);
    assert_eq!(classify(&t, Some(true)), None);
}

#[test]
fn done_with_a_failed_check_is_false_done() {
    let t = trace(&[SHELL, DONE], Outcome::Success);
    let f = classify(&t, Some(false)).unwrap();
    assert_eq!(f.category, Category::FalseDone);
    assert!(f.detail.contains("all done"));
}

#[test]
fn three_identical_actions_are_a_loop() {
    let t = trace(&[CLICK3, CLICK3, SHELL, CLICK3], Outcome::StepLimit);
    let f = classify(&t, Some(false)).unwrap();
    assert_eq!(f.category, Category::Loop);
    assert!(f.detail.contains("#3"), "{}", f.detail);
    assert!(f.detail.contains("3 times"), "{}", f.detail);
    // Twice is not a loop; waits never are.
    let t = trace(&[CLICK3, CLICK3, WAIT, WAIT, WAIT], Outcome::StepLimit);
    assert_eq!(cat(&t, None), Some(Category::StepLimit));
}

#[test]
fn limits_and_asking() {
    assert_eq!(
        cat(&trace(&[SHELL], Outcome::TimeLimit), None),
        Some(Category::TimeLimit)
    );
    let f = classify(&trace(&[SHELL, ASK], Outcome::AskedUser), None).unwrap();
    assert_eq!(f.category, Category::GaveUp);
    assert!(f.detail.contains("which file"));
    assert_eq!(
        cat(&trace(&[SHELL], Outcome::AskedUser), None),
        Some(Category::GaveUp)
    );
    assert_eq!(
        cat(&trace(&[SHELL], Outcome::Stopped), None),
        Some(Category::Other)
    );
}

#[test]
fn malformed_and_rate_limited_come_from_errors() {
    let mut t = trace(&[SHELL], Outcome::AskedUser);
    t.errors
        .push("malformed reply at step 1: expected value".into());
    assert_eq!(cat(&t, None), Some(Category::GaveUp), "one is not enough");
    t.errors
        .push("malformed reply at step 2: missing field".into());
    assert_eq!(cat(&t, None), Some(Category::Malformed));
    t.errors
        .push("model error at step 3: backend returned status 429: slow down".into());
    assert_eq!(cat(&t, None), Some(Category::RateLimited));
}

#[test]
fn act_errors_and_shell_timeouts_are_env_errors() {
    let mut t = trace(&[CLICK3, SHELL], Outcome::Failed);
    t.steps[0].path = Some("error".into());
    t.steps[0].output = Some("ERROR: xdotool missing".into());
    let f = classify(&t, Some(false)).unwrap();
    assert_eq!(f.category, Category::EnvError);
    assert!(f.detail.contains("xdotool"));

    let mut t = trace(&[SHELL], Outcome::StepLimit);
    t.steps[0].output = Some("command timed out after 60 s".into());
    assert_eq!(cat(&t, None), Some(Category::EnvError));
}

#[test]
fn clicking_a_mark_that_does_not_match_the_thought_is_a_grounding_miss() {
    let raw =
        r#"{"thought":"click the \"Save\" button","action":"click","target":3,"risk":"none"}"#;
    let mut t = trace(&[raw], Outcome::Failed);
    t.steps[0].elements = "#2 button \"Save\" (enabled)\n#3 button \"Delete\" (enabled)".into();
    t.steps[0].verified = false;
    let f = classify(&t, Some(false)).unwrap();
    assert_eq!(f.category, Category::GroundingMiss);
    assert!(f.detail.contains("delete"), "{}", f.detail);
    // The right mark is not a miss.
    t.steps[0].elements = "#3 button \"Save\" (enabled)".into();
    assert_eq!(cat(&t, Some(false)), Some(Category::Other));
}

#[test]
fn categories_serialise_snake_case() {
    let f = Failure::new(Category::FalseDone, "x");
    let j = serde_json::to_string(&f).unwrap();
    assert_eq!(j, r#"{"category":"false_done","detail":"x"}"#);
    assert_eq!(Category::GroundingMiss.as_str(), "grounding_miss");
}

#[test]
fn a_gate_that_never_verified_is_its_own_category() {
    use crate::ledger::{GateRecord, Ledger};
    let mut t = trace(&[CLICK3, DONE, CLICK3, DONE], Outcome::AskedUser);
    t.outcomes = Ledger::from_raw(&[serde_json::json!({"id":"sent","description":"sent",
        "probe":{"kind":"window_title","pattern":"thank you"}})])
    .records(&[]);
    t.gate.push(GateRecord {
        step: 1,
        kind: "refused".into(),
        failed: Vec::new(),
        last_effect: None,
    });
    let f = classify(&t, Some(false)).unwrap();
    assert_eq!(f.category, Category::GatedNeverVerified);
    assert!(f.detail.contains("sent"), "{}", f.detail);
    // The gate accepted (or was off) and the check failed: still false done.
    let mut t = trace(&[SHELL, DONE], Outcome::Success);
    t.gate.clear();
    assert_eq!(cat(&t, Some(false)), Some(Category::FalseDone));
}
