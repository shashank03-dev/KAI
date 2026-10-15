//! Pure adapter for current Responses computer actions and the legacy action.
//! No network or execution: callers must authorize and dispatch the results.
use crate::computer::*;
use base64::Engine as _;
use serde_json::{Value, json};
#[derive(Debug)]
pub struct ComputerCall {
    pub call_id: String,
    pub actions: Vec<ComputerAction>,
}
#[derive(Debug)]
pub struct ComputerResponse {
    pub calls: Vec<ComputerCall>,
    pub message: Option<String>,
}

pub fn parse_response(
    v: &Value,
    observation: u64,
    focused: Option<u32>,
) -> Result<ComputerResponse, ToolError> {
    if v.get("status").is_some_and(|s| s != "completed")
        || v.get("error").is_some_and(|e| !e.is_null())
    {
        return Err(ToolError::Failed);
    }
    let output = v["output"].as_array().ok_or(ToolError::InvalidArguments)?;
    let mut calls = Vec::new();
    let mut message = None;
    for item in output {
        match item["type"].as_str() {
            Some("computer_call") => {
                if item.get("status").is_some_and(|s| s != "completed") {
                    return Err(ToolError::Failed);
                }
                if item
                    .get("pending_safety_checks")
                    .is_some_and(|c| c.as_array().is_none_or(|a| !a.is_empty()))
                {
                    return Err(ToolError::NeedsUser);
                }
                let call_id = item["call_id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or(ToolError::InvalidArguments)?
                    .to_string();
                if calls.iter().any(|c: &ComputerCall| c.call_id == call_id) {
                    return Err(ToolError::InvalidArguments);
                }
                let raw = match (item.get("action"), item.get("actions")) {
                    (Some(a), None) => vec![a],
                    (None, Some(a)) => a
                        .as_array()
                        .ok_or(ToolError::InvalidArguments)?
                        .iter()
                        .collect(),
                    _ => return Err(ToolError::InvalidArguments),
                };
                if raw.is_empty() || raw.len() > 4 {
                    return Err(ToolError::InvalidArguments);
                }
                let actions = raw
                    .into_iter()
                    .map(|a| parse_action(a, observation, focused))
                    .collect::<Result<_, _>>()?;
                calls.push(ComputerCall { call_id, actions });
                if calls.len() > 4 {
                    return Err(ToolError::InvalidArguments);
                }
            }
            Some("message") => {
                if let Some(contents) = item["content"].as_array() {
                    for content in contents {
                        if content["type"] == "output_text" {
                            message = content["text"].as_str().map(str::to_string);
                        }
                    }
                }
            }
            Some("reasoning") => {}
            _ => return Err(ToolError::Unsupported),
        }
    }
    if calls.iter().map(|c| c.actions.len()).sum::<usize>() > 4 {
        return Err(ToolError::InvalidArguments);
    }
    Ok(ComputerResponse { calls, message })
}

pub fn parse_action(
    v: &Value,
    observation: u64,
    focused: Option<u32>,
) -> Result<ComputerAction, ToolError> {
    let o = v.as_object().ok_or(ToolError::InvalidArguments)?;
    let ty = v["type"].as_str().ok_or(ToolError::InvalidArguments)?;
    let keys: &[&str] = match ty {
        "click" => &["type", "x", "y", "button"],
        "double_click" | "move" => &["type", "x", "y"],
        "drag" => &["type", "path"],
        "scroll" => &["type", "x", "y", "scroll_x", "scroll_y"],
        "type" => &["type", "text"],
        "keypress" => &["type", "keys"],
        "wait" | "screenshot" => &["type"],
        _ => return Err(ToolError::Unsupported),
    };
    if o.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err(ToolError::InvalidArguments);
    }
    let point = |p: &Value| -> Result<Target, ToolError> {
        let x = p["x"]
            .as_f64()
            .filter(|x| x.is_finite() && *x >= 0.)
            .ok_or(ToolError::InvalidArguments)?;
        let y = p["y"]
            .as_f64()
            .filter(|y| y.is_finite() && *y >= 0.)
            .ok_or(ToolError::InvalidArguments)?;
        Ok(Target::Point { observation, x, y })
    };
    Ok(match ty {
        "click" => ComputerAction::Click {
            target: point(v)?,
            button: serde_json::from_value(v["button"].clone())
                .map_err(|_| ToolError::InvalidArguments)?,
            count: 1,
        },
        "double_click" => ComputerAction::Click {
            target: point(v)?,
            button: MouseButton::Left,
            count: 2,
        },
        "move" => ComputerAction::Move { target: point(v)? },
        "drag" => {
            let path = v["path"]
                .as_array()
                .filter(|p| (2..=256).contains(&p.len()))
                .ok_or(ToolError::InvalidArguments)?;
            if path
                .iter()
                .any(|p| p.as_object().is_none_or(|o| o.len() != 2))
            {
                return Err(ToolError::InvalidArguments);
            }
            ComputerAction::Drag {
                points: path.iter().map(point).collect::<Result<_, _>>()?,
                button: MouseButton::Left,
                duration_ms: 500,
            }
        }
        "scroll" => {
            let delta = |k| {
                v[k].as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .filter(|n| n.unsigned_abs() <= 4096)
                    .ok_or(ToolError::InvalidArguments)
            };
            ComputerAction::Scroll {
                target: point(v)?,
                dx: delta("scroll_x")?,
                dy: delta("scroll_y")?,
                unit: ScrollUnit::Pixels,
            }
        }
        "type" => ComputerAction::Type {
            target: Target::Element {
                observation,
                mark: focused.ok_or(ToolError::NeedsUser)?,
            },
            text: v["text"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 8192)
                .ok_or(ToolError::InvalidArguments)?
                .into(),
        },
        "keypress" => {
            let keys: Vec<String> = serde_json::from_value(v["keys"].clone())
                .map_err(|_| ToolError::InvalidArguments)?;
            let keys = keys
                .into_iter()
                .map(|k| match k.to_ascii_lowercase().as_str() {
                    "arrowleft" => "left".into(),
                    "arrowright" => "right".into(),
                    "arrowup" => "up".into(),
                    "arrowdown" => "down".into(),
                    "control" => "ctrl".into(),
                    "meta" | "super" | "win" => "cmd".into(),
                    " " => "space".into(),
                    _ => k.to_ascii_lowercase(),
                })
                .collect::<Vec<_>>();
            if !valid_keys(&keys) {
                return Err(ToolError::InvalidArguments);
            }
            ComputerAction::Keypress { keys }
        }
        "wait" => ComputerAction::Wait { duration_ms: 1000 },
        "screenshot" => ComputerAction::Observe,
        _ => unreachable!(),
    })
}
/// Current protocol output. Legacy output remains in the comparison runner.
pub fn screenshot_output(call_id: &str, jpeg: &[u8]) -> Value {
    json!({"type":"computer_call_output","call_id":call_id,"output":{"type":"computer_screenshot","image_url":format!("data:image/jpeg;base64,{}",base64::engine::general_purpose::STANDARD.encode(jpeg))}})
}
