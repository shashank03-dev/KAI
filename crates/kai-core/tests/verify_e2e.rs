//! The outcome ledger and DONE gate end to end through the real engine:
//! scripted model, fake screen and hands, a fake probe port, a scripted
//! person. See `docs/VERIFICATION.md`; every guarantee there names its test.

mod common;
mod verify_support;

use common::{Counting, Hands, Screen, approve_all, flag};
use kai_core::engine::{Engine, NoSteering};
use kai_core::human::{HumanHelper, HumanReply, OneTimeCode};
use kai_core::ledger::{OUTCOME_REQUEST, Status, VerifiedBy};
use kai_core::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use verify_support::*;

const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;
const CLICK1: &str =
    r#"{"thought":"open file","action":"click","target":1,"risk":"none","expect":"menu"}"#;
const CLICK2: &str =
    r#"{"thought":"edit","action":"click","target":2,"risk":"none","expect":"menu"}"#;
const CLICK3: &str =
    r#"{"thought":"save","action":"click","target":3,"risk":"none","expect":"saved"}"#;

/// Click #1 and name one outcome: the Save button reads "Saved!".
fn first_with(outcome: &str) -> String {
    format!(
        r#"{{"thought":"open file","action":"click","target":1,"risk":"none","expect":"menu","outcomes":[{outcome}]}}"#
    )
}
const SAVED: &str = r#"{"id":"saved","description":"the document is saved","probe":{"kind":"a11y_has","role":"button","name":"Saved!"}}"#;

fn config() -> Config {
    Config {
        outcomes: true,
        tree_only: false,
        ..Config::default()
    }
}

fn engine<'a>(
    client: &'a Counting,
    perceptor: &'a dyn engine::Perceptor,
    hands: &'a dyn engine::Actuator,
    human: Option<&'a dyn HumanHelper>,
    sink: &'a Sink,
    config: Config,
) -> Engine<'a> {
    Engine {
        client,
        careful: None,
        perceptor,
        actuator: hands,
        approver: Box::leak(Box::new(approve_all())),
        interrupt: Box::leak(Box::new(flag())),
        sink,
        steering: &NoSteering,
        verifier: None,
        human,
        skills: &[],
        config,
    }
}

fn with_probe(p: &dyn Probe) -> RunOptions<'_> {
    RunOptions {
        probe: Some(p),
        ..Default::default()
    }
}

#[tokio::test]
async fn done_is_refused_while_a_probe_fails() {
    let screen = Screen {
        rename_at: 2,
        ..Screen::marks()
    };
    let hands = Hands::new(&screen);
    let first = first_with(SAVED);
    let model = Counting::new(&[&first, DONE, CLICK3, DONE]);
    let sink = Sink::default();
    let e = engine(&model, &screen, &hands, None, &sink, config());
    let probe = FakeProbe::answering(true);
    let trace = e.run_with("save the document", with_probe(&probe)).await;

    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(model.count(), 4);
    // The first done was refused, recorded, and told to the model.
    assert_eq!(trace.gate.len(), 1);
    assert_eq!(trace.gate[0].kind, "refused");
    assert_eq!(trace.gate[0].failed[0].reason, "no such element on screen");
    assert_eq!(trace.steps[1].path.as_deref(), Some("gate"));
    assert!(!trace.steps[1].verified);
    let seen = model.seen();
    assert!(
        seen[2].text.contains(
            "not done: saved: a11y_has(button, Saved!) failed (no such element on screen)"
        ),
        "{}",
        seen[2].text
    );
    assert!(seen[2].had_image, "a full frame after a refusal");
    // Verified by the probe once the screen showed it, at the step that saw it.
    let o = &trace.outcomes[0];
    assert_eq!(
        (o.status, o.by, o.verified_at),
        (Status::Verified, Some(VerifiedBy::Probe), Some(3))
    );
    assert_eq!(o.evidence, vec![0, 2], "the confirmed clicks before it");
    assert!(sink.all().contains("Gated"));
    assert!(
        probe.calls().is_empty(),
        "observation probes need no OS call"
    );
}

#[tokio::test]
async fn the_model_cannot_mark_an_outcome_verified() {
    let screen = Screen::marks(); // "Saved!" never appears
    let hands = Hands::new(&screen);
    let claims = SAVED.replace(
        r#""id""#,
        r#""status":"verified","verified_at":0,"by":"probe","id""#,
    );
    let first = first_with(&claims);
    let done_claiming = format!(
        r#"{{"thought":"verified: the document is saved, outcome verified","action":"done","risk":"none","outcomes":[{claims}]}}"#
    );
    let model = Counting::new(&[&first, &done_claiming, &done_claiming, &done_claiming]);
    let sink = Sink::default();
    let e = engine(&model, &screen, &hands, None, &sink, config());
    let trace = e.run("save the document").await;

    assert_ne!(trace.outcome, Some(Outcome::Success));
    let o = &trace.outcomes[0];
    assert_eq!(
        (o.status, o.verified_at, o.by),
        (Status::Pending, None, None)
    );
    assert_eq!(trace.gate.iter().filter(|g| g.kind == "refused").count(), 3);
}

#[tokio::test]
async fn headless_gate_ends_as_gated_never_verified() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let first = first_with(SAVED);
    let model = Counting::new(&[&first, DONE, DONE, DONE, DONE]);
    let sink = Sink::default();
    let e = engine(&model, &screen, &hands, None, &sink, config());
    let trace = e.run("save the document").await;

    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(model.count(), 4, "bounded: 3 refusals, then no more calls");
    assert_eq!(trace.human.last().unwrap().result, "no_one_to_ask");
    let f = failure::classify(&trace, Some(false)).unwrap();
    assert_eq!(f.category, Category::GatedNeverVerified);
}

#[tokio::test]
async fn gate_refusals_are_bounded_then_kai_asks() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let first = first_with(SAVED);
    let model = Counting::new(&[&first, DONE, DONE, DONE, DONE, DONE, DONE, DONE]);
    let person = Person::new(vec![HumanReply::Answer(
        "use Save As in the File menu".into(),
    )]);
    let sink = Sink::default();
    let e = engine(&model, &screen, &hands, Some(&person), &sink, config());
    let trace = e.run("save the document").await;

    // 3 refusals → ask (answered) → 3 more → ask again (the person stops).
    assert_eq!(person.asked().len(), 2);
    assert!(
        person.asked()[0].contains("could not confirm"),
        "{}",
        person.asked()[0]
    );
    assert_eq!(trace.gate.iter().filter(|g| g.kind == "refused").count(), 6);
    assert_eq!(model.count(), 7);
    assert!(
        model.seen()[4]
            .text
            .contains("The user answered your question: use Save As")
    );
    assert_eq!(trace.outcome, Some(Outcome::Stopped));
}

#[tokio::test]
async fn i_did_it_waives_the_gate() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let first = first_with(SAVED);
    let model = Counting::new(&[&first, DONE, DONE, DONE, DONE]);
    let person = Person::new(vec![HumanReply::Resolved]);
    let sink = Sink::default();
    let e = engine(&model, &screen, &hands, Some(&person), &sink, config());
    let trace = e.run("save the document").await;

    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(trace.gate.last().unwrap().kind, "waived_by_user");
    let o = &trace.outcomes[0];
    assert_eq!((o.status, o.by), (Status::Pending, Some(VerifiedBy::User)));
}

#[tokio::test]
async fn os_probes_go_through_the_port_and_a_write_never_reaches_it() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let first = first_with(
        r#"{"id":"wiped","description":"old files gone","probe":{"kind":"shell_ro","cmd":"rm -rf ~/old"}},
           {"id":"note","description":"note exists","probe":{"kind":"shell_ro","cmd":"test -f ~/kai-note.txt"}}"#,
    );
    let model = Counting::new(&[&first, DONE, DONE]);
    let sink = Sink::default();
    let e = engine(&model, &screen, &hands, None, &sink, config());
    let probe = FakeProbe::answering(false);
    let trace = e.run_with("tidy up", with_probe(&probe)).await;

    assert!(
        trace.outcomes[0]
            .probe_refused
            .as_deref()
            .unwrap()
            .contains("allowlist")
    );
    assert!(
        probe.calls().iter().all(|c| !c.contains("rm ")),
        "{:?}",
        probe.calls()
    );
    assert_eq!(probe.calls()[0], "shell_ro(test -f ~/kai-note.txt)");
    assert_eq!(
        trace.gate[0].failed.len(),
        1,
        "only the checkable outcome gates"
    );
    assert_eq!(trace.gate[0].failed[0].reason, "exit 1");
}

#[tokio::test]
async fn a_hanging_probe_times_out_and_fails() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let first = first_with(
        r#"{"id":"note","description":"note exists","probe":{"kind":"file_exists","path":"~/kai-note.txt"}}"#,
    );
    let model = Counting::new(&[&first, DONE, DONE, DONE]);
    let sink = Sink::default();
    let cfg = Config {
        probe_timeout: Duration::from_millis(50),
        ..config()
    };
    let e = engine(&model, &screen, &hands, None, &sink, cfg);
    let probe = FakeProbe::hanging();
    let t = Instant::now();
    let trace = tokio::time::timeout(
        Duration::from_secs(10),
        e.run_with("write a note", with_probe(&probe)),
    )
    .await
    .expect("a hanging probe must not hang the run");
    assert!(t.elapsed() < Duration::from_secs(5));
    assert_eq!(trace.gate[0].failed[0].reason, "timed out after 50 ms");
    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
}

#[tokio::test]
async fn no_outcomes_means_unchanged_behaviour() {
    let replies = [CLICK1, CLICK3, DONE];
    let run = |on: bool| async move {
        let screen = Screen::marks();
        let hands = Hands::new(&screen);
        let model = Counting::new(&replies);
        let sink = Sink::default();
        let cfg = Config {
            outcomes: on,
            ..Config::default()
        };
        let e = engine(&model, &screen, &hands, None, &sink, cfg);
        let probe = FakeProbe::answering(false);
        let trace = e.run_with("save", with_probe(&probe)).await;
        (trace, model.seen(), hands.log(), probe.calls())
    };
    let (off, off_calls, off_log, _) = run(false).await;
    let (on, on_calls, on_log, probed) = run(true).await;
    assert_eq!(off.outcome, on.outcome);
    assert_eq!(off.steps.len(), on.steps.len());
    assert_eq!(off_log, on_log);
    assert_eq!(off_calls.len(), on_calls.len());
    // Only the first turn differs: it asks for outcomes.
    assert!(!off_calls[0].text.contains(OUTCOME_REQUEST));
    assert!(on_calls[0].text.contains(OUTCOME_REQUEST));
    for (a, b) in off_calls.iter().zip(&on_calls).skip(1) {
        assert_eq!(a.text, b.text);
        assert_eq!(a.had_image, b.had_image);
    }
    assert!(on.outcomes.is_empty() && on.gate.is_empty() && probed.is_empty());
}

#[tokio::test]
async fn nothing_secret_reaches_the_trace() {
    const CODE: &str = "482913";
    let page = CodePage::new();
    let hands = CodeHands(&page);
    let person = Person::new(vec![HumanReply::Code(OneTimeCode::parse(CODE).unwrap())]);
    // The model somehow names the code in its outcome and probe.
    let first = format!(
        r#"{{"thought":"press verify","action":"click","target":3,"risk":"none","expect":"signed in",
            "outcomes":[{{"id":"code","description":"code {CODE} accepted",
            "probe":{{"kind":"a11y_has","name":"{CODE}"}}}}]}}"#
    );
    let model = Counting::new(&[&first, DONE, DONE, DONE]);
    let sink = Sink::default();
    let e = engine(&model, &page, &hands, Some(&person), &sink, config());
    let trace = e.run("sign in").await;

    assert_eq!(
        page.typed.lock().unwrap().as_deref(),
        Some(CODE),
        "the code was typed"
    );
    assert!(!trace.gate.is_empty(), "the gate ran");
    let json = serde_json::to_string(&trace).unwrap();
    assert!(!json.contains(CODE), "trace: {json}");
    assert!(!sink.all().contains(CODE), "events: {}", sink.all());
    for c in model.seen() {
        assert!(!c.text.contains(CODE), "model turn: {}", c.text);
    }
}

#[tokio::test]
async fn events_confirm_an_invisible_change() {
    // A frozen screen: clicks change nothing the list or pixels show.
    let screen = Screen::marks();
    let mut hands = Hands::new(&screen);
    hands.frozen = true;
    let events = AtomicU64::new(0);
    let firing = Firing {
        hands: &hands,
        events: &events,
    };
    let eyes = Eventful {
        screen: &screen,
        events: Some(&events),
    };
    let model = Counting::new(&[CLICK1, CLICK2, CLICK3, DONE]);
    let sink = Sink::default();
    let e = engine(&model, &eyes, &firing, None, &sink, config());
    let trace = e.run("toggle things").await;

    assert_eq!(
        trace.outcome,
        Some(Outcome::Success),
        "not stopped as no progress"
    );
    assert_eq!(events.load(Ordering::SeqCst), 3);
    assert!(
        trace.steps[..3]
            .iter()
            .all(|s| s.effect == Some(Effect::Confirmed))
    );
}

#[tokio::test]
async fn no_events_no_change_is_a_no_op() {
    let screen = Screen::marks();
    let mut hands = Hands::new(&screen);
    hands.frozen = true;
    let eyes = Eventful {
        screen: &screen,
        events: None,
    };
    let model = Counting::new(&[CLICK1, CLICK2, CLICK3, DONE]);
    let sink = Sink::default();
    let e = engine(&model, &eyes, &hands, None, &sink, config());
    let trace = e.run("toggle things").await;

    // The old rule: two steps with no change stop the run.
    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    assert!(
        trace.steps[..2]
            .iter()
            .all(|s| s.effect == Some(Effect::NoOp))
    );
}
