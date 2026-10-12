use super::*;
use crate::element::Role;
use crate::ledger::{OutcomeRecord, Status as OStatus, VerifiedBy};
use crate::trace::{StepRecord, Timings};

fn d(raw: &str) -> Decision {
    Decision::parse_checked(raw).unwrap()
}

fn sig(role: Role, name: &str) -> Option<TargetSig> {
    Some(TargetSig {
        role,
        name: name.into(),
    })
}

/// Open a mail site, click Compose, type the address, click Send-ish.
fn trace(task: &str, to: &str) -> Trace {
    let mut t = Trace::new(task);
    let steps = [
        (
            d(
                r#"{"thought":"","action":"open_url","text":"https://mail.example.com","risk":"none"}"#,
            ),
            None,
        ),
        (
            d(r#"{"thought":"","action":"click","target":4,"risk":"low"}"#),
            sig(Role::Button, "Compose"),
        ),
        (
            d(&format!(
                r#"{{"thought":"","action":"type","target":7,"text":"{to}","risk":"low"}}"#
            )),
            sig(Role::TextField, "To"),
        ),
        (d(r#"{"thought":"","action":"done","risk":"none"}"#), None),
    ];
    for (i, (dec, target)) in steps.into_iter().enumerate() {
        let mut s = StepRecord::new(i as u32, &dec, Timings::default(), "Mock");
        s.target = target;
        t.push(s);
    }
    t.finish(Outcome::Success);
    t
}

fn verified_outcome() -> OutcomeRecord {
    OutcomeRecord {
        id: "drafted".into(),
        description: "draft open".into(),
        probe: None,
        probe_refused: None,
        status: OStatus::Verified,
        verified_at: Some(2),
        by: Some(VerifiedBy::Probe),
        evidence: vec![],
    }
}

#[test]
fn shape_turns_specifics_into_slots() {
    let (s, slots) = shape("Email alice@x.com about \"lunch on Friday\" please");
    assert_eq!(s, "email <slot> about <slot>");
    assert_eq!(slots, vec!["alice@x.com", "lunch on Friday"]);
    // An unpaired quote is just a character.
    let (s, slots) = shape("write \"hello");
    assert_eq!((s.as_str(), slots.len()), ("write hello", 0));
}

#[test]
fn steps_keep_signatures_not_marks_and_stop_before_done() {
    let steps = steps_of(&trace("t", "a@b.c"));
    assert_eq!(steps.len(), 3);
    assert!(steps.iter().all(|s| s.decision.target.is_none()));
    assert_eq!(steps[1].target, sig(Role::Button, "Compose"));
}

#[test]
fn a_targeted_step_without_a_signature_ends_the_procedure() {
    let mut t = trace("t", "a@b.c");
    t.steps[1].target = None; // e.g. a grid cell or a secure field
    assert_eq!(steps_of(&t).len(), 1);
}

#[test]
fn approvals_human_checks_and_high_risk_end_the_procedure() {
    for k in 0..3 {
        let mut t = trace("t", "a@b.c");
        match k {
            0 => t.steps[1].approval = Some("approved".into()),
            1 => t.steps[1].path = Some("human".into()),
            _ => t.steps[1].decision.risk = Risk::High,
        }
        assert_eq!(steps_of(&t).len(), 1, "case {k}");
    }
}

#[test]
fn one_unverified_success_is_only_a_candidate() {
    let s = ProcedureStore::in_memory();
    assert!(
        s.record(&trace("email alice@x.com", "alice@x.com"), false)
            .unwrap()
    );
    assert_eq!(s.list()[0].status, Status::Candidate);
    assert!(
        s.lookup("email alice@x.com").is_none(),
        "candidates never replay"
    );
    // A second model-led success makes it active.
    s.record(&trace("email alice@x.com", "alice@x.com"), false)
        .unwrap();
    assert_eq!(s.list()[0].status, Status::Active);
    assert!(s.lookup("email alice@x.com").is_some());
}

#[test]
fn a_verified_run_is_active_at_once() {
    let mut t = trace("email alice@x.com", "alice@x.com");
    t.outcomes = vec![verified_outcome()];
    assert!(all_verified(&t));
    let s = ProcedureStore::in_memory();
    s.record(&t, all_verified(&t)).unwrap();
    assert_eq!(s.list()[0].status, Status::Active);
    // A waived gate is not verification.
    t.outcomes[0].by = Some(VerifiedBy::User);
    assert!(!all_verified(&t));
}

#[test]
fn new_specifics_are_rebound_in_text_and_names() {
    let s = ProcedureStore::in_memory();
    let mut t = trace("email alice@x.com", "alice@x.com");
    t.outcomes = vec![verified_outcome()];
    s.record(&t, true).unwrap();
    let b = s.lookup("Email bob@y.org").unwrap();
    assert_eq!(b.steps[2].decision.text.as_deref(), Some("bob@y.org"));
    // The fixed parts are untouched.
    assert_eq!(
        b.steps[0].decision.text.as_deref(),
        Some("https://mail.example.com")
    );
    // Different words never match.
    assert!(s.lookup("forward alice@x.com").is_none());
}

#[test]
fn a_specific_no_step_uses_cannot_be_rebound() {
    let s = ProcedureStore::in_memory();
    // The task names a file the steps never typed: changing it means a
    // different job, not a different value.
    let mut t = trace("email report.pdf to alice@x.com", "alice@x.com");
    t.outcomes = vec![verified_outcome()];
    s.record(&t, true).unwrap();
    assert!(s.lookup("email notes.pdf to alice@x.com").is_none());
    assert!(s.lookup("email report.pdf to bob@x.com").is_some());
}

#[test]
fn shell_slots_refuse_anything_a_shell_reads_specially() {
    let ls = ProcStep {
        decision: d(
            r#"{"thought":"","action":"shell","text":"ls ~/Downloads/2026","risk":"none"}"#,
        ),
        target: None,
    };
    let old = vec!["2026".to_string()];
    let ok = bind(std::slice::from_ref(&ls), &old, &["2025".into()]).unwrap();
    assert_eq!(ok[0].decision.text.as_deref(), Some("ls ~/Downloads/2025"));
    for bad in ["x;rm", "$(id)", "a b", "`id`", "-rf", "a|b", "a>b"] {
        assert!(
            bind(std::slice::from_ref(&ls), &old, &[bad.into()]).is_none(),
            "{bad}"
        );
    }
}

#[test]
fn rebinding_never_replaces_a_new_value_again() {
    let st = ProcStep {
        decision: d(r#"{"thought":"","action":"type","target":2,"text":"1 and 10","risk":"none"}"#),
        target: sig(Role::TextField, "Note"),
    };
    let old = vec!["1".to_string(), "10".to_string()];
    let out = bind(&[st], &old, &["10".into(), "100".into()]).unwrap();
    assert_eq!(out[0].decision.text.as_deref(), Some("10 and 100"));
}

#[test]
fn failed_replays_quarantine_then_retire() {
    let s = ProcedureStore::in_memory();
    let mut t = trace("email alice@x.com", "alice@x.com");
    t.outcomes = vec![verified_outcome()];
    s.record(&t, true).unwrap();
    let shape = s.list()[0].shape.clone();
    s.report(&shape, false).unwrap();
    assert_eq!(s.list()[0].status, Status::Quarantined);
    assert!(s.lookup("email alice@x.com").is_none());
    // A fresh verified run brings it back.
    s.record(&t, true).unwrap();
    assert_eq!(s.list()[0].status, Status::Active);
    s.report(&shape, false).unwrap();
    assert_eq!(s.list()[0].status, Status::Retired);
    // Retired stays retired.
    assert!(!s.record(&t, true).unwrap());
    assert!(s.lookup("email alice@x.com").is_none());
}

#[test]
fn stored_on_disk_and_forgotten() {
    let dir = std::env::temp_dir().join(format!("kai-procs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let s = ProcedureStore::load(&dir).unwrap();
    let mut t = trace("email alice@x.com", "alice@x.com");
    t.outcomes = vec![verified_outcome()];
    s.record(&t, true).unwrap();
    let again = ProcedureStore::load(&dir).unwrap();
    assert_eq!(again.list().len(), 1);
    assert!(again.lookup("email carol@z.io").is_some());
    let shape = again.list()[0].shape.clone();
    assert!(again.forget(&shape).unwrap());
    assert!(ProcedureStore::load(&dir).unwrap().list().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_runs_are_never_recorded() {
    let mut t = trace("email alice@x.com", "alice@x.com");
    t.finish(Outcome::Failed);
    let s = ProcedureStore::in_memory();
    assert!(!s.record(&t, true).unwrap());
}
