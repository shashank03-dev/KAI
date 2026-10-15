//! macOS: the pure parts of reading the accessibility tree (Kai's role for
//! an element's `AXRole` and `AXSubrole`, its name, the visible region its
//! children are clipped to), so they are tested on every OS.

use kai_core::{Rect, Role};

/// The role for `(AXRole, AXSubrole)`. Secure text fields are a subrole of
/// `AXTextField`; tabs are radio buttons with the `AXTabButton` subrole.
pub fn map_role(role: &str, subrole: &str) -> Role {
    if subrole == "AXSecureTextField" {
        return Role::PasswordField;
    }
    match role {
        "AXButton" | "AXMenuButton" | "AXDisclosureTriangle" | "AXDockItem" => Role::Button,
        "AXLink" => Role::Link,
        "AXTextField" | "AXTextArea" | "AXSearchField" => Role::TextField,
        "AXCheckBox" => Role::Checkbox,
        "AXRadioButton" if subrole == "AXTabButton" => Role::Tab,
        "AXRadioButton" => Role::Radio,
        "AXMenuItem" | "AXMenuBarItem" => Role::MenuItem,
        "AXPopUpButton" | "AXComboBox" => Role::ComboBox,
        "AXSlider" | "AXIncrementor" => Role::Slider,
        "AXRow" | "AXOutlineRow" => Role::ListItem,
        "AXCell" => Role::Cell,
        "AXImage" => Role::Image,
        _ => Role::Other,
    }
}

/// The name to show for an element: its title, else its description, else
/// (for a field) its placeholder.
pub fn pick_name(title: &str, description: &str, placeholder: &str) -> String {
    [title, description, placeholder]
        .into_iter()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// The region a node's children can be seen in: a scroll area narrows it to
/// its own frame; anything else passes its clip on.
pub fn child_clip(clip: Option<Rect>, role: &str, bounds: Option<Rect>) -> Option<Rect> {
    let Some(b) = bounds.filter(|_| role == "AXScrollArea") else {
        return clip;
    };
    let Some(c) = clip else {
        return Some(b);
    };
    let (x0, y0) = (b.x.max(c.x), b.y.max(c.y));
    let (x1, y1) = ((b.x + b.w).min(c.x + c.w), (b.y + b.h).min(c.y + c.h));
    Some(Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)))
}

/// Whether a node's centre is in its clip (no clip: visible).
pub fn in_clip(r: Rect, clip: Option<Rect>) -> bool {
    let (cx, cy) = r.center();
    clip.is_none_or(|c| cx >= c.x && cy >= c.y && cx < c.x + c.w && cy < c.y + c.h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_areas_clip_their_children_and_nothing_else_does() {
        let area = Rect::new(0.0, 100.0, 400.0, 300.0);
        let clip = child_clip(None, "AXScrollArea", Some(area));
        assert_eq!(clip, Some(area));
        assert_eq!(
            child_clip(clip, "AXGroup", Some(Rect::new(0.0, 0.0, 9e3, 9e3))),
            clip
        );
        // A row scrolled below the area is not visible; one inside is.
        assert!(!in_clip(Rect::new(10.0, 450.0, 100.0, 20.0), clip));
        assert!(in_clip(Rect::new(10.0, 150.0, 100.0, 20.0), clip));
        // Nested scroll areas intersect.
        let inner = child_clip(
            clip,
            "AXScrollArea",
            Some(Rect::new(200.0, 50.0, 400.0, 100.0)),
        );
        assert_eq!(inner, Some(Rect::new(200.0, 100.0, 200.0, 50.0)));
        assert!(in_clip(Rect::new(0.0, 0.0, 10.0, 10.0), None));
    }

    #[test]
    fn secure_fields_are_password_fields_whatever_the_role() {
        assert_eq!(
            map_role("AXTextField", "AXSecureTextField"),
            Role::PasswordField
        );
        assert_eq!(map_role("AXTextField", ""), Role::TextField);
    }

    #[test]
    fn tabs_menus_and_lists() {
        assert_eq!(map_role("AXRadioButton", "AXTabButton"), Role::Tab);
        assert_eq!(map_role("AXRadioButton", ""), Role::Radio);
        assert_eq!(map_role("AXMenuBarItem", ""), Role::MenuItem);
        assert_eq!(map_role("AXPopUpButton", ""), Role::ComboBox);
        assert_eq!(map_role("AXRow", "AXOutlineRow"), Role::ListItem);
        assert_eq!(map_role("AXStaticText", ""), Role::Other);
        assert_eq!(map_role("AXGroup", ""), Role::Other);
    }

    #[test]
    fn name_prefers_title_then_description_then_placeholder() {
        assert_eq!(pick_name("Save", "save the file", ""), "Save");
        assert_eq!(pick_name("  ", "Close", ""), "Close");
        assert_eq!(pick_name("", "", "Search"), "Search");
        assert_eq!(pick_name("", "", ""), "");
    }
}
