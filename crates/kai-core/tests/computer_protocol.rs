use kai_core::{computer::*, cua_protocol::*};
use serde_json::json;
#[test]
fn current_and_legacy_calls_share_strict_actions() {
    let current = json!({"status":"completed","output":[{"type":"computer_call","call_id":"c1","actions":[{"type":"move","x":4,"y":8},{"type":"scroll","x":4,"y":8,"scroll_x":0,"scroll_y":0}],"pending_safety_checks":[]}]});
    let response = parse_response(&current, 7, None).unwrap();
    assert_eq!(response.calls[0].actions.len(), 2);
    assert!(matches!(
        response.calls[0].actions[1],
        ComputerAction::Scroll {
            dx: 0,
            dy: 0,
            unit: ScrollUnit::Pixels,
            ..
        }
    ));
    let old = json!({"output":[{"type":"computer_call","call_id":"old","action":{"type":"click","x":1,"y":2,"button":"middle"}}]});
    assert!(matches!(
        parse_response(&old, 7, None).unwrap().calls[0].actions[0],
        ComputerAction::Click {
            button: MouseButton::Middle,
            ..
        }
    ));
    assert_eq!(
        screenshot_output("c1", &[1, 2])["output"]["type"],
        "computer_screenshot"
    );
}
#[test]
fn malformed_incomplete_ambiguous_and_unknown_requests_fail_closed() {
    for action in [
        json!({"type":"click","x":1,"y":2,"button":"sideways"}),
        json!({"type":"move","x":1}),
        json!({"type":"type","text":"x"}),
        json!({"type":"keypress","keys":[]}),
        json!({"type":"teleport"}),
        json!({"type":"move","x":1,"y":2,"extra":true}),
    ] {
        assert!(parse_action(&action, 1, None).is_err(), "{action}");
    }
    assert!(parse_response(&json!({"status":"incomplete","output":[]}), 1, None).is_err());
    assert!(parse_response(&json!({"status":"completed"}), 1, None).is_err());
    assert!(parse_response(&json!({"output":[{"type":"computer_call","call_id":"x","action":{"type":"wait"},"actions":[]}]}),1,None).is_err());
}
#[test]
fn safety_checks_and_text_completion_are_not_verified_success() {
    let r = parse_response(
        &json!({"status":"completed","output":[{"type":"computer_call","call_id":"x","action":{"type":"wait"},"pending_safety_checks":[{"id":"check"}]}]}),
        1,
        None,
    );
    assert_eq!(r.unwrap_err(), ToolError::NeedsUser);
    let r=parse_response(&json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"done"}]}]}),1,None).unwrap();
    assert!(r.calls.is_empty());
    assert_eq!(r.message.as_deref(), Some("done"));
}
