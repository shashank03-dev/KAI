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
        {"nodeId":"10","ignored":false,"role":{"value":"button"},"name":{"value":"Hidden"},"backendDOMNodeId":41,"properties":[]},
        {"nodeId":"11","ignored":false,"role":{"value":"combobox"},"name":{"value":"Size"},"value":{"value":"Two"},"backendDOMNodeId":19,"properties":[]}
    ]})
}

fn snap(scroll_y: f64) -> Value {
    json!({
        "strings": ["https://t.test/contact","Contact us","INPUT","type","password","BUTTON","visible","hidden","OPTION","SELECT"],
        "documents": [{
            "documentURL": 0, "title": 1, "scrollOffsetX": 0, "scrollOffsetY": scroll_y,
            "nodes": {
                "backendNodeId": [12, 15, 32, 33, 37, 41, 3, 29, 19],
                "nodeName": [2, 2, 5, 5, 5, 5, -1, 8, 9],
                "attributes": [[], [3, 4], [], [], [], [], [], [], []]
            },
            "layout": {
                "nodeIndex": [0, 1, 2, 3, 4, 5, 6, 8],
                "bounds": [[42,80,185,21],[296,80,185,21],[628,80,47,21],[679,80,33,21],[0,2140,74,21],[0,120,50,20],[0,0,1280,2162],[576,81,48,19]],
                "styles": [[6],[6],[6],[6],[6],[7],[6],[6]]
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

fn by_name<'a>(v: &'a [(RawNode, DomRef)], name: &str) -> &'a (RawNode, DomRef) {
    v.iter().find(|(n, _)| n.name == name).unwrap()
}

#[test]
fn joins_tree_and_layout_on_backend_id() {
    let (v, info) = nodes(&ax(), &snap(0.0), VIEW);
    assert_eq!(info.url, "https://t.test/contact");
    assert_eq!(info.title, "Contact us");
    let (name, r) = by_name(&v, "Name");
    assert_eq!(
        *r,
        DomRef {
            id: 12,
            select: false
        }
    );
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
fn native_selects_are_flagged_and_show_their_value() {
    let (v, _) = nodes(&ax(), &snap(0.0), VIEW);
    let (size, r) = by_name(&v, "Size");
    assert_eq!(size.role, Role::ComboBox);
    assert_eq!(size.value.as_deref(), Some("Two"));
    assert_eq!(
        *r,
        DomRef {
            id: 19,
            select: true
        }
    );
    assert!(!by_name(&v, "Send").1.select);
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
    // Ragged arrays: more layout entries than boxes, a short box.
    let mut ragged = snap(0.0);
    ragged["documents"][0]["layout"]["bounds"] = json!([[42, 80, 185, 21], [296, 80]]);
    let (v, _) = nodes(&ax(), &ragged, VIEW);
    assert_eq!(v.len(), 1, "only the whole box survives");
    assert_eq!(v[0].0.name, "Name");
}

/// A page scrolled by 50 holding a same-process iframe at (100, 300)
/// whose own document is scrolled by 10 and holds one button.
fn framed() -> (Value, Value) {
    let tree = merged(vec![
        json!({"nodes": [{"role":{"value":"button"},"name":{"value":"Top"},"backendDOMNodeId":5}]}),
        json!({"nodes": [
            {"role":{"value":"button"},"name":{"value":"Inside"},"backendDOMNodeId":9},
            {"role":{"value":"button"},"name":{"value":"Top"},"backendDOMNodeId":5}
        ]}),
    ]);
    let snap = json!({
        "strings": ["https://t.test/","Framed","BUTTON","IFRAME","F1"],
        "documents": [
            {"documentURL": 0, "title": 1, "scrollOffsetX": 0, "scrollOffsetY": 50,
             "nodes": {"backendNodeId": [5, 6], "nodeName": [2, 3],
                       "contentDocumentIndex": {"index": [1], "value": [1]}},
             "layout": {"nodeIndex": [0, 1], "bounds": [[10, 60, 40, 20], [100, 300, 300, 150]]}},
            {"documentURL": 0, "frameId": 4, "scrollOffsetX": 0, "scrollOffsetY": 10,
             "nodes": {"backendNodeId": [9], "nodeName": [2]},
             "layout": {"nodeIndex": [0], "bounds": [[20, 30, 60, 20]]}}
        ]
    });
    (tree, snap)
}

#[test]
fn iframe_elements_are_placed_inside_their_frame() {
    let (tree, snap) = framed();
    assert_eq!(tree["nodes"].as_array().unwrap().len(), 2, "merged once");
    assert_eq!(frames(&snap), ["F1"]);
    let (v, _) = nodes(&tree, &snap, VIEW);
    assert_eq!(
        by_name(&v, "Top").0.bounds,
        Rect::new(10.0, 10.0, 40.0, 20.0)
    );
    // 100 + 20, 300 - 50 (page scroll) + 30 - 10 (frame scroll).
    let (inside, r) = by_name(&v, "Inside");
    assert_eq!(inside.bounds, Rect::new(120.0, 270.0, 60.0, 20.0));
    assert!(inside.visible);
    assert_eq!(r.id, 9);
}

#[test]
fn iframe_content_scrolled_out_of_its_frame_is_not_visible() {
    let (tree, mut snap) = framed();
    snap["documents"][1]["scrollOffsetY"] = json!(500);
    let (v, _) = nodes(&tree, &snap, VIEW);
    assert!(!by_name(&v, "Inside").0.visible);
}
