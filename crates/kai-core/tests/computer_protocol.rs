//! OpenAI computer-tool wire format: what real responses carry and what Kai
//! sends back (`docs/RESEARCH_2026_10_OPENAI_PARITY.md` §3).
use kai_core::{computer::*, cua_protocol::*};
use serde_json::{Value, json};

fn call(extra: Value) -> Value {
    let mut item = json!({"type":"computer_call","call_id":"c1","status":"completed"});
    for (k, v) in extra.as_object().unwrap() {
        item[k] = v.clone();
    }
    json!({"id":"resp_1","status":"completed","output":[item]})
}

fn one(action: Value) -> Result<ComputerAction, ToolError> {
    parse_action(&action, 7, Some(3))
}

#[test]
fn a_real_ga_response_with_null_fields_parses() {
    // `gpt-5.4` as dumped by the SDK on Azure's page: both `action` and
    // `pending_safety_checks` are null next to `actions`.
    let dump = json!({
        "id": "resp_068b", "status": "completed", "error": null, "incomplete_details": null,
        "output": [
            {"id":"msg_1","type":"message","role":"assistant","status":"completed","phase":"commentary",
             "content":[{"type":"output_text","annotations":[],"logprobs":[],"text":"I'll open Bing."}]},
            {"id":"cu_1","type":"computer_call","call_id":"call_4y94","action":null,
             "pending_safety_checks":null,"status":"completed","actions":[{"type":"screenshot"}]}
        ]
    });
    let r = parse_response(&dump, 7, None).unwrap();
    assert_eq!(r.calls.len(), 1);
    assert_eq!(r.calls[0].actions, vec![ComputerAction::Observe]);
    assert!(r.calls[0].pending.is_empty());
    assert_eq!(
        r.message,
        Some(("I'll open Bing.".into(), Phase::Commentary))
    );
}

#[test]
fn absent_null_and_empty_mean_the_same() {
    let wait = json!([{"type":"wait"}]);
    for checks in [json!(null), json!([])] {
        let r = parse_response(
            &call(json!({"actions":wait,"pending_safety_checks":checks})),
            1,
            None,
        );
        assert!(r.unwrap().calls[0].pending.is_empty());
    }
    assert!(parse_response(&call(json!({"actions":wait})), 1, None).is_ok());
    assert!(parse_response(&call(json!({"action":null,"actions":wait})), 1, None).is_ok());
    assert!(
        parse_response(
            &call(json!({"action":{"type":"wait"},"actions":[]})),
            1,
            None
        )
        .is_ok()
    );
    // Neither, or both set: refused.
    assert!(parse_response(&call(json!({"action":null,"actions":null})), 1, None).is_err());
    assert!(
        parse_response(
            &call(json!({"action":{"type":"wait"},"actions":wait})),
            1,
            None
        )
        .is_err()
    );
    assert!(one(json!({"type":"click","x":1,"y":2,"button":"left","keys":null})).is_ok());
    assert!(one(json!({"type":"click","x":1,"y":2,"button":"left","keys":[]})).is_ok());
}

#[test]
fn pending_safety_checks_come_back_as_data() {
    let check =
        json!({"id":"cu_sc_1","code":"malicious_instructions","message":"Please acknowledge."});
    let r = parse_response(
        &call(json!({"actions":[{"type":"wait"}],"pending_safety_checks":[check]})),
        1,
        None,
    )
    .unwrap();
    assert_eq!(
        r.calls[0].pending,
        vec![SafetyCheck {
            id: "cu_sc_1".into(),
            code: Some("malicious_instructions".into()),
            message: Some("Please acknowledge.".into()),
        }]
    );
    // A check without an id cannot be acknowledged: refused.
    let r = parse_response(
        &call(json!({"actions":[{"type":"wait"}],"pending_safety_checks":[{"code":"x"}]})),
        1,
        None,
    );
    assert!(r.is_err());
}

#[test]
fn buttons_follow_openai_names() {
    let click = |b: &str| one(json!({"type":"click","x":1,"y":2,"button":b}));
    for (name, want) in [
        ("left", MouseButton::Left),
        ("right", MouseButton::Right),
        ("wheel", MouseButton::Middle),
        ("middle", MouseButton::Middle),
        ("back", MouseButton::Back),
        ("forward", MouseButton::Forward),
    ] {
        assert!(
            matches!(click(name), Ok(ComputerAction::Click { button, .. }) if button == want),
            "{name}"
        );
    }
    assert!(click("sideways").is_err());
}

#[test]
fn every_openai_key_name_normalises_and_unknown_names_are_refused() {
    for (names, want) in [
        (vec!["CTRL", "ESCAPE"], vec!["ctrl", "esc"]),
        (vec!["RETURN"], vec!["enter"]),
        (vec!["OPTION", "TAB"], vec!["alt", "tab"]),
        (vec!["COMMAND", "A"], vec!["cmd", "a"]),
        (
            vec!["META", "SHIFT", "ARROWLEFT"],
            vec!["cmd", "shift", "left"],
        ),
        (vec!["CONTROL", "INSERT"], vec!["ctrl", "insert"]),
        (vec!["PAGEDOWN"], vec!["pagedown"]),
    ] {
        let got = one(json!({"type":"keypress","keys":names})).unwrap();
        assert_eq!(
            got,
            ComputerAction::Keypress {
                keys: want.into_iter().map(String::from).collect()
            }
        );
    }
    // Unknown names never reach OS injection.
    assert!(one(json!({"type":"keypress","keys":["LAUNCH_MISSILES"]})).is_err());
    assert!(one(json!({"type":"keypress","keys":[]})).is_err());
}

#[test]
fn modifiers_are_held_on_mouse_actions() {
    let a = one(json!({"type":"click","x":405,"y":157,"button":"left","keys":["SHIFT"]})).unwrap();
    assert_eq!(a.modifiers(), ["shift"]);
    let a = one(json!({"type":"scroll","x":1,"y":2,"scroll_x":0,"scroll_y":300,"keys":["CTRL"]}))
        .unwrap();
    assert_eq!(a.modifiers(), ["ctrl"]);
    // Only modifier keys may be held, once each.
    assert!(one(json!({"type":"click","x":1,"y":2,"button":"left","keys":["A"]})).is_err());
    assert!(
        one(json!({"type":"click","x":1,"y":2,"button":"left","keys":["SHIFT","SHIFT"]})).is_err()
    );
}

#[test]
fn waits_default_to_two_seconds_and_stay_bounded() {
    assert_eq!(
        one(json!({"type":"wait"})).unwrap(),
        ComputerAction::Wait { duration_ms: 2000 }
    );
    assert_eq!(
        one(json!({"type":"wait","ms":500})).unwrap(),
        ComputerAction::Wait { duration_ms: 500 }
    );
    assert_eq!(
        one(json!({"type":"wait","ms":60000})).unwrap(),
        ComputerAction::Wait { duration_ms: 3000 }
    );
    assert!(one(json!({"type":"wait","ms":"soon"})).is_err());
}

#[test]
fn normal_batches_fit_and_runaway_ones_do_not() {
    let five = json!([
        {"type":"click","x":1,"y":2,"button":"left"},{"type":"type","text":"penguin"},
        {"type":"keypress","keys":["ENTER"]},{"type":"wait"},{"type":"screenshot"}
    ]);
    let r = parse_response(&call(json!({"actions":five})), 1, Some(3)).unwrap();
    assert_eq!(r.calls[0].actions.len(), 5);
    let many: Vec<Value> = (0..=MAX_BATCH).map(|_| json!({"type":"wait"})).collect();
    assert!(parse_response(&call(json!({"actions":many})), 1, None).is_err());
}

#[test]
fn scroll_deltas_are_pixels_and_convert_to_notches_by_openais_recipe() {
    assert!(matches!(
        one(json!({"type":"scroll","x":4,"y":8,"scroll_x":-21,"scroll_y":300})).unwrap(),
        ComputerAction::Scroll {
            dx: -21,
            dy: 300,
            unit: ScrollUnit::Pixels,
            ..
        }
    ));
    for (px, notches) in [
        (0, 0),
        (1, 1),
        (149, 1),
        (151, 2),
        (250, 3),
        (-260, -3),
        (5000, 20),
    ] {
        assert_eq!(pixels_to_notches(px), notches, "{px}");
    }
}

#[test]
fn malformed_and_unknown_requests_fail_closed() {
    for action in [
        json!({"type":"move","x":1}),
        json!({"type":"type","text":""}),
        json!({"type":"teleport"}),
        json!({"type":"move","x":1,"y":2,"extra":true}),
        json!({"type":"move","x":-1,"y":2}),
    ] {
        assert!(one(action.clone()).is_err(), "{action}");
    }
    // Typing needs a field the tree says has focus.
    assert_eq!(
        parse_action(&json!({"type":"type","text":"x"}), 1, None),
        Err(ToolError::NeedsUser)
    );
    assert!(parse_response(&json!({"status":"incomplete","output":[]}), 1, None).is_err());
    assert!(parse_response(&json!({"status":"completed"}), 1, None).is_err());
}

#[test]
fn final_answers_are_not_verified_success() {
    let r = parse_response(
        &json!({"status":"completed","output":[{"type":"message","phase":"final_answer","content":[{"type":"output_text","text":"done"}]}]}),
        1,
        None,
    )
    .unwrap();
    assert!(r.calls.is_empty());
    assert_eq!(r.message, Some(("done".into(), Phase::FinalAnswer)));
}

#[test]
fn screenshots_go_back_at_original_detail_with_accepted_checks() {
    let out = screenshot_output("c1", &[1, 2], "image/jpeg", &[]);
    assert_eq!(out["output"]["type"], "computer_screenshot");
    assert_eq!(out["output"]["detail"], "original");
    assert!(
        out["output"]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/jpeg;base64,")
    );
    assert!(out.get("acknowledged_safety_checks").is_none());
    let check = SafetyCheck {
        id: "cu_sc_1".into(),
        code: Some("irrelevant_domain".into()),
        message: None,
    };
    let out = screenshot_output("c1", &[1], "image/png", std::slice::from_ref(&check));
    assert_eq!(
        out["acknowledged_safety_checks"],
        json!([{"id":"cu_sc_1","code":"irrelevant_domain"}])
    );
    assert_eq!(
        harness_note("action 2 did not run")["content"][0]["text"],
        "Harness note: action 2 did not run"
    );
}
