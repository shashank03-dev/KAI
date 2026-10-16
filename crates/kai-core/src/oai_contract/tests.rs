use super::*;
use crate::computer::{ComputerAction, MouseButton, Target, ToolError};
use crate::cua_protocol;
use serde_json::json;

const SIZE: (u32, u32) = (1440, 900);

fn act(raw: serde_json::Value, frame: CoordFrame) -> Result<Turn, ToolError> {
    decode(&raw.to_string(), frame, SIZE, 9, Some(4))
}

#[test]
fn an_open_models_click_equals_openais_click() {
    // The same click, from OpenAI and from an open model in pixel frame.
    let openai = cua_protocol::parse_response(
        &json!({"status":"completed","output":[{"type":"computer_call","call_id":"c",
            "actions":[{"type":"click","button":"left","x":405,"y":157,"keys":["SHIFT"]}]}]}),
        9,
        Some(4),
    )
    .unwrap()
    .calls
    .remove(0)
    .actions
    .remove(0);
    let open = act(
        json!({"thought":"open Filters","actions":[{"type":"click","button":"left","x":405,"y":157,"keys":["SHIFT"]}],"message":null,"phase":"commentary"}),
        CoordFrame::Pixels,
    )
    .unwrap();
    assert_eq!(
        open,
        Turn::Act {
            thought: "open Filters".into(),
            action: openai
        }
    );
}

#[test]
fn normalised_coordinates_become_screenshot_pixels() {
    let Turn::Act { action, .. } = act(
        json!({"thought":"t","actions":[{"type":"click","button":"left","x":500,"y":500,"keys":null}],"message":null,"phase":"commentary"}),
        CoordFrame::Norm1000,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(
        action,
        ComputerAction::Click {
            target: Target::Point {
                observation: 9,
                x: 720.,
                y: 450.
            },
            button: MouseButton::Left,
            count: 1,
            modifiers: vec![],
        }
    );
    // Off the frame: refused, never clamped onto some other element.
    assert!(act(
        json!({"thought":"t","actions":[{"type":"click","button":"left","x":1200,"y":5,"keys":null}],"message":null,"phase":"commentary"}),
        CoordFrame::Norm1000,
    )
    .is_err());
}

#[test]
fn a_mark_stays_an_element_target() {
    let Turn::Act { action, .. } = act(
        json!({"thought":"t","actions":[{"type":"click","button":"left","x":null,"y":null,"mark":14,"keys":null}],"message":null,"phase":"commentary"}),
        CoordFrame::Marks,
    )
    .unwrap() else {
        panic!()
    };
    assert!(matches!(
        action,
        ComputerAction::Click {
            target: Target::Element {
                observation: 9,
                mark: 14
            },
            ..
        }
    ));
    // Marks are for marks-frame models only.
    assert!(act(
        json!({"thought":"t","actions":[{"type":"click","button":"left","mark":14}],"message":null,"phase":"commentary"}),
        CoordFrame::Pixels,
    )
    .is_err());
}

#[test]
fn typing_keys_and_waits_use_openais_vocabulary() {
    let typed = act(
        json!({"thought":"t","actions":[{"type":"type","text":"penguin"}],"message":null,"phase":"commentary"}),
        CoordFrame::Marks,
    )
    .unwrap();
    assert!(matches!(
        typed,
        Turn::Act {
            action: ComputerAction::Type {
                target: Target::Element { mark: 4, .. },
                ..
            },
            ..
        }
    ));
    let keys = act(
        json!({"thought":"t","actions":[{"type":"keypress","keys":["CTRL","RETURN"]}],"message":null,"phase":"commentary"}),
        CoordFrame::Marks,
    )
    .unwrap();
    assert!(
        matches!(keys, Turn::Act { action: ComputerAction::Keypress { ref keys }, .. } if keys == &["ctrl", "enter"])
    );
}

#[test]
fn messages_end_the_turn_as_done_or_a_question() {
    assert_eq!(
        act(
            json!({"thought":"t","actions":[],"message":"Saved.","phase":"final_answer"}),
            CoordFrame::Marks
        )
        .unwrap(),
        Turn::Done {
            message: "Saved.".into()
        }
    );
    assert_eq!(
        act(
            json!({"thought":"t","actions":[],"message":"Which file?","phase":"commentary"}),
            CoordFrame::Marks
        )
        .unwrap(),
        Turn::Ask {
            message: "Which file?".into()
        }
    );
    // Nothing to do and nothing to say: a format error, not a silent stop.
    assert!(
        act(
            json!({"thought":"t","actions":[],"message":null,"phase":"commentary"}),
            CoordFrame::Marks
        )
        .is_err()
    );
    // One action per turn.
    assert!(act(
        json!({"thought":"t","actions":[{"type":"wait"},{"type":"wait"}],"message":null,"phase":"commentary"}),
        CoordFrame::Marks
    )
    .is_err());
}

#[test]
fn the_schema_offers_marks_only_in_marks_frame() {
    let marks = schema(CoordFrame::Marks).to_string();
    let pixels = schema(CoordFrame::Pixels).to_string();
    assert!(marks.contains("\"mark\""));
    assert!(!pixels.contains("\"mark\""));
    assert!(pixels.contains("\"wheel\""));
}
