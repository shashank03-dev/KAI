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
    select: bool,
}

/// The DOM node behind a mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomRef {
    /// `backendNodeId`: stable for the node's life, unlike `nodeId`.
    pub id: i64,
    /// A native `<select>`: options are chosen by label, not by clicking
    /// (its popup is drawn by the browser, outside the page).
    pub select: bool,
}

/// Where a document's (0,0) lands in the viewport, and the box it is clipped
/// to (its iframe's; `None` for the page itself).
#[derive(Debug, Clone, Copy)]
struct Place {
    dx: f32,
    dy: f32,
    clip: Option<Rect>,
}

/// Most frames read per observation (each one costs a protocol call).
const MAX_FRAMES: usize = 8;

/// The snapshot's string table lookup.
fn strings(snapshot: &Value) -> Vec<&str> {
    snapshot["strings"]
        .as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap_or("")).collect())
        .unwrap_or_default()
}

fn string<'a>(strings: &[&'a str], v: &Value) -> &'a str {
    v.as_i64()
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| strings.get(i).copied())
        .unwrap_or("")
}

fn num(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

/// The documents, their layout boxes and their places.
type Docs<'a> = (
    &'a [Value],
    Vec<HashMap<usize, (usize, Rect)>>,
    Vec<Option<Place>>,
);

fn docs(snapshot: &Value) -> Docs<'_> {
    let docs: &[Value] = snapshot["documents"].as_array().map_or(&[], Vec::as_slice);
    let boxes: Vec<_> = docs.iter().map(doc_boxes).collect();
    let places = places(docs, &boxes);
    (docs, boxes, places)
}

/// The layout of every document the snapshot holds (the page and its
/// same-process iframes; cross-site frames run in other processes and are
/// not in it), in viewport CSS pixels, per backend node id; and the page
/// facts.
fn layout(snapshot: &Value) -> (HashMap<i64, Layout>, PageInfo) {
    let strings = strings(snapshot);
    let s = |v: &Value| string(&strings, v);
    let (docs, boxes, places) = docs(snapshot);
    let main = docs.first().unwrap_or(&Value::Null);
    let info = PageInfo {
        url: s(&main["documentURL"]).to_string(),
        title: s(&main["title"]).to_string(),
        scroll: (num(&main["scrollOffsetX"]), num(&main["scrollOffsetY"])),
    };
    let mut out = HashMap::new();
    for (d, doc) in docs.iter().enumerate() {
        let Some(place) = places[d] else {
            continue;
        };
        let nodes = &doc["nodes"];
        let (Some(backend), names, attrs, styles) = (
            nodes["backendNodeId"].as_array(),
            nodes["nodeName"].as_array(),
            nodes["attributes"].as_array(),
            doc["layout"]["styles"].as_array(),
        ) else {
            continue;
        };
        for (&ni, &(i, b)) in &boxes[d] {
            let Some(id) = backend.get(ni).and_then(Value::as_i64) else {
                continue;
            };
            let rect = Rect::new(b.x + place.dx, b.y + place.dy, b.w, b.h);
            let visible = styles
                .and_then(|st| st.get(i))
                .and_then(|st| st.get(0))
                .map(|v| !matches!(s(v), "hidden" | "collapse"))
                .unwrap_or(true)
                && place.clip.is_none_or(|c| intersects(rect, c));
            let tag = names.and_then(|n| n.get(ni)).map(s).unwrap_or("");
            let is_input = tag.eq_ignore_ascii_case("input");
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
                    select: tag.eq_ignore_ascii_case("select"),
                },
            );
        }
    }
    (out, info)
}

/// One document's layout boxes in its own coordinates: node index →
/// (layout index, box). A ragged or malformed snapshot drops entries, it
/// never panics.
fn doc_boxes(doc: &Value) -> HashMap<usize, (usize, Rect)> {
    let lay = &doc["layout"];
    let (Some(index), Some(bounds)) = (lay["nodeIndex"].as_array(), lay["bounds"].as_array())
    else {
        return HashMap::new();
    };
    index
        .iter()
        .enumerate()
        .filter_map(|(i, ni)| {
            let ni = usize::try_from(ni.as_u64()?).ok()?;
            let b = bounds.get(i)?.as_array().filter(|b| b.len() >= 4)?;
            Some((
                ni,
                (i, Rect::new(num(&b[0]), num(&b[1]), num(&b[2]), num(&b[3]))),
            ))
        })
        .collect()
}

/// Each document's place: the page moves by its scroll; an iframe's
/// document starts at its iframe element's box (as placed in its parent,
/// within the parent's clip) less its own scroll. Unplaced: no iframe box
/// (not rendered), or not reachable from the page.
fn places(docs: &[Value], boxes: &[HashMap<usize, (usize, Rect)>]) -> Vec<Option<Place>> {
    let mut places: Vec<Option<Place>> = vec![None; docs.len()];
    let Some(main) = docs.first() else {
        return places;
    };
    places[0] = Some(Place {
        dx: -num(&main["scrollOffsetX"]),
        dy: -num(&main["scrollOffsetY"]),
        clip: None,
    });
    // An iframe element → the document it holds (`contentDocumentIndex`).
    let hosts: Vec<(usize, usize, usize)> = docs
        .iter()
        .enumerate()
        .flat_map(|(p, d)| {
            let rare = &d["nodes"]["contentDocumentIndex"];
            let nodes = rare["index"].as_array().map_or(&[][..], Vec::as_slice);
            let child = rare["value"].as_array().map_or(&[][..], Vec::as_slice);
            nodes
                .iter()
                .zip(child)
                .filter_map(move |(n, c)| Some((p, n.as_u64()? as usize, c.as_u64()? as usize)))
        })
        .collect();
    // Parents before children, however the documents are ordered.
    for _ in 0..docs.len() {
        for &(p, node, c) in &hosts {
            let (Some(parent), Some(&(_, frame))) = (places[p], boxes[p].get(&node)) else {
                continue;
            };
            if c == 0 || c >= docs.len() || places[c].is_some() {
                continue;
            }
            let at = Rect::new(frame.x + parent.dx, frame.y + parent.dy, frame.w, frame.h);
            places[c] = Some(Place {
                dx: at.x - num(&docs[c]["scrollOffsetX"]),
                dy: at.y - num(&docs[c]["scrollOffsetY"]),
                clip: Some(parent.clip.map_or(at, |clip| overlap(at, clip))),
            });
        }
    }
    places
}

/// The part of `a` inside `b` (empty when they do not meet).
fn overlap(a: Rect, b: Rect) -> Rect {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// The ids of the page's same-process iframes the snapshot placed, so their
/// accessibility trees can be read too (`Accessibility.getFullAXTree` reads
/// one frame at a time).
pub fn frames(snapshot: &Value) -> Vec<String> {
    let strings = strings(snapshot);
    let (docs, _, places) = docs(snapshot);
    docs.iter()
        .zip(places)
        .skip(1)
        .filter(|(_, p)| p.is_some_and(|p| p.clip.is_some_and(|c| c.w > 0.0 && c.h > 0.0)))
        .map(|(d, _)| string(&strings, &d["frameId"]).to_string())
        .filter(|id| !id.is_empty())
        .take(MAX_FRAMES)
        .collect()
}

/// Several frames' accessibility trees as one (a node already seen, by its
/// backend id, is not repeated).
pub fn merged(trees: Vec<Value>) -> Value {
    let mut seen = std::collections::HashSet::new();
    let nodes: Vec<Value> = trees
        .into_iter()
        .flat_map(|mut t| match t["nodes"].take() {
            Value::Array(a) => a,
            _ => Vec::new(),
        })
        .filter(|n| {
            n["backendDOMNodeId"]
                .as_i64()
                .is_none_or(|id| seen.insert(id))
        })
        .collect();
    serde_json::json!({ "nodes": nodes })
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

/// The interactive page elements (`ax_tree`: the page's and its frames',
/// see [`merged`]), as raw nodes in viewport CSS pixels, each
/// with its backend DOM node id, plus the page facts. `viewport` is the
/// visible area (`x`, `y` = 0). Grounding (`a11y::ground_indexed`) then does
/// reading order, de-duplication, the mark cap and the grid fallback, exactly
/// as on the desktop.
pub fn nodes(
    ax_tree: &Value,
    snapshot: &Value,
    viewport: Rect,
) -> (Vec<(RawNode, DomRef)>, PageInfo) {
    let (layout, info) = layout(snapshot);
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
        let bounds = l.bounds;
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
            focused: prop(n, "focused").and_then(Value::as_bool) == Some(true),
            value: if l.password { None } else { value },
            handle: None,
        };
        out.push((
            raw,
            DomRef {
                id,
                select: l.select,
            },
        ));
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
mod tests;
