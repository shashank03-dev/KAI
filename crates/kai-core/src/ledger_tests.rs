use super::*;
use crate::client::Images;
use crate::element::{Element, Grounding, Rect, Role};
use serde_json::json;

fn obs(title: &str) -> Observation {
    Observation {
        images: Images {
            screenshot_jpeg: Vec::new(),
            crop_jpeg: None,
        },
        grounding: Grounding::Marks(vec![Element {
            mark: 1,
            role: Role::Button,
            name: "Inbox".into(),
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
            enabled: true,
            secure: false,
            value: None,
        }]),
        screen_hash: 0,
        timing: Default::default(),
        screen: None,
        window_title: Some(title.into()),
        url: None,
    }
}

#[test]
fn a_refused_probe_keeps_the_outcome() {
    let l = Ledger::from_raw(&[
        json!({"id":"gone","description":"the folder is gone",
               "probe":{"kind":"shell_ro","cmd":"rm -rf ~/x"}}),
        json!({"id":"odd","description":"weird probe","probe":"ls"}),
        json!({"id":"menu","description":"off menu","probe":{"kind":"shell","cmd":"ls"}}),
    ]);
    let r = l.records(&[]);
    assert_eq!(r.len(), 3, "no outcome is dropped for a bad probe");
    assert!(
        r.iter()
            .all(|o| o.probe.is_none() && o.probe_refused.is_some())
    );
    assert!(r[0].probe_refused.as_deref().unwrap().contains("allowlist"));
    assert!(!l.gated());
}

#[test]
fn the_model_cannot_set_a_status() {
    let l = Ledger::from_raw(&[json!({"id":"sent","description":"form sent",
        "status":"verified","verified_at":0,"by":"probe",
        "probe":{"kind":"window_title","pattern":"thank you"}})]);
    let r = &l.records(&[])[0];
    assert_eq!(r.status, Status::Pending);
    assert_eq!((r.verified_at, r.by), (None, None));
    assert!(l.gated());
}

#[test]
fn ids_are_cleaned_and_outcomes_capped() {
    let many: Vec<_> = (0..7)
        .map(|i| json!({"id":"Same Id!","description":format!("thing {i}")}))
        .collect();
    let l = Ledger::from_raw(&many);
    let ids: Vec<String> = l.records(&[]).into_iter().map(|r| r.id).collect();
    assert_eq!(ids, vec!["same_id", "o2", "o3", "o4"]);
    // No description: the probe names it; neither: dropped. Not objects: dropped.
    let l = Ledger::from_raw(&[
        json!({"probe":{"kind":"file_exists","path":"~/a"}}),
        json!({"id":"x"}),
        json!("done"),
    ]);
    let r = l.records(&[]);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].description, "file_exists(~/a)");
}

#[test]
fn probes_verify_and_a_failure_at_done_reverts() {
    let mut l = Ledger::from_raw(&[json!({"id":"thanks","description":"thank-you page",
        "probe":{"kind":"window_title","pattern":"thank you"}})]);
    l.note_effect(2, Effect::Confirmed);
    l.note_effect(3, Effect::NoOp);
    assert!(!l.observe(&obs("Contact"), 3));
    assert!(l.observe(&obs("Thank you"), 4));
    let r = &l.records(&[])[0];
    assert_eq!(
        (r.status, r.verified_at, r.evidence.clone()),
        (Status::Verified, Some(4), vec![2])
    );
    // At a done claim the probe runs again; failing now reverts it.
    let (i, _, _) = l.probed()[0].clone();
    assert!(l.settle(i, &ProbeVerdict::fail("title is \"Contact\""), 6));
    assert_eq!(l.records(&[])[0].status, Status::Reverted);
    // Only an observation pass re-verifies; an observation failure is not a revert.
    assert!(l.observe(&obs("Thank you"), 7));
    assert!(!l.observe(&obs("Other"), 8));
    assert_eq!(l.records(&[])[0].status, Status::Verified);
}

#[test]
fn codes_are_blanked_from_every_string() {
    let code = OneTimeCode::parse("482913").unwrap();
    assert_eq!(
        redact_text("type 482913 here", std::slice::from_ref(&code)),
        "type [one-time code] here"
    );
    assert_eq!(
        redact_text("48 29-13", std::slice::from_ref(&code)),
        REDACTED_CODE
    );
    assert_eq!(redact_text("nothing", &[]), "nothing");
    let l = Ledger::from_raw(&[json!({"id":"c","description":"code 482913 accepted",
        "probe":{"kind":"a11y_has","name":"482913"}})]);
    let r = &l.records(std::slice::from_ref(&code))[0];
    assert!(!r.description.contains("482913") && !r.probe.as_ref().unwrap().contains("482913"));
}
