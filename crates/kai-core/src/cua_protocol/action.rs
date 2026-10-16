//! One OpenAI action object to Kai's canonical action. Unknown fields,
//! buttons and key names are refused: nothing unvetted reaches OS input.
use super::present;
use crate::computer::*;
use serde_json::{Map, Value};

/// OpenAI recipes sleep this long for a bare `wait`.
const WAIT_MS: u32 = 2000;

/// OpenAI's key names (uppercase, with aliases) to Kai's canonical names.
fn key_name(k: &str) -> String {
    match k.to_ascii_lowercase().as_str() {
        "return" => "enter".into(),
        "escape" => "esc".into(),
        "control" => "ctrl".into(),
        "option" => "alt".into(),
        "meta" | "command" | "super" | "win" => "cmd".into(),
        "arrowleft" => "left".into(),
        "arrowright" => "right".into(),
        "arrowup" => "up".into(),
        "arrowdown" => "down".into(),
        " " => "space".into(),
        other => other.into(),
    }
}

fn keys(v: Option<&Value>) -> Result<Vec<String>, ToolError> {
    let Some(v) = present(v) else {
        return Ok(Vec::new());
    };
    let names: Vec<String> =
        serde_json::from_value(v.clone()).map_err(|_| ToolError::InvalidArguments)?;
    Ok(names.iter().map(|k| key_name(k)).collect())
}

/// `wheel` is OpenAI's middle button; `middle` is kept for older inputs.
fn button(v: &Value) -> Result<MouseButton, ToolError> {
    Ok(match v.as_str() {
        Some("left") => MouseButton::Left,
        Some("right") => MouseButton::Right,
        Some("wheel" | "middle") => MouseButton::Middle,
        Some("back") => MouseButton::Back,
        Some("forward") => MouseButton::Forward,
        _ => return Err(ToolError::InvalidArguments),
    })
}

fn allowed(ty: &str) -> Result<&'static [&'static str], ToolError> {
    Ok(match ty {
        "click" => &["type", "x", "y", "button", "keys"],
        "double_click" | "move" => &["type", "x", "y", "keys"],
        "drag" => &["type", "path", "keys"],
        "scroll" => &["type", "x", "y", "scroll_x", "scroll_y", "keys"],
        "type" => &["type", "text"],
        "keypress" => &["type", "keys"],
        "wait" => &["type", "ms"],
        "screenshot" => &["type"],
        _ => return Err(ToolError::Unsupported),
    })
}

/// The object without its `null` fields, which mean "not given".
fn fields(v: &Value) -> Result<Map<String, Value>, ToolError> {
    let o = v.as_object().ok_or(ToolError::InvalidArguments)?;
    Ok(o.iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect())
}

pub fn parse_action(
    v: &Value,
    observation: u64,
    focused: Option<u32>,
) -> Result<ComputerAction, ToolError> {
    let o = fields(v)?;
    let ty = o
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ToolError::InvalidArguments)?;
    let accepted = allowed(ty)?;
    if o.keys().any(|k| !accepted.contains(&k.as_str())) {
        return Err(ToolError::InvalidArguments);
    }
    let point = |p: &Value| -> Result<Target, ToolError> {
        let coord = |k: &str| {
            p[k].as_f64()
                .filter(|c| c.is_finite() && *c >= 0.)
                .ok_or(ToolError::InvalidArguments)
        };
        Ok(Target::Point {
            observation,
            x: coord("x")?,
            y: coord("y")?,
        })
    };
    let at = || point(v);
    let modifiers = || -> Result<Vec<String>, ToolError> {
        let held = keys(o.get("keys"))?;
        if valid_modifiers(&held) {
            Ok(held)
        } else {
            Err(ToolError::InvalidArguments)
        }
    };
    Ok(match ty {
        "click" => ComputerAction::Click {
            target: at()?,
            button: button(&v["button"])?,
            count: 1,
            modifiers: modifiers()?,
        },
        "double_click" => ComputerAction::Click {
            target: at()?,
            button: MouseButton::Left,
            count: 2,
            modifiers: modifiers()?,
        },
        "move" => ComputerAction::Move {
            target: at()?,
            modifiers: modifiers()?,
        },
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
                modifiers: modifiers()?,
            }
        }
        "scroll" => {
            let delta = |k: &str| {
                v[k].as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .filter(|n| n.unsigned_abs() <= 4096)
                    .ok_or(ToolError::InvalidArguments)
            };
            ComputerAction::Scroll {
                target: at()?,
                dx: delta("scroll_x")?,
                dy: delta("scroll_y")?,
                unit: ScrollUnit::Pixels,
                modifiers: modifiers()?,
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
            let keys = keys(o.get("keys"))?;
            if !valid_keys(&keys) {
                return Err(ToolError::InvalidArguments);
            }
            ComputerAction::Keypress { keys }
        }
        "wait" => ComputerAction::Wait {
            duration_ms: match o.get("ms") {
                None => WAIT_MS,
                Some(ms) => ms
                    .as_u64()
                    .map(|ms| ms.min(3000) as u32)
                    .ok_or(ToolError::InvalidArguments)?,
            },
        },
        "screenshot" => ComputerAction::Observe,
        _ => unreachable!("allowed() lists every type"),
    })
}
