//! Multi-action steps, accessibility-first actions, steering and shell output,
//! driven end to end with scripted replies. Model calls are counted.

mod common;
use common::*;
use kai_core::engine::{ApprovalChoice, BATCHED};
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const TYPE_THEN_ENTER: &str = r#"{"thought":"search","action":"type","target":4,"text":"gemma 4","risk":"none","expect":"query typed","then":[{"action":"key","keys":["Return"],"expect":"results listed"}]}"#;
const TYPE: &str = r#"{"thought":"search","action":"type","target":4,"text":"gemma 4","risk":"none","expect":"query typed"}"#;
const ENTER: &str =
    r#"{"thought":"go","action":"key","keys":["Return"],"risk":"none","expect":"results listed"}"#;
const DONE: &str = r#"{"thought":"done","action":"done","risk":"none"}"#;

#[tokio::test]
async fn batch_runs_follow_ups_without_model_calls() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[TYPE_THEN_ENTER, DONE]);
    let trace = engine(&client, &screen, &hands, &ap, &stop)
        .run("search for gemma 4")
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(client.count(), 2, "type+enter in one call, then done");
    assert_eq!(trace.steps.len(), 3);
    assert_eq!(trace.steps[1].backend, BATCHED);
    assert_eq!(trace.steps[1].decision.keys, vec!["enter"]);
    assert!(trace.steps[1].verified, "screen changed after the batch");
    assert_eq!(
        hands.log(),
        vec!["Type Some(4) []", "Key None [\"enter\"]"],
        "actions in order"
    );

    // Same task without batching: one call per action.
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let client = Counting::new(&[TYPE, ENTER, DONE]);
    let mut e = engine(&client, &screen, &hands, &ap, &stop);
    e.config.max_batch = 0;
    let trace = e.run("search for gemma 4").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(client.count(), 3);
    eprintln!("MEASURED type+enter task: batched 2 model calls, unbatched 3");
}

#[tokio::test]
async fn high_risk_follow_up_is_never_run_blind() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let ap = Approve(ApprovalChoice::Approve, AtomicUsize::new(0));
    let stop = flag();
    // The follow-up clicks "Delete": high risk by effect words.
    let client = Counting::new(&[
        r#"{"thought":"x","action":"click","target":1,"risk":"none","expect":"menu","then":[{"action":"click","target":5,"risk":"none"}]}"#,
        DONE,
    ]);
    let trace = engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(hands.log(), vec!["Click Some(1) []"], "Delete not clicked");
    assert_eq!(ap.1.load(Ordering::SeqCst), 0);
    assert_eq!(trace.steps.len(), 2);
}

#[tokio::test]
async fn batch_stops_when_the_target_changes() {
    let mut screen = Screen::marks();
    screen.rename_at = 1; // after the first action, #3 is no longer "Save"
    let hands = Hands::new(&screen);
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[
        r#"{"thought":"x","action":"click","target":1,"risk":"none","expect":"menu","then":[{"action":"click","target":3}]}"#,
        DONE,
    ]);
    let trace = engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(hands.log(), vec!["Click Some(1) []"]);
    assert!(
        screen.tree.load(Ordering::SeqCst) >= 1,
        "re-observed tree-only"
    );
}

#[tokio::test]
async fn batch_with_unchanged_target_runs_after_tree_check() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[
        r#"{"thought":"x","action":"click","target":1,"risk":"none","expect":"menu","then":[{"action":"click","target":3,"expect":"saved"}]}"#,
        DONE,
    ]);
    let trace = engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    assert_eq!(hands.log(), vec!["Click Some(1) []", "Click Some(3) []"]);
    assert_eq!(client.count(), 2);
    assert_eq!(trace.steps[1].path.as_deref(), Some("mouse"));
}

#[tokio::test]
async fn interrupt_stops_a_batch_midway() {
    let screen = Screen::marks();
    let stop = flag();
    let mut hands = Hands::new(&screen);
    hands.stop = Some((&stop, 1));
    let ap = approve_all();
    let client = Counting::new(&[
        r#"{"thought":"x","action":"key","keys":["tab"],"risk":"none","then":[{"action":"key","keys":["tab"]},{"action":"key","keys":["tab"]}]}"#,
    ]);
    let trace = engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    assert_eq!(trace.outcome, Some(Outcome::Stopped));
    assert_eq!(hands.log().len(), 1, "nothing acts after stop");
}

#[tokio::test]
async fn accessibility_path_is_tried_first_and_recorded() {
    let screen = Screen::marks();
    let mut hands = Hands::new(&screen);
    hands.a11y = true;
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[
        r#"{"thought":"x","action":"click","target":3,"risk":"none","expect":"saved"}"#,
        r#"{"thought":"x","action":"key","keys":["tab"],"risk":"none"}"#,
        DONE,
    ]);
    let trace = engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    assert_eq!(
        hands.log(),
        vec!["a11y Click Some(3)", "Key None [\"tab\"]"]
    );
    assert_eq!(trace.steps[0].path.as_deref(), Some("a11y"));
    assert_eq!(trace.steps[1].path.as_deref(), Some("keyboard"));
    assert_eq!(trace.steps[2].path.as_deref(), Some("none"));
}

#[tokio::test]
async fn steering_is_folded_in_and_shell_output_is_emitted() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[
        r#"{"thought":"x","action":"shell","text":"uname -a","risk":"none"}"#,
        DONE,
    ]);
    let events = Events::default();
    let steer = Once(Mutex::new(vec!["use the terminal".into()]));
    let mut e = engine(&client, &screen, &hands, &ap, &stop);
    e.sink = &events;
    e.steering = &steer;
    let trace = e.run("what kernel is this").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let seen = client.seen();
    assert!(
        seen[0]
            .text
            .contains("User says (mid-run): use the terminal")
    );
    assert!(!seen[1].text.contains("User says"), "one-shot note");
    assert!(seen[1].text.contains("ran uname -a"), "output in history");
    let ev = events.0.lock().unwrap().clone();
    assert_eq!(ev[0], "steered use the terminal");
    assert_eq!(ev[1], "output uname -a => ran uname -a");
}
