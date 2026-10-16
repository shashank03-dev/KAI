//! Procedures through the real engine (`docs/PROCEDURES.md`): recorded from
//! model-led runs, replayed with no model call and new specifics, handed
//! back to the model when the screen does not match, quarantined on failure.

mod common;

use common::{Counting, Hands, Screen, approve_all, engine, flag};
use kai_core::engine::PROCEDURE;
use kai_core::procedure::{ProcedureStore, Status};
use kai_core::*;

const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;
const ASK: &str = r#"{"thought":"unsure","action":"ask_user","text":"which one?","risk":"none"}"#;

/// Click Save (#3), then type the address into Search (#4).
fn model_led(addr: &str) -> Vec<String> {
    vec![
        r#"{"thought":"save","action":"click","target":3,"risk":"none","expect":"saved"}"#.into(),
        format!(
            r#"{{"thought":"search","action":"type","target":4,"text":"{addr}","risk":"none","expect":"typed"}}"#
        ),
        DONE.into(),
    ]
}

struct Ran {
    trace: Trace,
    calls: Vec<String>,
    acted: Vec<String>,
}

async fn run(store: &ProcedureStore, task: &str, replies: &[String], screen: Screen) -> Ran {
    let refs: Vec<&str> = replies.iter().map(String::as_str).collect();
    let hands = Hands::new(&screen);
    let model = Counting::new(&refs);
    let (approver, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approver, &stop);
    e.config = Config {
        procedures: true,
        tree_only: false,
        ..Config::default()
    };
    let trace = e
        .run_with(
            task,
            RunOptions {
                procedures: Some(store),
                ..Default::default()
            },
        )
        .await;
    Ran {
        trace,
        calls: model.seen().into_iter().map(|s| s.text).collect(),
        acted: hands.log(),
    }
}

/// Two model-led successes make the procedure active.
async fn learned() -> ProcedureStore {
    let store = ProcedureStore::in_memory();
    for _ in 0..2 {
        let r = run(
            &store,
            "find alpha@x.io",
            &model_led("alpha@x.io"),
            Screen::marks(),
        )
        .await;
        assert_eq!(r.trace.outcome, Some(Outcome::Success));
        assert_eq!(r.calls.len(), 3, "model-led");
    }
    assert_eq!(store.list()[0].status, Status::Active);
    store
}

#[tokio::test]
async fn a_known_task_replays_with_new_specifics_and_one_model_call() {
    let store = learned().await;
    let r = run(&store, "find beta@y.io", &[DONE.into()], Screen::marks()).await;
    assert_eq!(r.trace.outcome, Some(Outcome::Success));
    // Only the final check is a model call.
    assert_eq!(r.calls.len(), 1);
    assert!(
        r.calls[0].contains("All 2 steps of a remembered procedure"),
        "{}",
        r.calls[0]
    );
    let replayed: Vec<_> = r
        .trace
        .steps
        .iter()
        .filter(|s| s.backend == PROCEDURE)
        .collect();
    assert_eq!(replayed.len(), 2);
    // Targets found again by name; the new address typed.
    assert_eq!(replayed[0].decision.target, Some(3));
    assert_eq!(replayed[1].decision.text.as_deref(), Some("beta@y.io"));
    assert_eq!(r.acted.len(), 2);
    assert_eq!(store.list()[0].successes, 3);
}

#[tokio::test]
async fn a_missing_target_hands_over_to_the_model() {
    let store = learned().await;
    // "Save" is already "Saved!": the first step cannot be found.
    let screen = Screen {
        rename_at: 0,
        ..Screen::marks()
    };
    let r = run(&store, "find beta@y.io", &[DONE.into()], screen).await;
    assert!(r.acted.is_empty(), "nothing ran blind: {:?}", r.acted);
    let note = &r.calls[0];
    assert!(note.contains("0 of 2 steps"), "{note}");
    assert!(note.contains("button \"Save\" is not on screen"), "{note}");
    assert!(
        note.contains("beta@y.io"),
        "the hint carries the new specifics: {note}"
    );
}

#[tokio::test]
async fn a_replay_that_fails_is_quarantined() {
    let store = learned().await;
    let r = run(&store, "find beta@y.io", &[ASK.into()], Screen::marks()).await;
    assert_eq!(r.trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(store.list()[0].status, Status::Quarantined);
    // A quarantined procedure no longer replays.
    let again = run(
        &store,
        "find beta@y.io",
        &model_led("beta@y.io"),
        Screen::marks(),
    )
    .await;
    assert!(again.trace.steps.iter().all(|s| s.backend != PROCEDURE));
}

#[tokio::test]
async fn different_words_never_replay() {
    let store = learned().await;
    let r = run(
        &store,
        "delete alpha@x.io",
        &model_led("alpha@x.io"),
        Screen::marks(),
    )
    .await;
    assert!(r.trace.steps.iter().all(|s| s.backend != PROCEDURE));
    assert_eq!(r.calls.len(), 3);
}

#[tokio::test]
async fn off_records_and_replays_nothing() {
    let store = ProcedureStore::in_memory();
    let screen = Screen::marks();
    let replies = model_led("alpha@x.io");
    let refs: Vec<&str> = replies.iter().map(String::as_str).collect();
    let hands = Hands::new(&screen);
    let model = Counting::new(&refs);
    let (approver, stop) = (approve_all(), flag());
    let e = engine(&model, &screen, &hands, &approver, &stop);
    let opts = RunOptions {
        procedures: Some(&store),
        ..Default::default()
    };
    e.run_with("find alpha@x.io", opts).await;
    assert!(store.list().is_empty());
}

/// Hands whose every click fails with a clear OS-level reason.
struct Broken;
#[async_trait::async_trait]
impl Actuator for Broken {
    async fn act(&self, _: &Decision, _: &Grounding) -> anyhow::Result<()> {
        anyhow::bail!("the Save button vanished")
    }
    async fn open_app(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn open_url(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn shell(&self, _: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
}

#[tokio::test]
async fn a_failed_replay_step_is_recorded_and_explained_to_the_model() {
    let store = learned().await;
    let screen = Screen::marks();
    let model = Counting::new(&[DONE]);
    let (approver, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &Broken, &approver, &stop);
    e.config = Config {
        procedures: true,
        tree_only: false,
        ..Config::default()
    };
    let trace = e
        .run_with(
            "find beta@y.io",
            RunOptions {
                procedures: Some(&store),
                ..Default::default()
            },
        )
        .await;
    let failed = &trace.steps[0];
    assert_eq!(failed.path.as_deref(), Some("error"));
    assert!(
        failed
            .output
            .as_deref()
            .unwrap()
            .contains("Save button vanished")
    );
    let first = &model.seen()[0].text;
    assert!(
        first.contains("step failed (ERROR: the Save button vanished)"),
        "{first}"
    );
}

#[tokio::test]
async fn a_replay_stops_at_the_time_limit() {
    let store = learned().await;
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let model = Counting::new(&[DONE]);
    let (approver, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approver, &stop);
    e.config = Config {
        procedures: true,
        tree_only: false,
        max_time: std::time::Duration::ZERO,
        ..Config::default()
    };
    let trace = e
        .run_with(
            "find beta@y.io",
            RunOptions {
                procedures: Some(&store),
                ..Default::default()
            },
        )
        .await;
    assert_eq!(trace.outcome, Some(Outcome::TimeLimit));
    assert!(hands.log().is_empty(), "no replay step after the deadline");
    assert!(model.seen().is_empty());
}
