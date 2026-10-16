//! An open model's reply in OpenAI's computer-tool vocabulary, and its
//! translation into the exact `computer_call` the OpenAI path parses. One
//! parser for GPT and open models means an A/B compares models, not
//! harnesses (`docs/RESEARCH_2026_10_OPENAI_PARITY.md` §5a).
use super::CoordFrame;
use crate::computer::{ComputerAction, Target, ToolError};
use crate::cua_protocol;
use serde_json::{Value, json};

/// What the model may write: a short thought, at most one action, or a
/// message that ends the turn.
pub fn schema(frame: CoordFrame) -> Value {
    let int = json!({"type": ["integer", "null"]});
    let mut point = vec![("x", int.clone()), ("y", int.clone())];
    if frame == CoordFrame::Marks {
        point.push(("mark", int.clone()));
    }
    let keys = json!({"type": ["array", "null"], "items": {"type": "string"}});
    let object = |ty: &str, extra: &[(&str, Value)]| {
        let mut props = serde_json::Map::new();
        props.insert("type".into(), json!({"type": "string", "enum": [ty]}));
        for (k, v) in extra {
            props.insert((*k).into(), v.clone());
        }
        let required: Vec<String> = props.keys().cloned().collect();
        json!({"type": "object", "additionalProperties": false, "required": required, "properties": props})
    };
    let with = |extra: &[(&'static str, Value)]| -> Vec<(&'static str, Value)> {
        point.iter().cloned().chain(extra.iter().cloned()).collect()
    };
    let button = json!({"type": "string", "enum": ["left", "right", "wheel", "back", "forward"]});
    let variants = vec![
        object(
            "click",
            &with(&[("button", button), ("keys", keys.clone())]),
        ),
        object("double_click", &with(&[("keys", keys.clone())])),
        object("move", &with(&[])),
        object(
            "scroll",
            &with(&[
                ("scroll_x", json!({"type": "integer"})),
                ("scroll_y", json!({"type": "integer"})),
            ]),
        ),
        object("type", &[("text", json!({"type": "string"}))]),
        object(
            "keypress",
            &[(
                "keys",
                json!({"type": "array", "items": {"type": "string"}, "minItems": 1}),
            )],
        ),
        object("wait", &[]),
    ];
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["thought", "actions", "message", "phase"],
        "properties": {
            "thought": {"type": "string"},
            "actions": {"type": "array", "maxItems": 1, "items": {"anyOf": variants}},
            "message": {"type": ["string", "null"]},
            "phase": {"type": "string", "enum": ["commentary", "final_answer"]}
        }
    })
}

/// A decoded turn: one action, or a message (done, or a question).
#[derive(Debug, PartialEq)]
pub enum Turn {
    Act {
        thought: String,
        action: ComputerAction,
    },
    Done {
        message: String,
    },
    Ask {
        message: String,
    },
}

const POINTED: [&str; 4] = ["click", "double_click", "move", "scroll"];

/// Turn the model's coordinates (or mark) into the screenshot's pixels, so
/// only OpenAI's own fields reach the parser. A mark is replaced afterwards
/// by an element target: the accessibility path still applies to it.
fn to_openai(
    a: &Value,
    frame: CoordFrame,
    size: (u32, u32),
) -> Result<(Value, Option<u32>), ToolError> {
    let mut a = a.as_object().ok_or(ToolError::InvalidArguments)?.clone();
    let ty = a
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mark = a.remove("mark").filter(|m| !m.is_null());
    if !POINTED.contains(&ty.as_str()) {
        a.retain(|k, v| !(matches!(k.as_str(), "x" | "y") && v.is_null()));
        return Ok((Value::Object(a), None));
    }
    if let Some(m) = mark {
        let m = m.as_u64().filter(|m| *m > 0 && frame == CoordFrame::Marks);
        let m = m
            .and_then(|m| u32::try_from(m).ok())
            .ok_or(ToolError::InvalidArguments)?;
        a.insert("x".into(), json!(0));
        a.insert("y".into(), json!(0));
        return Ok((Value::Object(a), Some(m)));
    }
    let coord = |k: &str| {
        a.get(k)
            .and_then(Value::as_f64)
            .ok_or(ToolError::InvalidArguments)
    };
    let (x, y) = frame
        .to_image(coord("x")?, coord("y")?, size)
        .ok_or(ToolError::InvalidArguments)?;
    a.insert("x".into(), json!(x.floor() as i64));
    a.insert("y".into(), json!(y.floor() as i64));
    Ok((Value::Object(a), None))
}

/// Decode `raw` against the image the model saw. The action goes through
/// `cua_protocol::parse_response`, the OpenAI path's own parser.
pub fn decode(
    raw: &str,
    frame: CoordFrame,
    size: (u32, u32),
    observation: u64,
    focused: Option<u32>,
) -> Result<Turn, ToolError> {
    let start = raw.find('{').ok_or(ToolError::InvalidArguments)?;
    let end = raw.rfind('}').ok_or(ToolError::InvalidArguments)?;
    let v: Value =
        serde_json::from_str(&raw[start..=end]).map_err(|_| ToolError::InvalidArguments)?;
    let thought = v["thought"].as_str().unwrap_or("").trim().to_string();
    let actions = cua_protocol::present(v.get("actions"))
        .map(|a| a.as_array().cloned().ok_or(ToolError::InvalidArguments))
        .transpose()?
        .unwrap_or_default();
    let message = v["message"]
        .as_str()
        .map(str::trim)
        .filter(|m| !m.is_empty());
    match actions.as_slice() {
        [] => {
            let message = message.ok_or(ToolError::InvalidArguments)?.to_string();
            Ok(match v["phase"].as_str() {
                Some("final_answer") => Turn::Done { message },
                _ => Turn::Ask { message },
            })
        }
        [one] => {
            let (openai, mark) = to_openai(one, frame, size)?;
            let response = json!({"status": "completed", "output": [
                {"type": "reasoning", "summary": [{"type": "summary_text", "text": thought}]},
                {"type": "computer_call", "call_id": "local", "status": "completed",
                 "pending_safety_checks": [], "actions": [openai]}
            ]});
            let mut parsed = cua_protocol::parse_response(&response, observation, focused)?;
            let mut action = parsed.calls.remove(0).actions.remove(0);
            if let Some(mark) = mark
                && let ComputerAction::Click { target, .. }
                | ComputerAction::Move { target, .. }
                | ComputerAction::Scroll { target, .. } = &mut action
            {
                *target = Target::Element { observation, mark };
            }
            Ok(Turn::Act { thought, action })
        }
        _ => Err(ToolError::InvalidArguments),
    }
}
