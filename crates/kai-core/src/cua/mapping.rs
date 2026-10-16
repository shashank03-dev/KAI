//! Legacy mapping is deliberately limited; unsupported input must be explicit.
use crate::{Decision, Element, Grounding, Rect, Role};
/// What a native action becomes.
pub enum Mapped {
    Act(Decision, Grounding),
    Ask(String),
}

/// Build a [`Decision`] from JSON, so new optional fields keep their
/// defaults. The model's fields are copied in as sent, so a wrong type
/// (`"text": 5`) is refused, never a panic.
fn decision(v: serde_json::Value) -> Option<Decision> {
    let mut d: Decision = serde_json::from_value(v).ok()?;
    d.validate_and_clamp().ok()?;
    Some(d)
}

/// Map a `computer_call.action` to a Kai decision plus a one-element grounding
/// at the global point. `img` is the screenshot size the model saw; `screen`
/// the captured display in global pixels.
pub fn map_action(a: &serde_json::Value, img: (u32, u32), screen: Rect) -> Mapped {
    let ty = a["type"].as_str().unwrap_or("");
    if matches!(ty, "click" | "double_click" | "scroll" | "move") {
        let space = crate::computer::ObservationSpace {
            session: 0,
            id: 0,
            image_width: img.0,
            image_height: img.1,
            region: screen,
        };
        let valid = a["x"]
            .as_f64()
            .zip(a["y"].as_f64())
            .is_some_and(|(x, y)| space.point(x, y).is_ok());
        if !valid {
            return Mapped::Ask("Invalid native coordinates".into());
        }
    }
    if ty == "scroll" {
        return Mapped::Ask("Pixel scroll requires the shared pixel-capable adapter".into());
    }
    if ty == "click" && !matches!(a["button"].as_str(), Some("left" | "right")) {
        return Mapped::Ask("Unsupported native button".into());
    }
    let point = |x: &serde_json::Value, y: &serde_json::Value| {
        let sx = screen.w / img.0.max(1) as f32;
        let sy = screen.h / img.1.max(1) as f32;
        let gx = screen.x + x.as_f64().unwrap_or(0.0) as f32 * sx;
        let gy = screen.y + y.as_f64().unwrap_or(0.0) as f32 * sy;
        Grounding::Marks(vec![Element {
            mark: 1,
            role: Role::Other,
            name: "native pointer".into(),
            bounds: Rect::new(gx - 0.5, gy - 0.5, 1.0, 1.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }])
    };
    let none = || Grounding::Marks(Vec::new());
    let at = || point(&a["x"], &a["y"]);
    let act = |v: serde_json::Value, g: Grounding| match decision(v) {
        Some(d) => Mapped::Act(d, g),
        None => Mapped::Ask("Invalid native action fields".into()),
    };
    match ty {
        "click" => match a["button"].as_str().unwrap_or("left") {
            "right" => act(
                serde_json::json!({"thought":"native right click","action":"right_click","target":1,"risk":"none"}),
                at(),
            ),
            _ => act(
                serde_json::json!({"thought":"native click","action":"click","target":1,"risk":"none"}),
                at(),
            ),
        },
        "double_click" => act(
            serde_json::json!({"thought":"native double click","action":"double_click","target":1,"risk":"none"}),
            at(),
        ),
        "type" => act(
            serde_json::json!({"thought":"native type","action":"type","text":a["text"],"risk":"none"}),
            none(),
        ),
        "keypress" => {
            let keys: Vec<String> = a["keys"]
                .as_array()
                .map(|k| k.iter().filter_map(|s| s.as_str()).map(key_name).collect())
                .unwrap_or_default();
            if !crate::computer::valid_keys(&keys) {
                return Mapped::Ask("Invalid native keys".into());
            }
            act(
                serde_json::json!({"thought":"native keypress","action":"key","keys":keys,"risk":"none"}),
                none(),
            )
        }
        "wait" | "screenshot" => act(
            serde_json::json!({"thought":format!("native {ty}"),"action":"wait","risk":"none"}),
            none(),
        ),
        "move" | "drag" => Mapped::Ask("Move and drag require shared versioned dispatch".into()),
        other => Mapped::Ask(format!("Unknown native action `{other}`")),
    }
}

/// CUA key names (`CTRL`, `ARROWLEFT`, `ENTER`) to Kai's.
fn key_name(k: &str) -> String {
    match k.to_ascii_lowercase().as_str() {
        "arrowleft" => "left".into(),
        "arrowright" => "right".into(),
        "arrowup" => "up".into(),
        "arrowdown" => "down".into(),
        "control" => "ctrl".into(),
        "super" | "meta" | "win" | "cmd" => "cmd".into(),
        " " => "space".into(),
        other => other.to_string(),
    }
}
