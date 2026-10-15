//! Discovery schema is narrowed to the executor's actual capabilities.
use super::*;
use serde_json::{Value, json};

fn object(fields: &[(&str, Value)]) -> Value {
    let properties: serde_json::Map<String, Value> = fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    json!({"type":"object","additionalProperties":false,"required":fields.iter().map(|(k,_)|*k).collect::<Vec<_>>(),"properties":properties})
}
pub fn with_schema(base: &Value, caps: &Capabilities) -> Value {
    if caps.actions.is_empty() {
        return base.clone();
    }
    let integer = json!({"type":"integer","minimum":0});
    let string = json!({"type":"string"});
    let target = json!({"anyOf":[
        object(&[("kind",json!({"type":"string","enum":["point"]})),("observation",integer.clone()),("x",json!({"type":"number"})),("y",json!({"type":"number"}))]),
        object(&[("kind",json!({"type":"string","enum":["element"]})),("observation",integer.clone()),("mark",integer.clone())])
    ]});
    let button = json!({"type":"string","enum":caps.buttons});
    let mut variants = vec![json!({"type":"null"})];
    for kind in &caps.actions {
        let name = serde_json::to_value(kind).expect("enum serializes");
        let mut fields = vec![("type", json!({"type":"string","enum":[name]}))];
        match kind {
            ActionKind::Observe => {}
            ActionKind::Click => fields.extend([
                ("target", target.clone()),
                ("button", button.clone()),
                ("count", json!({"type":"integer","enum":[1,2]})),
            ]),
            ActionKind::Move => fields.push(("target", target.clone())),
            ActionKind::Drag => fields.extend([
                (
                    "points",
                    json!({"type":"array","items":target,"minItems":2,"maxItems":256}),
                ),
                ("button", button.clone()),
                (
                    "duration_ms",
                    json!({"type":"integer","minimum":0,"maximum":5000}),
                ),
            ]),
            ActionKind::Type => {
                fields.extend([("target", target.clone()), ("text", string.clone())])
            }
            ActionKind::Keypress => fields.push((
                "keys",
                json!({"type":"array","items":string,"minItems":1,"maxItems":8}),
            )),
            ActionKind::Scroll => fields.extend([
                ("target", target.clone()),
                ("dx", json!({"type":"integer"})),
                ("dy", json!({"type":"integer"})),
                ("unit", json!({"type":"string","enum":caps.scroll_units})),
            ]),
            ActionKind::Wait => fields.push((
                "duration_ms",
                json!({"type":"integer","minimum":0,"maximum":3000}),
            )),
            ActionKind::OpenApp => fields.push(("name", string.clone())),
            ActionKind::OpenUrl => fields.push(("url", string.clone())),
        }
        variants.push(object(&fields));
    }
    let mut s = base.clone();
    if let Some(actions) = s["properties"]["action"]["enum"].as_array_mut() {
        actions.push(json!("computer"));
    }
    s["properties"]["computer"] = json!({"anyOf":variants});
    s
}
