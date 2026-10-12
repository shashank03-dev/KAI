//! Diagnosed recovery through the real engine (`engine/recover.rs`): two
//! steps that change nothing get a diagnosis and one change of course
//! instead of an immediate stop.

mod common;

use common::{Counting, Hands, Screen, approve_all, engine, flag};
use kai_core::failure::Category;
use kai_core::*;

const SAVE: &str =
    r#"{"thought":"click \"Save\"","action":"click","target":3,"risk":"none","expect":"saved"}"#;
const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;

async fn run(recover: bool, replies: &[&str]) -> (Trace, Vec<String>) {
    let screen = Screen::marks();
    let hands = Hands {
        frozen: true, // nothing the hands do changes the screen
        ..Hands::new(&screen)
    };
    let model = Counting::new(replies);
    let (approver, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approver, &stop);
    e.config = Config {
        recover,
        tree_only: false,
        ..Config::default()
    };
    let t = e.run("save the note").await;
    (t, model.seen().into_iter().map(|s| s.text).collect())
}

#[tokio::test]
async fn off_stops_after_two_steps_without_change() {
    let (t, calls) = run(false, &[SAVE, SAVE, SAVE, DONE]).await;
    assert_eq!(t.outcome, Some(Outcome::AskedUser));
    assert!(t.recoveries.is_empty());
    assert!(calls.iter().all(|c| !c.contains("Diagnosis:")));
}

#[tokio::test]
async fn on_diagnoses_the_repeat_and_goes_on_once() {
    let (t, calls) = run(true, &[SAVE, SAVE, SAVE, DONE]).await;
    assert_eq!(t.recoveries.len(), 1);
    assert_eq!(t.recoveries[0].category, Category::Loop);
    let note = calls
        .iter()
        .find(|c| c.contains("Diagnosis:"))
        .expect("diagnosis shown");
    assert!(note.contains("click on \"Save\" was repeated"), "{note}");
    assert!(note.contains("different route"), "{note}");
}

#[tokio::test]
async fn a_second_stall_still_stops_and_asks() {
    let (t, _) = run(true, &[SAVE, SAVE, SAVE, SAVE, SAVE, SAVE, DONE]).await;
    assert_eq!(t.outcome, Some(Outcome::AskedUser));
    assert_eq!(t.recoveries.len(), 1, "one recovery per run");
}
