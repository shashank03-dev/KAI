//! The per-observation map from mark number to the live accessible object,
//! and the OS-free part of accessibility-first actions: which decisions may
//! skip the mouse at all.
//!
//! The perceptor and the actuator are separate objects, so the map lives
//! here: each grounded observation replaces it. A lookup only succeeds when
//! the caller acts on the very grounding that was recorded (the layout of
//! every mark matches) and the element (mark number, role *and* bounds) is
//! the one recorded, so a stale grounding, or one from another screen lane
//! in this process, never reaches a live object: it falls back to the mouse.

use super::{NodeRef, RawNode};
use kai_core::{Action, Decision, Element, Grounding, Rect, Role};
use std::sync::Mutex;

struct Entry {
    el: Element,
    node: NodeRef,
}

/// The latest grounded observation: its layout key and its live handles.
struct Live {
    key: u64,
    entries: Vec<Entry>,
}

static LIVE: Mutex<Live> = Mutex::new(Live {
    key: 0,
    entries: Vec::new(),
});

/// A fingerprint of a grounding's layout: each mark's number, role and box.
/// Names and values are left out on purpose: the engine may blank sensitive
/// text in the grounding it acts on, and that is still the same screen.
fn layout_key(g: &Grounding) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: u64| {
        for b in v.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    if let Grounding::Marks(els) = g {
        for e in els {
            eat(e.mark as u64);
            eat(e.role as u64);
            for v in [e.bounds.x, e.bounds.y, e.bounds.w, e.bounds.h] {
                eat(v.to_bits() as u64);
            }
        }
    }
    h
}

/// What to do through the accessibility interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// The element's default activation: AT-SPI "click"/"press"/"toggle"
    /// actions; UIA Invoke, Toggle, ExpandCollapse or SelectionItem.
    Press,
    /// Put this text into an EMPTY editable field.
    ///
    /// Kai's `type` means "click the field, then type": the mouse path puts
    /// the caret where the click lands and inserts there. For an empty field
    /// that is the same as setting its contents, so only that case runs here.
    /// A field that already has text falls back to the mouse and keyboard,
    /// because where a click would leave the caret (end of a short entry,
    /// mid-line in a long one) cannot be reproduced reliably.
    Fill(String),
}

/// Replace the map with this observation's marks.
pub fn remember(grounding: &Grounding, nodes: &[RawNode], picked: &[usize]) {
    let mut entries = Vec::new();
    if let Grounding::Marks(els) = grounding {
        for (el, &i) in els.iter().zip(picked) {
            if let Some(node) = nodes.get(i).and_then(|n| n.handle.clone()) {
                entries.push(Entry {
                    el: el.clone(),
                    node,
                });
            }
        }
    }
    if let Ok(mut live) = LIVE.lock() {
        *live = Live {
            key: layout_key(grounding),
            entries,
        };
    }
}

/// The live object for exactly this element of `g`, when `g` is the latest
/// grounded observation.
pub fn lookup(el: &Element, g: &Grounding) -> Option<NodeRef> {
    let live = LIVE.lock().ok()?;
    if live.key != layout_key(g) {
        return None;
    }
    live.entries
        .iter()
        .find(|e| e.el.mark == el.mark && e.el.bounds == el.bounds && e.el.role == el.role)
        .map(|e| e.node.clone())
}

/// Pixels an element may have moved since the observation and still be the
/// one the model saw.
const MOVED_PX: f32 = 4.0;

/// Whether the element is still where the observation showed it (`now` is
/// its box read just before acting; `None` when it cannot be read). A
/// control that moved, or scrolled away, is no longer "what the model saw":
/// the action falls back to the mouse path, which the engine re-checks.
pub(super) fn same_place(seen: Rect, now: Option<Rect>) -> bool {
    let Some(now) = now else {
        return false;
    };
    let near = |a: f32, b: f32| (a - b).abs() <= MOVED_PX;
    near(seen.x, now.x)
        && near(seen.y, now.y)
        && near(seen.x + seen.w, now.x + now.w)
        && near(seen.y + seen.h, now.y + now.h)
}

/// The target element and plan for a decision, or `None` when the decision
/// must go through the mouse/keyboard.
pub fn resolve(d: &Decision, g: &Grounding) -> Option<(Element, Plan)> {
    let Grounding::Marks(els) = g else {
        return None;
    };
    let target = d.target?;
    let el = els.iter().find(|e| e.mark == target)?;
    plan(d, el).map(|p| (el.clone(), p))
}

/// The OS-free policy. Never for secure or disabled elements, never for
/// double/right clicks (their meaning is app-specific), never for list items
/// or cells on a plain click (activating them usually means "open").
pub fn plan(d: &Decision, el: &Element) -> Option<Plan> {
    if el.secure || !el.enabled || el.role == Role::PasswordField {
        return None;
    }
    match d.action {
        Action::Click => matches!(
            el.role,
            Role::Button
                | Role::Link
                | Role::Checkbox
                | Role::Radio
                | Role::MenuItem
                | Role::Tab
                | Role::ComboBox
        )
        .then_some(Plan::Press),
        Action::Type => {
            let text = d.text.as_deref()?;
            // Newlines and tabs are keys with side effects (submit, move
            // focus) in single-line fields; let the keyboard do them.
            let plain = !text.is_empty() && !text.contains(['\n', '\r', '\t']);
            (el.role == Role::TextField && plain).then(|| Plan::Fill(text.to_string()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::Rect;

    fn el(role: Role) -> Element {
        Element {
            mark: 3,
            role,
            name: "x".into(),
            bounds: Rect::new(10.0, 10.0, 50.0, 20.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }
    }

    fn dec(json: &str) -> Decision {
        Decision::parse_checked(json).unwrap()
    }

    const CLICK: &str = r#"{"thought":"","action":"click","target":3,"risk":"none"}"#;

    #[test]
    fn click_on_controls_presses() {
        for r in [Role::Button, Role::Checkbox, Role::MenuItem, Role::Link] {
            assert_eq!(plan(&dec(CLICK), &el(r)), Some(Plan::Press), "{r:?}");
        }
        assert_eq!(plan(&dec(CLICK), &el(Role::ListItem)), None);
        assert_eq!(plan(&dec(CLICK), &el(Role::TextField)), None);
    }

    #[test]
    fn other_clicks_fall_back() {
        let d = dec(r#"{"thought":"","action":"double_click","target":3,"risk":"none"}"#);
        assert_eq!(plan(&d, &el(Role::Button)), None);
        let d = dec(r#"{"thought":"","action":"right_click","target":3,"risk":"none"}"#);
        assert_eq!(plan(&d, &el(Role::Button)), None);
    }

    #[test]
    fn secure_and_disabled_fall_back() {
        let mut e = el(Role::Button);
        e.secure = true;
        assert_eq!(plan(&dec(CLICK), &e), None);
        let mut e = el(Role::Button);
        e.enabled = false;
        assert_eq!(plan(&dec(CLICK), &e), None);
        let t = dec(r#"{"thought":"","action":"type","target":3,"text":"pw","risk":"high"}"#);
        assert_eq!(plan(&t, &el(Role::PasswordField)), None);
    }

    #[test]
    fn type_fills_plain_text_only() {
        let t = dec(r#"{"thought":"","action":"type","target":3,"text":"hello","risk":"none"}"#);
        assert_eq!(
            plan(&t, &el(Role::TextField)),
            Some(Plan::Fill("hello".into()))
        );
        let t = dec(r#"{"thought":"","action":"type","target":3,"text":"a\nb","risk":"none"}"#);
        assert_eq!(plan(&t, &el(Role::TextField)), None);
        let mut t = dec(r#"{"thought":"","action":"type","target":3,"text":"x","risk":"none"}"#);
        t.text = Some(String::new());
        assert_eq!(plan(&t, &el(Role::TextField)), None);
    }

    #[test]
    fn an_element_that_moved_is_not_acted_on_blind() {
        let seen = Rect::new(10.0, 10.0, 50.0, 20.0);
        assert!(same_place(seen, Some(seen)));
        assert!(same_place(seen, Some(Rect::new(12.0, 9.0, 49.0, 21.0))));
        assert!(!same_place(seen, Some(Rect::new(10.0, 300.0, 50.0, 20.0))));
        assert!(!same_place(seen, Some(Rect::new(10.0, 10.0, 200.0, 20.0))));
        assert!(!same_place(seen, None));
    }

    #[test]
    fn resolve_needs_marks_and_a_known_target() {
        let g = Grounding::Marks(vec![el(Role::Button)]);
        assert!(resolve(&dec(CLICK), &g).is_some());
        let miss = dec(r#"{"thought":"","action":"click","target":9,"risk":"none"}"#);
        assert!(resolve(&miss, &g).is_none());
        let grid = Grounding::Grid {
            cols: 4,
            rows: 4,
            screen: Rect::new(0.0, 0.0, 100.0, 100.0),
        };
        assert!(resolve(&dec(CLICK), &grid).is_none());
    }

    #[cfg(target_os = "linux")]
    fn dummy() -> NodeRef {
        atspi::ObjectRefOwned::from_static_str_unchecked(":1.42", "/org/a11y/atspi/accessible/7")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn dummy() -> NodeRef {}

    #[cfg(target_os = "macos")]
    fn dummy() -> NodeRef {
        // SAFETY: creating the system-wide element has no preconditions.
        super::super::macos::AxRef(unsafe {
            objc2_application_services::AXUIElement::new_system_wide()
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn raw(e: &Element) -> RawNode {
        RawNode {
            role: e.role,
            name: e.name.clone(),
            bounds: e.bounds,
            enabled: true,
            visible: true,
            secure: false,
            focused: false,
            value: None,
            handle: Some(dummy()),
        }
    }

    /// One test, because the map is process-global and tests run in
    /// parallel threads.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn lookup_needs_the_recorded_grounding_and_element() {
        let button = el(Role::Button);
        let first = Grounding::Marks(vec![button.clone()]);
        remember(&first, &[raw(&button)], &[0]);
        assert!(lookup(&button, &first).is_some());
        let mut moved = button.clone();
        moved.bounds.x += 5.0;
        assert!(lookup(&moved, &first).is_none());

        // Blanked text is still the same screen.
        let mut blanked = button.clone();
        blanked.name = "[sensitive data]".into();
        assert!(lookup(&button, &Grounding::Marks(vec![blanked])).is_some());

        // A newer observation (another lane, or the screen changed) holds
        // the same button plus more: the older grounding no longer reaches
        // a live object, even for an identical element.
        let mut other = el(Role::Link);
        other.mark = 4;
        other.bounds.y += 40.0;
        let newer = Grounding::Marks(vec![button.clone(), other.clone()]);
        remember(&newer, &[raw(&button), raw(&other)], &[0, 1]);
        assert!(lookup(&button, &first).is_none());
        assert!(lookup(&button, &newer).is_some());
    }
}
