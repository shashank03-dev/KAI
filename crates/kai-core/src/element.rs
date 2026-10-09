//! Set-of-Marks grounding: the interactive elements Kai found on screen, each
//! given a stable number, drawn as a box on the screenshot and listed as text
//! for the model (`#14 button "Save" (enabled)`).

use serde::{Deserialize, Serialize};

/// A rectangle in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Centre point — where the actuator clicks.
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Role of a UI element, from the accessibility tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Button,
    Link,
    TextField,
    PasswordField,
    Checkbox,
    Radio,
    MenuItem,
    Tab,
    ComboBox,
    Slider,
    ListItem,
    Image,
    Cell,
    Other,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Role::Button => "button",
            Role::Link => "link",
            Role::TextField => "textfield",
            Role::PasswordField => "password",
            Role::Checkbox => "checkbox",
            Role::Radio => "radio",
            Role::MenuItem => "menuitem",
            Role::Tab => "tab",
            Role::ComboBox => "combobox",
            Role::Slider => "slider",
            Role::ListItem => "listitem",
            Role::Image => "image",
            Role::Cell => "cell",
            Role::Other => "element",
        }
    }
}

/// One numbered, interactive element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    /// The mark number shown to the model (1-based).
    pub mark: u32,
    pub role: Role,
    pub name: String,
    pub bounds: Rect,
    pub enabled: bool,
    /// True when this is (or sits inside) a password field — forces high risk.
    #[serde(default)]
    pub secure: bool,
}

/// What grounding produced: a set of marks, or a coarse grid fallback when the
/// accessibility tree was empty or useless (games, canvases, some Electron).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Grounding {
    Marks(Vec<Element>),
    /// A `cols`×`rows` grid; the model answers "click cell D7".
    Grid {
        cols: u32,
        rows: u32,
        screen: Rect,
    },
}

impl Grounding {
    /// The compact text list sent alongside the screenshot.
    pub fn as_text(&self) -> String {
        match self {
            Grounding::Marks(els) => {
                if els.is_empty() {
                    return "(no interactive elements found)".to_string();
                }
                els.iter()
                    .map(|e| {
                        let state = if !e.enabled {
                            "disabled"
                        } else if e.secure {
                            "secure"
                        } else {
                            "enabled"
                        };
                        format!("#{} {} \"{}\" ({})", e.mark, e.role.label(), e.name, state)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Grounding::Grid { cols, rows, .. } => format!(
                "No accessibility tree. A {cols}x{rows} grid overlays the screen; \
                 refer to a cell like A1 (top-left) .. {}{}.",
                col_letter(cols - 1),
                rows
            ),
        }
    }

    /// Look up the element for a target mark, if this is a marks grounding.
    pub fn element(&self, mark: u32) -> Option<&Element> {
        match self {
            Grounding::Marks(els) => els.iter().find(|e| e.mark == mark),
            Grounding::Grid { .. } => None,
        }
    }

    /// Map a grid cell index (row-major, 0-based) to its rectangle.
    pub fn grid_cell(&self, index: u32) -> Option<Rect> {
        match self {
            Grounding::Grid { cols, rows, screen } if index < cols * rows => {
                let c = index % cols;
                let r = index / cols;
                let cw = screen.w / *cols as f32;
                let ch = screen.h / *rows as f32;
                Some(Rect::new(
                    screen.x + c as f32 * cw,
                    screen.y + r as f32 * ch,
                    cw,
                    ch,
                ))
            }
            _ => None,
        }
    }
}

fn col_letter(mut n: u32) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(mark: u32, role: Role, name: &str, enabled: bool, secure: bool) -> Element {
        Element {
            mark,
            role,
            name: name.into(),
            bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
            enabled,
            secure,
        }
    }

    #[test]
    fn marks_render_as_text_with_state() {
        let g = Grounding::Marks(vec![
            el(1, Role::Button, "Save", true, false),
            el(2, Role::Button, "Delete", false, false),
            el(3, Role::PasswordField, "Password", true, true),
        ]);
        let t = g.as_text();
        assert!(t.contains("#1 button \"Save\" (enabled)"));
        assert!(t.contains("#2 button \"Delete\" (disabled)"));
        assert!(t.contains("#3 password \"Password\" (secure)"));
    }

    #[test]
    fn center_is_the_middle() {
        assert_eq!(Rect::new(0.0, 0.0, 10.0, 20.0).center(), (5.0, 10.0));
    }

    #[test]
    fn grid_cells_map_row_major() {
        let g = Grounding::Grid {
            cols: 2,
            rows: 2,
            screen: Rect::new(0.0, 0.0, 100.0, 100.0),
        };
        assert_eq!(g.grid_cell(0), Some(Rect::new(0.0, 0.0, 50.0, 50.0)));
        assert_eq!(g.grid_cell(3), Some(Rect::new(50.0, 50.0, 50.0, 50.0)));
        assert_eq!(g.grid_cell(4), None);
    }

    #[test]
    fn element_lookup_by_mark() {
        let g = Grounding::Marks(vec![el(7, Role::Link, "Home", true, false)]);
        assert_eq!(g.element(7).unwrap().name, "Home");
        assert!(g.element(8).is_none());
    }

    #[test]
    fn empty_marks_say_so() {
        assert_eq!(
            Grounding::Marks(vec![]).as_text(),
            "(no interactive elements found)"
        );
    }
}
