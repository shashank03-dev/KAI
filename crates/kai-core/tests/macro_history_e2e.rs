//! Macro-action history through the real engine: what the model is shown
//! once a run is longer than the raw window (`macro_history.rs`).

mod common;

use common::{Counting, Hands, Screen, approve_all, engine, flag};
use kai_core::*;

fn click(t: u32) -> String {
    format!(
        r#"{{"thought":"next","action":"click","target":{t},"risk":"none","expect":"changed"}}"#
    )
}
const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;

/// Five clicks (File, Edit, Back, Forward, Reload), then done. Returns the
/// text of every model turn.
async fn turns(cfg: Config, rename_at: usize, first: Option<String>) -> Vec<String> {
    let mut replies: Vec<String> = [1, 2, 6, 7, 8].iter().map(|t| click(*t)).collect();
    if let Some(f) = first {
        replies[0] = f;
    }
    replies.push(DONE.into());
    let refs: Vec<&str> = replies.iter().map(String::as_str).collect();
    let screen = Screen {
        rename_at,
        ..Screen::marks()
    };
    let hands = Hands::new(&screen);
    let model = Counting::new(&refs);
    let (approver, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approver, &stop);
    e.config = Config {
        tree_only: false,
        ..cfg
    };
    let trace = e.run("tidy up").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    model.seen().into_iter().map(|s| s.text).collect()
}

#[tokio::test]
async fn older_steps_become_a_named_summary() {
    let on = turns(
        Config {
            macro_history: true,
            ..Config::default()
        },
        usize::MAX,
        None,
    )
    .await;
    let last = on.last().unwrap();
    // The three oldest steps, by element name (mark numbers go stale).
    assert!(
        last.contains("Earlier: click \"File\"; click \"Edit\"; click \"Back\"."),
        "{last}"
    );
    // The last two stay raw.
    let raw = &last[last.find("Your last steps:").unwrap()..];
    assert!(
        raw.contains("click #7") && raw.contains("click #8"),
        "{raw}"
    );
    assert!(!raw.contains("click #6"), "{raw}");
    // Nothing to summarise until the run is longer than the raw window.
    assert!(on[..3].iter().all(|t| !t.contains("Earlier:")));
}

#[tokio::test]
async fn off_keeps_the_last_three_and_nothing_else() {
    let off = turns(Config::default(), usize::MAX, None).await;
    let last = off.last().unwrap();
    assert!(!last.contains("Earlier:") && !last.contains("Verified:"));
    let raw = &last[last.find("Your last steps:").unwrap()..];
    assert!(
        raw.contains("click #6") && raw.contains("click #8"),
        "{raw}"
    );
    assert!(!raw.contains("click #2"), "{raw}");
}

#[tokio::test]
async fn verified_outcomes_lead_the_summary() {
    let first = r#"{"thought":"open","action":"click","target":1,"risk":"none","expect":"menu","outcomes":[{"id":"saved","description":"the document is saved","probe":{"kind":"a11y_has","role":"button","name":"Saved!"}}]}"#;
    let on = turns(
        Config {
            macro_history: true,
            outcomes: true,
            ..Config::default()
        },
        2,
        Some(first.into()),
    )
    .await;
    let last = on.last().unwrap();
    assert!(
        last.contains("Verified: the document is saved. Earlier:"),
        "{last}"
    );
}
