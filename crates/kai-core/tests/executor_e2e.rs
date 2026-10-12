//! The executor end to end: Gemma (scripted replies, counted) plans, a fake
//! jevos grounds and gates, the engine acts with approvals unchanged.

mod common;
mod executor_support;

use common::{Approve, Counting, Once, flag};
use executor_support::{FakeJevos, Form, FormHands};
use kai_core::engine::{ApprovalChoice, EXECUTOR, NullSink};
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const DONE: &str = r#"{"thought":"done","action":"done","risk":"none"}"#;

// Task A: fill the contact form and send it.
const A_PLAN: &str = r#"{"thought":"fill form","action":"type","target":3,"text":"Ada","risk":"none","expect":"Name holds Ada",
 "plan":[{"do":"type","target":"Email field","text":"ada@x.org","expect":"Email holds ada@x.org"},
         {"do":"type","target":"Message field","text":"Hello","expect":"Message holds Hello"},
         {"do":"click","target":"Send message button","expect":"message sent"},
         {"do":"done","target":"","expect":""}]}"#;
const A_OFF: &[&str] = &[
    r#"{"thought":"name","action":"type","target":3,"text":"Ada","risk":"none","expect":"Name holds Ada"}"#,
    r#"{"thought":"email","action":"type","target":4,"text":"ada@x.org","risk":"none","expect":"Email holds it"}"#,
    r#"{"thought":"msg","action":"type","target":5,"text":"Hello","risk":"none","expect":"Message holds it"}"#,
    r#"{"thought":"send","action":"click","target":7,"risk":"high","expect":"message sent"}"#,
    DONE,
];

// Task B: search and open the result.
const B_PLAN: &str = r#"{"thought":"open site","action":"open_url","text":"https://example.org","risk":"none","expect":"site open",
 "plan":[{"do":"type","target":"Search box","text":"gemma 4","expect":"Search holds gemma 4"},
         {"do":"key","target":"","keys":["Return"],"expect":"results listed"},
         {"do":"click","target":"Gemma 4 model card link","expect":"model card open"},
         {"do":"done","target":"","expect":""}]}"#;
const B_OFF: &[&str] = &[
    r#"{"thought":"open","action":"open_url","text":"https://example.org","risk":"none","expect":"site open"}"#,
    r#"{"thought":"q","action":"type","target":2,"text":"gemma 4","risk":"none","expect":"typed"}"#,
    r#"{"thought":"go","action":"key","keys":["Return"],"risk":"none","expect":"results"}"#,
    r#"{"thought":"open","action":"click","target":10,"risk":"none","expect":"card open"}"#,
    DONE,
];

struct Out {
    trace: Trace,
    calls: usize,
    texts: Vec<String>,
    actions: Vec<String>,
    approvals: usize,
}

async fn run(
    replies: &[&str],
    jevos: Option<&FakeJevos>,
    approver: ApprovalChoice,
    level: ApprovalLevel,
    steering: &dyn kai_core::engine::Steering,
) -> Out {
    let form = Form::default();
    let hands = FormHands::new(&form);
    let client = Counting::new(replies);
    let ap = Approve(approver, AtomicUsize::new(0));
    let stop = flag();
    let mut config = Config {
        approval: level,
        ..Config::default()
    };
    config.executor = jevos.is_some();
    let engine = Engine {
        client: &client,
        careful: None,
        perceptor: &form,
        actuator: &hands,
        approver: &ap,
        interrupt: &stop,
        sink: &NullSink,
        steering,
        verifier: None,
        human: None,
        skills: &[],
        config,
    };
    let opts = RunOptions {
        executor: jevos.map(|j| j as &dyn kai_core::executor::Decider),
        ..Default::default()
    };
    let trace = engine.run_with("fill the form", opts).await;
    Out {
        calls: client.count(),
        texts: client.seen().into_iter().map(|s| s.text).collect(),
        actions: hands.log(),
        approvals: ap.1.load(Ordering::SeqCst),
        trace,
    }
}

async fn plain(replies: &[&str], jevos: Option<&FakeJevos>) -> Out {
    let none = kai_core::engine::NoSteering;
    run(
        replies,
        jevos,
        ApprovalChoice::Approve,
        ApprovalLevel::HighRiskOnly,
        &none,
    )
    .await
}

#[tokio::test]
async fn executor_carries_out_a_confident_plan_with_approvals_unchanged() {
    let j = FakeJevos::confident();
    let o = plain(&[A_PLAN, DONE], Some(&j)).await;
    assert_eq!(o.trace.outcome, Some(Outcome::Success));
    assert_eq!(o.calls, 2, "plan + confirm done");
    assert_eq!(
        o.actions,
        vec![
            "type #3 Ada",
            "type #4 ada@x.org",
            "type #5 Hello",
            "click #7"
        ]
    );
    assert_eq!(o.approvals, 1, "sending still asks, like a model step");
    let ex: Vec<&trace::StepRecord> = o
        .trace
        .steps
        .iter()
        .filter(|s| s.backend == EXECUTOR)
        .collect();
    assert_eq!(ex.len(), 3);
    assert_eq!(ex[2].approval.as_deref(), Some("approved"));
    let x = ex[0].executor.as_ref().unwrap();
    assert_eq!(x.target_p, Some(0.9));
    assert!(x.ready.unwrap() > 0.9 && x.margin.unwrap() > 0.8);
    assert_eq!(x.expect_met, Some(0.92), "checked by the next request");
    assert!(ex.iter().all(|s| s.verified));
    let st = o.trace.executor.as_ref().unwrap();
    assert!(st.enabled);
    assert_eq!((st.plans, st.acted, st.escalations), (1, 3, 0));
    // 3 grounding requests + 1 final check of the last subgoal.
    assert_eq!(st.decisions, 4);
    assert_eq!(j.count(), 4);
    assert!(o.texts[1].contains("ends with `done`"));
}

#[tokio::test]
async fn measured_model_calls_executor_on_vs_off() {
    let mut lines = Vec::new();
    for (name, on, off) in [("contact form", A_PLAN, A_OFF), ("search", B_PLAN, B_OFF)] {
        let j = FakeJevos::confident();
        let with = plain(&[on, DONE], Some(&j)).await;
        let without = plain(off, None).await;
        assert_eq!(with.trace.outcome, Some(Outcome::Success));
        assert_eq!(without.trace.outcome, Some(Outcome::Success));
        assert_eq!(with.actions, without.actions, "{name}: same actions");
        assert!(with.calls < without.calls);
        lines.push(format!(
            "MEASURED executor e2e ({name}, mock Gemma + fake jevos): model calls executor on {} vs off {} ({} jevos requests, {} actions)",
            with.calls,
            without.calls,
            j.count(),
            with.actions.len()
        ));
    }
    for l in lines {
        eprintln!("{l}");
    }
}

#[tokio::test]
async fn unsure_executor_hands_back_and_pauses_after_two_escalations() {
    let mut j = FakeJevos::confident();
    j.target_p = 0.55;
    j.second_p = 0.4;
    let replan = r#"{"thought":"email","action":"type","target":4,"text":"ada@x.org","risk":"none","expect":"Email holds it",
      "plan":[{"do":"type","target":"Message field","text":"Hello","expect":"Message holds Hello"}]}"#;
    let replan2 = r#"{"thought":"msg","action":"type","target":5,"text":"Hello","risk":"none","expect":"Message holds it",
      "plan":[{"do":"click","target":"Send message button","expect":"sent"}]}"#;
    let o = plain(&[A_PLAN, replan, replan2, A_OFF[3], DONE], Some(&j)).await;
    assert_eq!(o.trace.outcome, Some(Outcome::Success));
    assert_eq!(o.calls, 5);
    assert!(o.trace.steps.iter().all(|s| s.backend != EXECUTOR));
    assert!(o.texts[1].contains("Executor was unsure about 'type 'ada@x.org' into Email field'"));
    let st = o.trace.executor.as_ref().unwrap();
    assert_eq!(st.escalations, 2);
    // The third plan fell in the cooldown: never asked.
    assert_eq!(st.plans, 2);
    assert_eq!(j.count(), 2);
}

#[tokio::test]
async fn not_ready_escalates_and_already_done_never_skips_by_default() {
    let mut j = FakeJevos::confident();
    j.ready = 0.5;
    let o = plain(&[A_PLAN, DONE], Some(&j)).await;
    assert_eq!(o.actions, vec!["type #3 Ada"]);
    assert!(o.texts[1].contains("not ready"));

    let mut j = FakeJevos::confident();
    j.done = 0.9;
    let o = plain(&[A_PLAN, DONE], Some(&j)).await;
    // jevos' "already done" was measured unreliable, so it never skips work
    // by default: the subgoals still run.
    let st = o.trace.executor.as_ref().unwrap();
    assert_eq!(st.skipped, 0);
    assert!(st.acted >= 3, "{st:?}");
}

#[tokio::test]
async fn skipped_approval_stops_the_plan_and_tells_the_model() {
    let j = FakeJevos::confident();
    let none = kai_core::engine::NoSteering;
    let o = run(
        &[A_PLAN, DONE],
        Some(&j),
        ApprovalChoice::Skip,
        ApprovalLevel::HighRiskOnly,
        &none,
    )
    .await;
    assert!(
        !o.actions.contains(&"click #7".to_string()),
        "Send never clicked"
    );
    assert_eq!(o.approvals, 1);
    assert!(o.texts[1].contains("skipped 'click Send message button'"));
}

#[tokio::test]
async fn secure_field_always_asks_even_on_autopilot() {
    let j = FakeJevos::confident();
    let plan = r#"{"thought":"login","action":"type","target":3,"text":"ada","risk":"none","expect":"Name holds ada",
      "plan":[{"do":"type","target":"Password field","text":"hunter2","expect":"password typed"}]}"#;
    let none = kai_core::engine::NoSteering;
    let o = run(
        &[plan, DONE],
        Some(&j),
        ApprovalChoice::Skip,
        ApprovalLevel::Autopilot,
        &none,
    )
    .await;
    assert_eq!(o.approvals, 1, "password typing asks on autopilot");
    assert_eq!(
        o.actions,
        vec!["type #3 ada"],
        "never typed without approval"
    );
}

#[tokio::test]
async fn steering_or_unreachable_executor_falls_back_to_the_model() {
    // A steering message arriving mid-run ends the plan.
    let j = FakeJevos::confident();
    let steer = Once(Mutex::new(Vec::new()));
    struct Late<'a>(&'a Once, AtomicUsize);
    impl kai_core::engine::Steering for Late<'_> {
        fn take(&self) -> Vec<String> {
            // Nothing on the model step's drain; a message on the executor's.
            if self.1.fetch_add(1, Ordering::SeqCst) == 1 {
                vec!["use my work email".into()]
            } else {
                self.0.take()
            }
        }
    }
    let late = Late(&steer, AtomicUsize::new(0));
    let o = run(
        &[A_PLAN, DONE],
        Some(&j),
        ApprovalChoice::Approve,
        ApprovalLevel::HighRiskOnly,
        &late,
    )
    .await;
    assert_eq!(o.actions, vec!["type #3 Ada"]);
    assert!(o.texts[1].contains("use my work email"));
    assert_eq!(j.count(), 0);

    // Health check fails: no executor, the plan is ignored, nothing breaks.
    let mut j = FakeJevos::confident();
    j.healthy = false;
    let o = plain(A_OFF, Some(&j)).await;
    assert_eq!(o.trace.outcome, Some(Outcome::Success));
    assert_eq!(o.calls, 5);
    let st = o.trace.executor.as_ref().unwrap();
    assert!(!st.enabled && st.unavailable.is_some());
    assert_eq!(j.count(), 0);
}
