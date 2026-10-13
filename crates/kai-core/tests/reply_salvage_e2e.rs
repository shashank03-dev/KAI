//! A reply cut off inside `then` runs its main action with no repair call
//! (`reply_salvage.rs`); a reply cut in the main decision is still repaired.

mod common;

use common::{Counting, Hands, Screen, approve_all, engine, flag};
use kai_core::*;

const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;

async fn calls(first: &str, then: &[&str]) -> (usize, Trace) {
    let mut replies = vec![first];
    replies.extend_from_slice(then);
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let model = Counting::new(&replies);
    let (approver, stop) = (approve_all(), flag());
    let e = engine(&model, &screen, &hands, &approver, &stop);
    let trace = e.run("open the menu").await;
    (model.count(), trace)
}

#[tokio::test]
async fn a_reply_cut_in_then_costs_no_repair_call() {
    let cut = r#"{"thought":"open it","action":"click","target":1,"risk":"none","expect":"menu","then":[{"action":"key","keys":["down"],"risk":"none"},{"action":"key","keys":["left","left","le"#;
    let (n, trace) = calls(cut, &[DONE]).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(n, 2, "the cut reply and the done, no repair in between");
    assert!(trace.errors.is_empty(), "{:?}", trace.errors);
}

#[tokio::test]
async fn a_reply_cut_in_the_main_decision_is_still_repaired() {
    let cut = r#"{"thought":"open it","action":"cli"#;
    let ok = r#"{"thought":"open it","action":"click","target":1,"risk":"none","expect":"menu"}"#;
    let (n, trace) = calls(cut, &[ok, DONE]).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(n, 3, "cut reply, repair, done");
}
