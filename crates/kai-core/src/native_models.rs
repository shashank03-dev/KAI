//! Strict adapters for computer-use models' original action languages.
//! These parsers only produce canonical actions; the engine's shared executor
//! remains responsible for capabilities, stale observations, risk and approval.
use crate::computer::{ComputerAction as A, MouseButton, ScrollUnit, Target};
use crate::oai_contract::CoordFrame;
use anyhow::{Context, bail, ensure};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeModel {
    Fara15,
    UiTars15,
    Holo31,
}

#[derive(Debug, PartialEq)]
pub enum NativeTurn {
    Act(A),
    Done(String),
    Ask(String),
}

impl NativeModel {
    pub fn frame(self) -> CoordFrame {
        match self {
            Self::Fara15 => CoordFrame::Pixels,
            Self::UiTars15 => CoordFrame::SmartResize,
            Self::Holo31 => CoordFrame::Norm1000,
        }
    }

    /// Holo's native function-calling channel, sent as OpenAI-compatible
    /// `tools` rather than embedding a pretend JSON action in plain text.
    pub fn tool_spec(self) -> Option<Value> {
        matches!(self, Self::Holo31).then(|| serde_json::json!({
            "type": "function",
            "function": {
                "name": "computer_use",
                "description": "Perform one bounded computer action on the visible screen, or finish or ask the user.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {"type":"string", "enum": ["left_click","double_click","right_click","mouse_move","key","type","scroll","hscroll","visit_url","wait","terminate","ask_user_question"]},
                        "coordinate": {"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"description":"x,y from 0 to 1000 relative to screenshot"},
                        "keys": {"type":"array","items":{"type":"string"}},
                        "text": {"type":"string"},
                        "pixels": {"type":"integer","minimum":-4096,"maximum":4096},
                        "url": {"type":"string"},
                        "time": {"type":"number","minimum":0,"maximum":3},
                        "answer": {"type":"string"},
                        "question": {"type":"string"}
                    },
                    "required": ["action"],
                    "additionalProperties": false
                }
            }
        }))
    }

    pub fn system(self, size: (u32, u32)) -> String {
        match self {
            Self::Fara15 => format!(
                "You are Fara, a computer use agent (CUA) specialized for web browsers. \
                 You are developed by Microsoft AI Frontiers. You assist users with completing \
                 and automating tasks that require the use of a web browser.\n\n\
                 Critical points: Never fabricate missing personal information. Ask for details \
                 when a task is underspecified. Ask before an irreversible action unless the user \
                 explicitly authorized it.\n\n\
                 You are provided with function signatures within <tools></tools> XML tags:\n\
                 <tools>\n{{\"name\":\"computer_use\",\"description\":\"Browser computer control\",\"parameters\":{{\"type\":\"object\",\"properties\":{{\
                 \"action\":{{\"type\":\"string\",\"enum\":[\"key\",\"type\",\"mouse_move\",\"left_click\",\"double_click\",\"right_click\",\"scroll\",\"hscroll\",\"visit_url\",\"ask_user_question\",\"wait\",\"terminate\"]}},\
                 \"coordinate\":{{\"type\":\"array\",\"description\":\"[x,y] in screenshot pixels\"}},\
                 \"keys\":{{\"type\":\"array\"}},\"text\":{{\"type\":\"string\"}},\"pixels\":{{\"type\":\"number\",\"description\":\"positive scrolls up\"}},\
                 \"url\":{{\"type\":\"string\"}},\"question\":{{\"type\":\"string\"}},\"time\":{{\"type\":\"number\"}},\"answer\":{{\"type\":\"string\"}}}},\"required\":[\"action\"]}}}}\n</tools>\n\n\
                 For each function call, return a json object with function name and arguments \
                 within <tool_call></tool_call> XML tags:\n\
                 <tool_call>\n{{\"name\":\"computer_use\",\"arguments\":{{\"action\":\"left_click\",\"coordinate\":[100,100]}}}}\n</tool_call>\n\n\
                 Screenshot dimensions: {} x {} pixels. Type only into the focused field. \
                 Some Fara actions are unavailable on this driver; use only the actions listed. \
                 A tool call is not proof that the task succeeded.",
                size.0, size.1
            ),
            Self::UiTars15 => format!(
                "You are a GUI agent. You are given a task and action history, with screenshots. \
                 Perform the next action.\nOutput Format:\nThought: ...\nAction: ...\n\
                 Action Space:\nclick(point='<point>x y</point>')\n\
                 left_double(point='<point>x y</point>')\nright_single(point='<point>x y</point>')\n\
                 drag(start_point='<point>x y</point>', end_point='<point>x y</point>')\n\
                 hotkey(key='ctrl c')\ntype(content='text')\n\
                 scroll(point='<point>x y</point>', direction='down')\n\
                 wait()\nfinished(content='summary')\n\n\
                 Coordinates are in the Qwen2.5-VL smart-resized image frame. \
                 Original image: {} x {} pixels. Only call one action each turn. \
                 Never output executable Python code.",
                size.0, size.1
            ),
            Self::Holo31 => format!(
                "You are Holo3.1, a visual computer-use agent. Inspect the screenshot and call \
                 the provided computer_use function with exactly one action per turn. \
                 Coordinates are normalized to 0..1000 on each axis of the original screenshot \
                 ({} by {} pixels). Only type into the focused field. Positive scroll pixels \
                 move up. Call terminate with an answer only after the requested result is \
                 visibly verified. Ask the user about missing personal details and irreversible \
                 changes. Never generate Python, scripts, or another tool name. \
                 Your function call is a proposed action, never evidence of completion.",
                size.0, size.1
            ),
        }
    }

    pub fn decode(
        self,
        raw: &str,
        observation: u64,
        focused: Option<u32>,
        size: (u32, u32),
    ) -> anyhow::Result<NativeTurn> {
        ensure!(
            size.0 > 0 && size.1 > 0,
            "native model requires screenshot dimensions"
        );
        match self {
            Self::Fara15 => fara(raw, observation, focused, size),
            Self::UiTars15 => ui_tars(raw, observation, focused, size),
            Self::Holo31 => {
                let UniqueKeys(args) =
                    serde_json::from_str(raw).context("invalid Holo tool arguments")?;
                native_arguments(&args, CoordFrame::Norm1000, observation, focused, size)
            }
        }
    }
}

fn fields<'a>(value: &'a Value, allowed: &[&str]) -> anyhow::Result<&'a Map<String, Value>> {
    let obj = value.as_object().context("expected JSON object")?;
    ensure!(
        obj.keys().all(|k| allowed.contains(&k.as_str())),
        "unknown native action field"
    );
    Ok(obj)
}

/// JSON's usual Value decoder keeps only the last occurrence of a key.
/// Native tool calls must reject ambiguous envelopes and nested arguments.
struct UniqueKeys(Value);

impl<'de> Deserialize<'de> for UniqueKeys {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictJson;
        impl<'de> Visitor<'de> for StrictJson {
            type Value = UniqueKeys;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON value without duplicate object keys")
            }

            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueKeys(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueKeys(Value::Number(v.into())))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueKeys(Value::Number(v.into())))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                let n = serde_json::Number::from_f64(v)
                    .ok_or_else(|| E::custom("non-finite JSON number"))?;
                Ok(UniqueKeys(Value::Number(n)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueKeys(Value::String(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueKeys(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueKeys(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                self.visit_unit()
            }
            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                UniqueKeys::deserialize(deserializer)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element::<UniqueKeys>()? {
                    items.push(item.0);
                }
                Ok(UniqueKeys(Value::Array(items)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut fields = Map::new();
                while let Some((key, value)) = map.next_entry::<String, UniqueKeys>()? {
                    if fields.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate JSON key: {key}")));
                    }
                    fields.insert(key, value.0);
                }
                Ok(UniqueKeys(Value::Object(fields)))
            }
        }
        deserializer.deserialize_any(StrictJson)
    }
}

fn string<'a>(obj: &'a Map<String, Value>, field: &str) -> anyhow::Result<&'a str> {
    obj.get(field)
        .and_then(Value::as_str)
        .context(format!("missing {field}"))
}

fn point(x: f64, y: f64, frame: CoordFrame, size: (u32, u32), id: u64) -> anyhow::Result<Target> {
    let (x, y) = frame
        .to_image(x, y, size)
        .context("point outside screenshot frame")?;
    Ok(Target::Point {
        observation: id,
        x,
        y,
    })
}

fn coord(
    obj: &Map<String, Value>,
    key: &str,
    frame: CoordFrame,
    size: (u32, u32),
    id: u64,
) -> anyhow::Result<Target> {
    let c = obj
        .get(key)
        .and_then(Value::as_array)
        .context(format!("missing {key}"))?;
    ensure!(c.len() == 2, "coordinate must have exactly two numbers");
    point(
        c[0].as_f64().context("invalid x")?,
        c[1].as_f64().context("invalid y")?,
        frame,
        size,
        id,
    )
}

fn focused_target(focused: Option<u32>, id: u64) -> anyhow::Result<Target> {
    Ok(Target::Element {
        observation: id,
        mark: focused
            .filter(|&n| n > 0)
            .context("typing requires a focused field")?,
    })
}

fn key_name(k: &str) -> String {
    match k.to_ascii_lowercase().as_str() {
        "control" => "ctrl",
        "option" => "alt",
        "command" | "meta" | "super" | "win" => "cmd",
        "escape" => "esc",
        "return" => "enter",
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        "pagedown" => "pagedown",
        "pageup" => "pageup",
        other => return other.to_owned(),
    }
    .to_string()
}

fn keys(values: &[Value]) -> anyhow::Result<Vec<String>> {
    let keys: Vec<String> = values
        .iter()
        .map(|v| v.as_str().map(key_name).context("key must be a string"))
        .collect::<Result<_, _>>()?;
    ensure!(
        crate::computer::valid_keys(&keys),
        "invalid key combination"
    );
    Ok(keys)
}

fn click(target: Target, button: MouseButton, count: u8) -> A {
    A::Click {
        target,
        button,
        count,
        modifiers: vec![],
    }
}

fn fara(raw: &str, id: u64, focused: Option<u32>, size: (u32, u32)) -> anyhow::Result<NativeTurn> {
    let (_, after) = raw
        .split_once("<tool_call>")
        .context("missing Fara <tool_call>")?;
    let (body, tail) = after
        .split_once("</tool_call>")
        .context("unclosed Fara tool call")?;
    ensure!(
        !tail.contains("<tool_call>"),
        "multiple Fara tool calls are unsupported"
    );
    let UniqueKeys(v) = serde_json::from_str(body.trim()).context("invalid Fara JSON")?;
    let env = fields(&v, &["name", "arguments"])?;
    ensure!(
        string(env, "name")? == "computer_use",
        "unexpected Fara tool"
    );
    let args = env.get("arguments").context("missing arguments")?;
    native_arguments(args, CoordFrame::Pixels, id, focused, size)
}

/// Decode validated JSON tool arguments from either Fara's XML envelope or
/// Holo's native API `tool_calls`, using the model's own coordinate frame.
fn native_arguments(
    args: &Value,
    frame: CoordFrame,
    id: u64,
    focused: Option<u32>,
    size: (u32, u32),
) -> anyhow::Result<NativeTurn> {
    let a = args.as_object().context("arguments must be object")?;
    let kind = string(a, "action")?;
    let allowed: &[&str] = match kind {
        "left_click" | "double_click" | "right_click" | "mouse_move" => &["action", "coordinate"],
        "scroll" | "hscroll" => &["action", "pixels"],
        "key" => &["action", "keys"],
        "type" => &["action", "text"],
        "visit_url" => &["action", "url"],
        "wait" => &["action", "time"],
        "terminate" => &["action", "answer"],
        "ask_user_question" => &["action", "question"],
        _ => bail!("unsupported native action: {kind}"),
    };
    fields(args, allowed)?;
    let act = match kind {
        "left_click" => click(
            coord(a, "coordinate", frame, size, id)?,
            MouseButton::Left,
            1,
        ),
        "double_click" => click(
            coord(a, "coordinate", frame, size, id)?,
            MouseButton::Left,
            2,
        ),
        "right_click" => click(
            coord(a, "coordinate", frame, size, id)?,
            MouseButton::Right,
            1,
        ),
        "mouse_move" => A::Move {
            target: coord(a, "coordinate", frame, size, id)?,
            modifiers: vec![],
        },
        "key" => A::Keypress {
            keys: keys(
                a.get("keys")
                    .and_then(Value::as_array)
                    .context("missing keys")?,
            )?,
        },
        "type" => A::Type {
            target: focused_target(focused, id)?,
            text: string(a, "text")?.to_owned(),
        },
        "scroll" | "hscroll" => {
            let px = a
                .get("pixels")
                .and_then(Value::as_f64)
                .context("missing pixels")?;
            ensure!(
                px.is_finite() && px.fract() == 0. && px.abs() <= 4096.,
                "scroll must be an integer within 4096 pixels"
            );
            let signed = -(px as i32); // Fara positive=up; Kai positive=down.
            A::Scroll {
                target: point(
                    size.0 as f64 / 2.,
                    size.1 as f64 / 2.,
                    CoordFrame::Pixels,
                    size,
                    id,
                )?,
                dx: if kind == "hscroll" { signed } else { 0 },
                dy: if kind == "scroll" { signed } else { 0 },
                unit: ScrollUnit::Pixels,
                modifiers: vec![],
            }
        }
        "visit_url" => A::OpenUrl {
            url: string(a, "url")?.to_owned(),
        },
        "wait" => {
            let sec = a
                .get("time")
                .and_then(Value::as_f64)
                .context("missing time")?;
            ensure!(
                sec.is_finite() && (0. ..=3.).contains(&sec),
                "wait exceeds three seconds"
            );
            A::Wait {
                duration_ms: (sec * 1000.).round() as u32,
            }
        }
        "terminate" => return Ok(NativeTurn::Done(string(a, "answer")?.to_owned())),
        "ask_user_question" => return Ok(NativeTurn::Ask(string(a, "question")?.to_owned())),
        _ => unreachable!(),
    };
    Ok(NativeTurn::Act(act))
}

/// Parse a single UI-TARS action as *text*, never as executable Python.
/// Only literal single/double quoted argument strings are accepted.
fn ui_tars(
    raw: &str,
    id: u64,
    focused: Option<u32>,
    size: (u32, u32),
) -> anyhow::Result<NativeTurn> {
    let (_, action) = raw
        .split_once("Action:")
        .context("missing UI-TARS Action:")?;
    let action = action.trim().trim_end_matches("[EOS]").trim();
    let open = action.find('(').context("missing function arguments")?;
    ensure!(action.ends_with(')'), "unclosed UI-TARS action");
    let name = action[..open].trim();
    ensure!(
        name.bytes().all(|c| c.is_ascii_alphabetic() || c == b'_'),
        "invalid function name"
    );
    let args = parse_literal_args(&action[open + 1..action.len() - 1])?;
    let allowed: &[&str] = match name {
        "click" | "left_double" | "right_single" => &["point"],
        "drag" => &["start_point", "end_point"],
        "scroll" => &["point", "direction"],
        "hotkey" => &["key"],
        "type" | "finished" => &["content"],
        "wait" => &[],
        _ => bail!("unsupported UI-TARS action: {name}"),
    };
    ensure!(
        args.keys().all(|k| allowed.contains(&k.as_str())),
        "unknown UI-TARS field"
    );
    let get = |k: &str| -> anyhow::Result<&str> {
        args.get(k)
            .map(String::as_str)
            .context(format!("missing {k}"))
    };
    let pt = |s: &str| -> anyhow::Result<Target> {
        let inner = s
            .strip_prefix("<point>")
            .and_then(|s| s.strip_suffix("</point>"))
            .context("invalid <point> tag")?;
        let vals: Vec<_> = inner.split_whitespace().collect();
        ensure!(vals.len() == 2, "point requires x y");
        point(
            vals[0].parse()?,
            vals[1].parse()?,
            CoordFrame::SmartResize,
            size,
            id,
        )
    };
    let action = match name {
        "click" => click(pt(get("point")?)?, MouseButton::Left, 1),
        "left_double" => click(pt(get("point")?)?, MouseButton::Left, 2),
        "right_single" => click(pt(get("point")?)?, MouseButton::Right, 1),
        "drag" => A::Drag {
            points: vec![pt(get("start_point")?)?, pt(get("end_point")?)?],
            button: MouseButton::Left,
            duration_ms: 500,
            modifiers: vec![],
        },
        "hotkey" => A::Keypress {
            keys: keys(
                &get("key")?
                    .split_whitespace()
                    .map(|k| Value::String(k.to_owned()))
                    .collect::<Vec<_>>(),
            )?,
        },
        "type" => A::Type {
            target: focused_target(focused, id)?,
            text: get("content")?.to_owned(),
        },
        "scroll" => {
            let (dx, dy) = match get("direction")? {
                "up" => (0, -600),
                "down" => (0, 600),
                "left" => (-600, 0),
                "right" => (600, 0),
                _ => bail!("invalid scroll direction"),
            };
            A::Scroll {
                target: pt(get("point")?)?,
                dx,
                dy,
                unit: ScrollUnit::Pixels,
                modifiers: vec![],
            }
        }
        "wait" => A::Wait { duration_ms: 3000 },
        "finished" => return Ok(NativeTurn::Done(get("content")?.to_owned())),
        _ => unreachable!(),
    };
    Ok(NativeTurn::Act(action))
}

fn parse_literal_args(s: &str) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let mut out = std::collections::BTreeMap::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        let (name, value) = rest.split_once('=').context("expected named argument")?;
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "invalid argument name"
        );
        let value = value.trim_start();
        let quote = value.chars().next().context("missing quoted value")?;
        ensure!(
            matches!(quote, '\'' | '"'),
            "arguments must be quoted literals"
        );
        let mut decoded = String::new();
        let mut escaped = false;
        let mut end = None;
        for (i, c) in value[1..].char_indices() {
            if escaped {
                decoded.push(match c {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    _ => bail!("unknown string escape"),
                });
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == quote {
                end = Some(i + 2);
                break;
            } else {
                decoded.push(c);
            }
        }
        let end = end.context("unclosed string literal")?;
        ensure!(
            out.insert(name.to_owned(), decoded).is_none(),
            "duplicate argument"
        );
        rest = value[end..].trim_start();
        if !rest.is_empty() {
            rest = rest
                .strip_prefix(',')
                .context("unexpected code after argument")?
                .trim_start();
            ensure!(!rest.is_empty(), "trailing comma");
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fara_call(args: &str) -> String {
        format!("<tool_call>{{\"name\":\"computer_use\",\"arguments\":{args}}}</tool_call>")
    }
    #[test]
    fn fara_native_actions_and_scroll_sign() {
        let a = NativeModel::Fara15
            .decode(
                &fara_call(r#"{"action":"left_click","coordinate":[50,75]}"#),
                9,
                None,
                (1440, 900),
            )
            .unwrap();
        assert_eq!(
            a,
            NativeTurn::Act(click(
                Target::Point {
                    observation: 9,
                    x: 50.,
                    y: 75.
                },
                MouseButton::Left,
                1
            ))
        );
        assert!(matches!(
            NativeModel::Fara15
                .decode(
                    &fara_call(r#"{"action":"scroll","pixels":400}"#),
                    9,
                    None,
                    (1440, 900)
                )
                .unwrap(),
            NativeTurn::Act(A::Scroll { dy: -400, .. })
        ));
        assert!(matches!(
            NativeModel::Fara15
                .decode(
                    &fara_call(r#"{"action":"terminate","answer":"done"}"#),
                    9,
                    None,
                    (1440, 900)
                )
                .unwrap(),
            NativeTurn::Done(_)
        ));
    }
    #[test]
    fn fara_rejects_extra_tools_bad_points_and_unsafe_actions() {
        for args in [
            r#"{"action":"left_click","coordinate":[2000,2]}"#,
            r#"{"action":"triple_click","coordinate":[1,2]}"#,
            r#"{"action":"visit_url","url":"https://example.com","shell":"rm -rf ~"}"#,
        ] {
            assert!(
                NativeModel::Fara15
                    .decode(&fara_call(args), 1, None, (1280, 800))
                    .is_err()
            );
        }
        let twice = fara_call(r#"{"action":"wait","time":1}"#).repeat(2);
        assert!(
            NativeModel::Fara15
                .decode(&twice, 1, None, (1280, 800))
                .is_err()
        );
        assert!(
            NativeModel::Fara15
                .decode(
                    &fara_call(r#"{"action":"type","text":"secret"}"#),
                    1,
                    None,
                    (1280, 800)
                )
                .is_err()
        );
    }
    #[test]
    fn fara_rejects_duplicate_json_keys_before_action_decoding() {
        let duplicated_action = fara_call(
            r#"{"action":"left_click","action":"visit_url","coordinate":[1,2],"url":"https://example.com"}"#,
        );
        let duplicated_point =
            fara_call(r#"{"action":"left_click","coordinate":[1,2],"coordinate":[3,4]}"#);
        let duplicated_envelope = r#"<tool_call>{"name":"ignored","name":"computer_use","arguments":{"action":"wait","time":1}}</tool_call>"#;
        for raw in [
            duplicated_action.as_str(),
            duplicated_point.as_str(),
            duplicated_envelope,
        ] {
            assert!(
                NativeModel::Fara15
                    .decode(raw, 1, None, (1280, 800))
                    .is_err(),
                "duplicate JSON keys must be rejected: {raw}"
            );
        }
    }
    #[test]
    fn holo_function_calls_are_normalized_and_strict() {
        let model = NativeModel::Holo31;
        assert_eq!(model.frame(), CoordFrame::Norm1000);
        assert_eq!(
            model.tool_spec().unwrap()["function"]["name"],
            "computer_use"
        );
        assert!(matches!(
            model.decode(r#"{"action":"left_click","coordinate":[500,500]}"#, 7, None, (1280,800)).unwrap(),
            NativeTurn::Act(A::Click { target: Target::Point { x, y, .. }, .. })
                if (x - 640.).abs() < 1. && (y - 400.).abs() < 1.
        ));
        assert_eq!(
            model
                .decode(
                    r#"{"action":"terminate","answer":"Verified"}"#,
                    7,
                    None,
                    (1280, 800)
                )
                .unwrap(),
            NativeTurn::Done("Verified".into())
        );
        for raw in [
            r#"{"action":"left_click","coordinate":[1001,500]}"#,
            r#"{"action":"left_click","coordinate":[500,500],"action":"wait"}"#,
            r#"{"action":"visit_url","url":"https://example.com","shell":"rm -rf /"}"#,
            r#"{"action":"type","text":"private"}"#,
            r#"{"action":"run_python","code":"import os"}"#,
            r#"<tool_call>{"action":"wait"}</tool_call>"#,
        ] {
            assert!(
                model.decode(raw, 7, None, (1280, 800)).is_err(),
                "accepted {raw}"
            );
        }
        assert!(matches!(
            model
                .decode(
                    r#"{"action":"type","text":"test"}"#,
                    7,
                    Some(2),
                    (1280, 800)
                )
                .unwrap(),
            NativeTurn::Act(A::Type { .. })
        ));
    }

    #[test]
    fn tars_actions_are_decoded_without_running_code() {
        let raw = "Thought: click search\nAction: click(point='<point>140 224</point>')";
        assert!(matches!(
            NativeModel::UiTars15
                .decode(raw, 3, None, (1280, 800))
                .unwrap(),
            NativeTurn::Act(A::Click { .. })
        ));
        assert!(matches!(
            NativeModel::UiTars15
                .decode("Action: hotkey(key='ctrl c')", 3, None, (1280, 800))
                .unwrap(),
            NativeTurn::Act(A::Keypress { .. })
        ));
        assert!(
            NativeModel::UiTars15
                .decode(
                    "Action: __import__('os').system('whoami')",
                    3,
                    None,
                    (1280, 800)
                )
                .is_err()
        );
        assert!(
            NativeModel::UiTars15
                .decode(
                    "Action: click(point='<point>99999 0</point>')",
                    3,
                    None,
                    (1280, 800)
                )
                .is_err()
        );
    }
}
