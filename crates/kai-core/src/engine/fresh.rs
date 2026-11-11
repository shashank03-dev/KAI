//! Is an observation still good to act on? Approvals and queued input can
//! outlive the screen, so the action's target is checked again just before
//! acting. Only what the action depends on is compared: a clock, a ticker, a
//! progress label or a field's typed value elsewhere on the page must not
//! refuse every step (`docs/reviews/UI_UX_AUDIT_2026_10_16.md`, review of the
//! computer-use foundation).

use crate::computer::{ComputerAction, Target};
use crate::{Element, Grounding};

/// Same element: what it is, where it is and whether it takes input. Its
/// value and focus may change (typing lands, a caret blinks).
fn same_element(a: &Element, b: &Element) -> bool {
    a.role == b.role
        && a.name == b.name
        && a.bounds == b.bounds
        && a.enabled == b.enabled
        && a.secure == b.secure
}

fn same_mark(expected: &Grounding, fresh: &Grounding, mark: u32) -> bool {
    match (expected.element(mark), fresh.element(mark)) {
        (Some(a), Some(b)) => same_element(a, b),
        _ => false,
    }
}

/// The smallest element under a point, the one a click there reaches.
fn under(g: &Grounding, x: f32, y: f32) -> Option<&Element> {
    let Grounding::Marks(els) = g else {
        return None;
    };
    els.iter()
        .filter(|e| {
            let r = e.bounds;
            x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h
        })
        .min_by(|a, b| (a.bounds.w * a.bounds.h).total_cmp(&(b.bounds.w * b.bounds.h)))
}

/// What `mark` (or, with none, the keyboard focus) acts on is unchanged.
/// A grid has no elements to compare: its layout must match, and the
/// caller compares pixels.
pub(crate) fn same_target(expected: &Grounding, fresh: &Grounding, mark: Option<u32>) -> bool {
    match (expected, fresh) {
        (Grounding::Grid { .. }, _) | (_, Grounding::Grid { .. }) => expected == fresh,
        _ => match mark {
            Some(m) => same_mark(expected, fresh, m),
            None => match expected.focused_field() {
                Some(m) => fresh.focused_field() == Some(m) && same_mark(expected, fresh, m),
                None => true,
            },
        },
    }
}

/// The same elements in the same places; values and focus may differ. A
/// provider's queued actions go on after a click focused a field or typing
/// landed, and stop when a dialog opened or the page moved.
pub(crate) fn same_layout(expected: &Grounding, fresh: &Grounding) -> bool {
    match (expected, fresh) {
        (Grounding::Marks(a), Grounding::Marks(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_element(a, b))
        }
        _ => expected == fresh,
    }
}

/// [`same_target`] for a computer tool action.
pub(crate) fn same_tool_target(
    expected: &Grounding,
    fresh: &Grounding,
    action: &ComputerAction,
) -> bool {
    let point = |t: &Target| match *t {
        Target::Element { mark, .. } => same_target(expected, fresh, Some(mark)),
        Target::Point { x, y, .. } => match (
            under(expected, x as f32, y as f32),
            under(fresh, x as f32, y as f32),
        ) {
            (Some(a), Some(b)) => same_element(a, b),
            (None, None) => same_target(expected, fresh, None),
            _ => false,
        },
    };
    match action {
        ComputerAction::Click { target, .. }
        | ComputerAction::Move { target, .. }
        | ComputerAction::Type { target, .. }
        | ComputerAction::Scroll { target, .. } => point(target),
        ComputerAction::Drag { points, .. } => points.iter().all(point),
        ComputerAction::Keypress { .. } => same_target(expected, fresh, None),
        ComputerAction::Observe
        | ComputerAction::Wait { .. }
        | ComputerAction::OpenApp { .. }
        | ComputerAction::OpenUrl { .. } => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rect, Role};

    fn el(mark: u32, name: &str, x: f32) -> Element {
        Element {
            mark,
            role: Role::Button,
            name: name.into(),
            bounds: Rect::new(x, 0.0, 50.0, 20.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }
    }

    fn page(clock: &str) -> Grounding {
        let mut field = el(2, "Email", 100.0);
        field.role = Role::TextField;
        field.focused = true;
        Grounding::Marks(vec![el(1, "Send", 0.0), field, el(3, clock, 200.0)])
    }

    #[test]
    fn a_ticking_clock_elsewhere_does_not_refuse_the_step() {
        let (before, after) = (page("10:01"), page("10:02"));
        assert!(same_target(&before, &after, Some(1)));
        assert!(same_target(&before, &after, None), "focus is unchanged");
        assert!(
            !same_target(&before, &after, Some(3)),
            "the clock itself changed"
        );
    }

    #[test]
    fn a_typed_value_or_caret_does_not_refuse_the_step() {
        let before = page("x");
        let mut after = page("x");
        if let Grounding::Marks(els) = &mut after {
            els[1].value = Some("a@b.c".into());
        }
        assert!(same_target(&before, &after, Some(2)));
    }

    #[test]
    fn a_moved_or_renamed_target_refuses() {
        let before = page("x");
        let mut after = page("x");
        if let Grounding::Marks(els) = &mut after {
            els[0].name = "Delete".into();
        }
        assert!(!same_target(&before, &after, Some(1)));
        let shifted = Grounding::Marks(vec![el(1, "Send", 30.0)]);
        assert!(!same_target(&before, &shifted, Some(1)));
        assert!(!same_target(&before, &Grounding::Marks(vec![]), Some(1)));
    }

    #[test]
    fn focus_that_moved_refuses_untargeted_typing() {
        let before = page("x");
        let mut after = page("x");
        if let Grounding::Marks(els) = &mut after {
            els[1].focused = false;
            els[0].focused = true;
        }
        assert!(!same_target(&before, &after, None));
    }

    #[test]
    fn a_layout_survives_typing_but_not_a_new_dialog() {
        let before = page("x");
        let mut typed = page("x");
        if let Grounding::Marks(els) = &mut typed {
            els[1].value = Some("hello".into());
            els[1].focused = false;
        }
        assert!(same_layout(&before, &typed));
        let mut dialog = page("x");
        if let Grounding::Marks(els) = &mut dialog {
            els.push(el(4, "Discard changes?", 300.0));
        }
        assert!(!same_layout(&before, &dialog));
    }

    #[test]
    fn a_point_click_compares_the_element_under_it() {
        let before = page("10:01");
        let after = page("10:02");
        let at = |x| ComputerAction::Click {
            modifiers: vec![],
            target: Target::Point {
                observation: 1,
                x,
                y: 5.0,
            },
            button: crate::computer::MouseButton::Left,
            count: 1,
        };
        assert!(same_tool_target(&before, &after, &at(10.0)));
        assert!(!same_tool_target(&before, &after, &at(210.0)));
    }
}
