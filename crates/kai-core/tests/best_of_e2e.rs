//! Plan-level best-of-N through the real engine (`best_of.rs`).

mod common;

use common::{Counting, Hands, Screen, approve_all, engine, flag};
use kai_core::*;

const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;
const CLICK: &str =
    r#"{"thought":"save","action":"click","target":3,"risk":"none","expect":"saved"}"#;
const LONG: &str = "Open the notes file, then save it";

async fn run(task: &str, best_of: u32, replies: &[&str]) -> (Trace, Vec<String>) {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let model = Counting::new(replies);
    let (approver, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approver, &stop);
    e.config = Config {
        best_of,
        tree_only: false,
        ..Config::default()
    };
    let trace = e.run(task).await;
    (trace, model.seen().into_iter().map(|s| s.text).collect())
}

#[tokio::test]
async fn a_long_task_samples_plans_and_follows_the_agreed_one() {
    let replies = [
        r#"{"plan":["open the terminal","run a script","check output"]}"#,
        r#"{"plan":["open the notes file","click Save"]}"#,
        r#"{"plan":["open notes file in the editor","press Save"]}"#,
        CLICK,
        DONE,
    ];
    let (trace, calls) = run(LONG, 3, &replies).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let b = trace.best_of.expect("plans recorded");
    assert_eq!((b.plans.len(), b.calls), (3, 3));
    assert_ne!(b.chosen, 0, "the outlier plan lost: {:?}", b.scores);
    // The three planning calls come first, then the normal steps, each
    // carrying the chosen plan.
    assert!(
        calls[..3]
            .iter()
            .all(|c| c.contains("Reply with the plan as JSON"))
    );
    for c in &calls[3..] {
        assert!(
            c.contains("Plan for this task (a guide; the screen decides): 1."),
            "{c}"
        );
        assert!(!c.contains("run a script"), "{c}");
    }
}

#[tokio::test]
async fn short_tasks_and_off_make_no_planning_calls() {
    let (t, calls) = run("save it", 3, &[CLICK, DONE]).await;
    assert!(t.best_of.is_none() && calls.len() == 2);
    let (t, calls) = run(LONG, 0, &[CLICK, DONE]).await;
    assert!(t.best_of.is_none() && calls.len() == 2);
    assert!(calls.iter().all(|c| !c.contains("Plan for this task")));
}

#[tokio::test]
async fn unusable_plans_mean_no_plan_not_a_failed_run() {
    let replies = ["no idea", r#"{"plan":["one step"]}"#, "{}", CLICK, DONE];
    let (t, calls) = run(LONG, 3, &replies).await;
    assert_eq!(t.outcome, Some(Outcome::Success));
    assert!(t.best_of.is_none());
    assert!(calls[3..].iter().all(|c| !c.contains("Plan for this task")));
}
