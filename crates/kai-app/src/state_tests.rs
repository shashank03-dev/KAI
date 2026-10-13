use super::*;
use kai_core::engine::ApprovalChoice;
use kai_core::human::HumanReply;
use std::sync::Arc;

fn decision(json: &str) -> Box<Decision> {
    Box::new(Decision::parse_checked(json).unwrap())
}

fn pending(text: &str) -> (Pending, futures::channel::oneshot::Receiver<ApprovalChoice>) {
    let (reply, rx) = futures::channel::oneshot::channel();
    let d = decision(&format!(
        r#"{{"thought":"x","action":"shell","text":"{text}","risk":"high"}}"#
    ));
    let p = Pending {
        decision: *d,
        element: None,
        explain: None,
        reply,
    };
    (p, rx)
}

#[test]
fn team_approvals_wait_their_turn_and_are_never_dropped() {
    // Two sub-agents ask at once, and a third acts meanwhile. Dropping
    // an approval answers Stop, which stopped a sub-agent live.
    let mut s = RunState::default();
    s.start("t".into(), true, 40);
    let (a, mut rx_a) = pending("rm a");
    let (b, mut rx_b) = pending("rm b");
    s.apply(Ui::Approval(a));
    s.apply(Ui::Approval(b));
    s.apply(Ui::Event(EngineEvent::Acted {
        step: 3,
        timings: Default::default(),
    }));
    assert_eq!(s.phase, Phase::Waiting);
    assert!(matches!(rx_a.try_recv(), Ok(None)), "a was dropped");
    assert!(matches!(rx_b.try_recv(), Ok(None)), "b was dropped");
    let first = s.take_approval().unwrap();
    assert_eq!(first.decision.text.as_deref(), Some("rm a"));
    assert_eq!(s.phase, Phase::Waiting, "b is showing now");
    first.reply.send(ApprovalChoice::Approve).unwrap();
    assert_eq!(rx_a.try_recv(), Ok(Some(ApprovalChoice::Approve)));
    let second = s.take_approval().unwrap();
    assert_eq!(second.decision.text.as_deref(), Some("rm b"));
    assert!(s.take_approval().is_none());
}

#[test]
fn pointer_is_a_share_of_the_screen() {
    let mut s = RunState::default();
    s.start("t".into(), false, 40);
    s.apply(Ui::Event(EngineEvent::Observed {
        step: 0,
        elements: String::new(),
        screenshot: Arc::from(&[][..]),
        screen: Some(Rect::new(0.0, 0.0, 1000.0, 500.0)),
    }));
    s.apply(Ui::Event(EngineEvent::Decided {
        step: 0,
        decision: decision(r#"{"thought":"x","action":"click","target":1,"risk":"none"}"#),
        target: Some(Rect::new(240.0, 240.0, 20.0, 20.0)),
    }));
    assert_eq!(s.pointer, Some((0.25, 0.5)));
    assert_eq!(s.steps.len(), 1);
    assert_eq!(s.steps[0].state, StepState::Working);
}

#[test]
fn frames_follow_the_newest_unless_scrubbed() {
    let mut s = RunState::default();
    s.apply(Ui::Frame {
        step: 0,
        path: "a".into(),
    });
    s.apply(Ui::Frame {
        step: 1,
        path: "b".into(),
    });
    assert_eq!(s.shown, Some(1));
    s.shown = Some(0);
    s.apply(Ui::Frame {
        step: 2,
        path: "c".into(),
    });
    assert_eq!(s.shown, Some(0), "a scrubbed viewer stays put");
}

#[test]
fn a_failed_run_marks_its_last_step() {
    let mut s = RunState::default();
    s.apply(Ui::Event(EngineEvent::Decided {
        step: 0,
        decision: decision(r#"{"thought":"x","action":"wait","risk":"none"}"#),
        target: None,
    }));
    let mut t = Trace::new("x");
    t.finish(Outcome::Stopped);
    s.apply(Ui::Finished(Box::new(t)));
    assert_eq!(s.phase, Phase::Done);
    assert_eq!(s.steps[0].state, StepState::Failed);
}

#[test]
fn steering_is_marked_read_when_the_agent_takes_it() {
    let mut s = RunState::default();
    s.queue("t".into(), false, 40, "m".into());
    assert_eq!(s.phase, Phase::Queued);
    s.begin();
    s.sent("use Firefox".into());
    assert_eq!(s.steers[0].read_at, None);
    s.apply(Ui::Event(EngineEvent::Steered {
        step: 2,
        message: "use Firefox".into(),
    }));
    assert_eq!(s.steers[0].read_at, Some(2));
    s.apply(Ui::Event(EngineEvent::Output {
        step: 3,
        command: "ls".into(),
        output: "a\nb".into(),
    }));
    assert_eq!(s.outputs.len(), 1);
}

#[test]
fn describes_in_plain_words() {
    let d = decision(r#"{"thought":"x","action":"click","target":3,"risk":"none"}"#);
    assert_eq!(describe(&d, Some("Save")), "Clicking “Save”");
    assert_eq!(describe(&d, None), "Clicking #3");
}

fn human(
    prompt: &str,
) -> (
    HumanPending,
    futures::channel::oneshot::Receiver<HumanReply>,
) {
    let (reply, rx) = futures::channel::oneshot::channel();
    let check = kai_core::human::HumanCheck {
        kind: kai_core::human::CheckKind::Captcha,
        prompt: prompt.into(),
        evidence: "test".into(),
        field: None,
    };
    (HumanPending { check, reply }, rx)
}

#[test]
fn one_sub_agents_check_ending_never_drops_anothers() {
    // A answers and its run reports HumanDone while B's check is showing:
    // B must stay (dropping it answers Stop).
    let mut s = RunState::default();
    s.start("t".into(), true, 40);
    let (a, _rx_a) = human("a");
    let (b, mut rx_b) = human("b");
    s.apply(Ui::Human(a));
    s.apply(Ui::Human(b));
    let a = s.take_human().unwrap();
    a.reply.send(HumanReply::Resolved).unwrap();
    s.apply(Ui::Event(EngineEvent::HumanDone {
        step: 2,
        kind: kai_core::human::CheckKind::Captcha,
        result: "resolved".into(),
    }));
    assert_eq!(s.human.as_ref().unwrap().check.prompt, "b");
    assert_eq!(s.phase, Phase::Waiting);
    assert!(matches!(rx_b.try_recv(), Ok(None)), "b was dropped");
}

#[test]
fn an_abandoned_check_or_approval_gives_way_to_the_next() {
    // A timed out (its run stopped listening): the next one shows.
    let mut s = RunState::default();
    s.start("t".into(), true, 40);
    let (a, rx_a) = human("a");
    let (b, _rx_b) = human("b");
    s.apply(Ui::Human(a));
    s.apply(Ui::Human(b));
    drop(rx_a);
    s.apply(Ui::Event(EngineEvent::HumanDone {
        step: 2,
        kind: kai_core::human::CheckKind::Captcha,
        result: "timed_out".into(),
    }));
    assert_eq!(s.human.as_ref().unwrap().check.prompt, "b");
    let (c, rx_c) = pending("rm c");
    let (d, _rx_d) = pending("rm d");
    s.apply(Ui::Approval(c));
    s.apply(Ui::Approval(d));
    drop(rx_c);
    s.apply(Ui::Event(EngineEvent::Acted {
        step: 3,
        timings: Default::default(),
    }));
    assert_eq!(
        s.approval.as_ref().unwrap().decision.text.as_deref(),
        Some("rm d")
    );
}

#[test]
fn stopping_answers_every_queued_approval_and_check() {
    let mut s = RunState::default();
    s.start("t".into(), true, 40);
    let (a, _ra) = pending("rm a");
    let (b, _rb) = pending("rm b");
    let (h, _rh) = human("h");
    s.apply(Ui::Approval(a));
    s.apply(Ui::Approval(b));
    s.apply(Ui::Human(h));
    let (approvals, humans) = s.take_all();
    assert_eq!((approvals.len(), humans.len()), (2, 1));
    assert!(s.approval.is_none() && s.approvals_waiting.is_empty() && s.human.is_none());
}

#[test]
fn an_approve_right_after_an_approval_appears_is_ignored() {
    // A double click on Approve must not approve the queued step that
    // takes the first one's place.
    let mut s = RunState::default();
    s.start("t".into(), true, 40);
    let (a, _ra) = pending("rm a");
    let (b, _rb) = pending("rm b");
    s.apply(Ui::Approval(a));
    s.apply(Ui::Approval(b));
    s.approval_since = Some(Instant::now() - APPROVE_GRACE);
    let first = s.answer_approval(ApprovalChoice::Approve).unwrap();
    assert_eq!(first.decision.text.as_deref(), Some("rm a"));
    // b just appeared: the second click of the double click is ignored.
    assert!(s.answer_approval(ApprovalChoice::Approve).is_none());
    assert_eq!(s.phase, Phase::Waiting);
    // Skip and Stop are never held back.
    let second = s.answer_approval(ApprovalChoice::Skip).unwrap();
    assert_eq!(second.decision.text.as_deref(), Some("rm b"));
    assert_eq!(s.phase, Phase::Running);
}
