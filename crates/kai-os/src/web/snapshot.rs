//! A page as Kai's marks: the accessibility tree (`Accessibility.getFullAXTree`:
//! roles, names, values, disabled) joined on the backend node id with the DOM
//! snapshot (`DOMSnapshot.captureSnapshot`: layout boxes, `visibility`, input
//! types). Two protocol calls per observation, no per-node round trips.
//! Pure functions over the protocol's JSON, so they are tested on fixtures.

use crate::a11y::RawNode;
use kai_core::{Rect, Role};
use serde_json::Value;
use std::collections::HashMap;

/// What the DOM snapshot says about the page as a whole.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
    /// Document scroll, in CSS pixels; layout boxes are document-relative.
    pub scroll: (f32, f32),
}

/// Layout facts for one DOM node.
#[derive(Debug, Clone, Copy)]
struct Layout {
    bounds: Rect,
    visible: bool,
    password: bool,
}

/// The main document's layout per backend node id, and the page facts.
fn layout(snapshot: &Value) -> (HashMap<i64, Layout>, PageInfo) {
    let strings: Vec<&str> = snapshot["strings"]
        .as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap_or("")).collect())
        .unwrap_or_default();
    let s = |v: &Value| -> &str {
        v.as_i64()
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| strings.get(i).copied())
            .unwrap_or("")
    };
    let doc = &snapshot["documents"][0];
    let f = |v: &Value| v.as_f64().unwrap_or(0.0) as f32;
    let info = PageInfo {
        url: s(&doc["documentURL"]).to_string(),
        title: s(&doc["title"]).to_string(),
        scroll: (f(&doc["scrollOffsetX"]), f(&doc["scrollOffsetY"])),
    };
    let nodes = &doc["nodes"];
    let backend = nodes["backendNodeId"].as_array();
    let names = nodes["nodeName"].as_array();
    let attrs = nodes["attributes"].as_array();
    let lay = &doc["layout"];
    let (Some(backend), Some(index), Some(bounds)) = (
        backend,
        lay["nodeIndex"].as_array(),
        lay["bounds"].as_array(),
    ) else {
        return (HashMap::new(), info);
    };
    let styles = lay["styles"].as_array();
    let mut out = HashMap::new();
    for (i, ni) in index.iter().enumerate() {
        let Some(ni) = ni.as_u64().map(|n| n as usize) else {
            continue;
        };
        let Some(id) = backend.get(ni).and_then(Value::as_i64) else {
            continue;
        };
        let b = &bounds[i];
        let rect = Rect::new(f(&b[0]), f(&b[1]), f(&b[2]), f(&b[3]));
        let visible = styles
            .and_then(|st| st.get(i))
            .and_then(|st| st.get(0))
            .map(|v| !matches!(s(v), "hidden" | "collapse"))
            .unwrap_or(true);
        let is_input = names
            .and_then(|n| n.get(ni))
            .is_some_and(|n| s(n).eq_ignore_ascii_case("input"));
        let password = is_input
            && attrs
                .and_then(|a| a.get(ni))
                .and_then(Value::as_array)
                .is_some_and(|kv| {
                    kv.chunks(2).any(|p| {
                        p.len() == 2
                            && s(&p[0]).eq_ignore_ascii_case("type")
                            && s(&p[1]).eq_ignore_ascii_case("password")
                    })
                });
        out.insert(
            id,
            Layout {
                bounds: rect,
                visible,
                password,
            },
        );
    }
    (out, info)
}

/// Chrome's accessibility roles mapped to Kai's. `None`: not an element the
/// model acts on (text, headings, containers, the page itself).
fn role(ax: &str, focusable: bool) -> Option<Role> {
    Some(match ax {
        "button" | "PopUpButton" | "ToggleButton" => Role::Button,
        "link" => Role::Link,
        "textbox" | "searchbox" | "spinbutton" | "TextField" | "SearchBox" => Role::TextField,
        "checkbox" | "switch" | "menuitemcheckbox" => Role::Checkbox,
        "radio" | "menuitemradio" => Role::Radio,
        "menuitem" => Role::MenuItem,
        "tab" => Role::Tab,
        "combobox" | "listbox" | "ComboBoxGrouping" | "ComboBoxMenuButton" => Role::ComboBox,
        "slider" => Role::Slider,
        "option" | "treeitem" => Role::ListItem,
        "listitem" if focusable => Role::ListItem,
        "gridcell" | "cell" if focusable => Role::Cell,
        _ => return None,
    })
}

fn prop<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    node["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
        .map(|p| &p["value"]["value"])
}

/// The interactive page elements, as raw nodes in viewport CSS pixels, each
/// with its backend DOM node id, plus the page facts. `viewport` is the
/// visible area (`x`, `y` = 0). Grounding (`a11y::ground_indexed`) then does
/// reading order, de-duplication, the mark cap and the grid fallback, exactly
/// as on the desktop.
pub fn nodes(ax_tree: &Value, snapshot: &Value, viewport: Rect) -> (Vec<(RawNode, i64)>, PageInfo) {
    let (layout, info) = layout(snapshot);
    let (sx, sy) = info.scroll;
    let mut out = Vec::new();
    for n in ax_tree["nodes"].as_array().into_iter().flatten() {
        if n["ignored"].as_bool() == Some(true) {
            continue;
        }
        let Some(id) = n["backendDOMNodeId"].as_i64() else {
            continue;
        };
        let focusable = prop(n, "focusable").and_then(Value::as_bool) == Some(true);
        let Some(mut r) = role(n["role"]["value"].as_str().unwrap_or(""), focusable) else {
            continue;
        };
        let Some(l) = layout.get(&id) else {
            continue; // no box: not rendered (closed <select> options, display:none)
        };
        let bounds = Rect::new(l.bounds.x - sx, l.bounds.y - sy, l.bounds.w, l.bounds.h);
        if l.password {
            r = Role::PasswordField;
        }
        let disabled = prop(n, "disabled").and_then(Value::as_bool) == Some(true);
        let value = match r {
            Role::TextField | Role::ComboBox | Role::Slider => n["value"]["value"]
                .as_str()
                .map(str::to_string)
                .or_else(|| n["value"]["value"].as_f64().map(|v| v.to_string())),
            _ => None,
        };
        let raw = RawNode {
            role: r,
            name: n["name"]["value"].as_str().unwrap_or("").trim().to_string(),
            bounds,
            enabled: !disabled,
            visible: l.visible && intersects(bounds, viewport),
            secure: l.password,
            value: if l.password { None } else { value },
            handle: None,
        };
        out.push((raw, id));
    }
    (out, info)
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.w > 0.0
        && a.h > 0.0
        && a.x < b.x + b.w
        && a.y < b.y + b.h
        && a.x + a.w > b.x
        && a.y + a.h > b.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Shapes copied from Chromium 141's real replies (trimmed).
    fn ax() -> Value {
        let p = |name: &str, v: Value| json!({"name": name, "value": {"type": "x", "value": v}});
        json!({"nodes": [
            {"nodeId":"1","ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"Contact us"},"backendDOMNodeId":3,"properties":[p("focusable", json!(true))]},
            {"nodeId":"2","ignored":false,"role":{"value":"textbox"},"name":{"value":"Name "},"value":{"value":"Ann"},"backendDOMNodeId":12,"properties":[p("focusable", json!(true))]},
            {"nodeId":"3","ignored":false,"role":{"value":"textbox"},"name":{"value":"Password "},"backendDOMNodeId":15,"properties":[]},
            {"nodeId":"4","ignored":false,"role":{"value":"button"},"name":{"value":"Send"},"backendDOMNodeId":32,"properties":[]},
            {"nodeId":"5","ignored":false,"role":{"value":"button"},"name":{"value":"Off"},"backendDOMNodeId":33,"properties":[p("disabled", json!(true))]},
            {"nodeId":"6","ignored":false,"role":{"value":"option"},"name":{"value":"Two"},"backendDOMNodeId":29,"properties":[]},
            {"nodeId":"7","ignored":false,"role":{"value":"button"},"name":{"value":"Far below"},"backendDOMNodeId":37,"properties":[]},
            {"nodeId":"8","ignored":true,"role":{"value":"button"},"name":{"value":"Ghost"},"backendDOMNodeId":40,"properties":[]},
            {"nodeId":"9","ignored":false,"role":{"value":"StaticText"},"name":{"value":"Contact"},"backendDOMNodeId":38,"properties":[]},
            {"nodeId":"10","ignored":false,"role":{"value":"button"},"name":{"value":"Hidden"},"backendDOMNodeId":41,"properties":[]}
        ]})
    }

    fn snap(scroll_y: f64) -> Value {
        json!({
            "strings": ["https://t.test/contact","Contact us","INPUT","type","password","BUTTON","visible","hidden","OPTION"],
            "documents": [{
                "documentURL": 0, "title": 1, "scrollOffsetX": 0, "scrollOffsetY": scroll_y,
                "nodes": {
                    "backendNodeId": [12, 15, 32, 33, 37, 41, 3, 29],
                    "nodeName": [2, 2, 5, 5, 5, 5, -1, 8],
                    "attributes": [[], [3, 4], [], [], [], [], [], []]
                },
                "layout": {
                    "nodeIndex": [0, 1, 2, 3, 4, 5, 6],
                    "bounds": [[42,80,185,21],[296,80,185,21],[628,80,47,21],[679,80,33,21],[0,2140,74,21],[0,120,50,20],[0,0,1280,2162]],
                    "styles": [[6],[6],[6],[6],[6],[7],[6]]
                }
            }]
        })
    }

    const VIEW: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1280.0,
        h: 800.0,
    };

    fn by_name<'a>(v: &'a [(RawNode, i64)], name: &str) -> &'a (RawNode, i64) {
        v.iter().find(|(n, _)| n.name == name).unwrap()
    }

    #[test]
    fn joins_tree_and_layout_on_backend_id() {
        let (v, info) = nodes(&ax(), &snap(0.0), VIEW);
        assert_eq!(info.url, "https://t.test/contact");
        assert_eq!(info.title, "Contact us");
        let (name, id) = by_name(&v, "Name");
        assert_eq!(*id, 12);
        assert_eq!(name.role, Role::TextField);
        assert_eq!(name.value.as_deref(), Some("Ann"));
        assert_eq!(name.bounds, Rect::new(42.0, 80.0, 185.0, 21.0));
        assert!(!by_name(&v, "Off").0.enabled);
        // No layout box (a closed <select>'s option), ignored, or plain text:
        // not an element.
        for gone in ["Two", "Ghost", "Contact", "Contact us"] {
            assert!(
                v.iter().all(|(n, _)| n.name != gone),
                "{gone} should be dropped"
            );
        }
    }

    #[test]
    fn password_inputs_are_secure_and_never_show_a_value() {
        let (v, _) = nodes(&ax(), &snap(0.0), VIEW);
        let (pw, _) = by_name(&v, "Password");
        assert_eq!(pw.role, Role::PasswordField);
        assert!(pw.secure);
        assert_eq!(pw.value, None);
    }

    #[test]
    fn hidden_and_offscreen_elements_are_not_visible() {
        let (v, _) = nodes(&ax(), &snap(0.0), VIEW);
        assert!(!by_name(&v, "Hidden").0.visible, "visibility:hidden");
        assert!(!by_name(&v, "Far below").0.visible, "below the fold");
        assert!(by_name(&v, "Send").0.visible);
    }

    #[test]
    fn boxes_move_with_the_scroll() {
        let (v, info) = nodes(&ax(), &snap(2000.0), VIEW);
        assert_eq!(info.scroll, (0.0, 2000.0));
        let (far, _) = by_name(&v, "Far below");
        assert_eq!(far.bounds.y, 140.0);
        assert!(far.visible);
        assert!(!by_name(&v, "Send").0.visible, "scrolled off the top");
    }

    #[test]
    fn garbage_yields_nothing_rather_than_panicking() {
        let (v, info) = nodes(&json!({}), &json!({"documents": "x"}), VIEW);
        assert!(v.is_empty());
        assert_eq!(info, PageInfo::default());
    }
}
