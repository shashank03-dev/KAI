use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollUnit {
    Pixels,
    WheelNotches,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Point { observation: u64, x: f64, y: f64 },
    Element { observation: u64, mark: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComputerAction {
    Observe,
    Click {
        target: Target,
        button: MouseButton,
        count: u8,
        /// Keys held for the action (`shift` for Shift+click), released after.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<String>,
    },
    Move {
        target: Target,
        /// Keys held for the action (`shift` for Shift+click), released after.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<String>,
    },
    Drag {
        points: Vec<Target>,
        button: MouseButton,
        duration_ms: u32,
        /// Keys held for the action (`shift` for Shift+click), released after.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<String>,
    },
    Type {
        target: Target,
        text: String,
    },
    Keypress {
        keys: Vec<String>,
    },
    Scroll {
        target: Target,
        dx: i32,
        dy: i32,
        unit: ScrollUnit,
        /// Keys held for the action (`shift` for Shift+click), released after.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<String>,
    },
    Wait {
        duration_ms: u32,
    },
    OpenApp {
        name: String,
    },
    OpenUrl {
        url: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Observe,
    Click,
    Move,
    Drag,
    Type,
    Keypress,
    Scroll,
    Wait,
    OpenApp,
    OpenUrl,
}

/// OpenAI's documented pixel-to-wheel conversion: one notch per 100 px,
/// at least one, rounded half away from zero, clamped to the 20-notch bound.
pub fn pixels_to_notches(px: i32) -> i32 {
    if px == 0 {
        return 0;
    }
    let notches = ((px.unsigned_abs() as f64 / 100.).round() as i32).clamp(1, 20);
    notches * px.signum()
}

impl ComputerAction {
    /// Modifier keys held during a mouse action; none for the rest.
    pub fn modifiers(&self) -> &[String] {
        match self {
            Self::Click { modifiers, .. }
            | Self::Move { modifiers, .. }
            | Self::Drag { modifiers, .. }
            | Self::Scroll { modifiers, .. } => modifiers,
            _ => &[],
        }
    }

    pub fn targets(&self) -> Vec<&Target> {
        match self {
            Self::Click { target, .. }
            | Self::Move { target, .. }
            | Self::Type { target, .. }
            | Self::Scroll { target, .. } => vec![target],
            Self::Drag { points, .. } => points.iter().collect(),
            _ => vec![],
        }
    }

    /// The targets, for updating them in place (all but drag points).
    pub fn targets_mut(&mut self) -> Vec<&mut Target> {
        match self {
            Self::Click { target, .. }
            | Self::Move { target, .. }
            | Self::Type { target, .. }
            | Self::Scroll { target, .. } => vec![target],
            Self::Drag { points, .. } => points.iter_mut().collect(),
            _ => vec![],
        }
    }

    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Observe => ActionKind::Observe,
            Self::Click { .. } => ActionKind::Click,
            Self::Move { .. } => ActionKind::Move,
            Self::Drag { .. } => ActionKind::Drag,
            Self::Type { .. } => ActionKind::Type,
            Self::Keypress { .. } => ActionKind::Keypress,
            Self::Scroll { .. } => ActionKind::Scroll,
            Self::Wait { .. } => ActionKind::Wait,
            Self::OpenApp { .. } => ActionKind::OpenApp,
            Self::OpenUrl { .. } => ActionKind::OpenUrl,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRequest {
    pub version: u16,
    pub id: u64,
    pub session: u64,
    /// Inclusive execution budget; the dispatcher starts it before preflight.
    pub timeout_ms: u32,
    pub action: ComputerAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub actions: Vec<ActionKind>,
    pub buttons: Vec<MouseButton>,
    pub scroll_units: Vec<ScrollUnit>,
}

impl Capabilities {
    /// The complete contract vocabulary, for schema discovery. Real drivers
    /// must remove capabilities they have not implemented and verified.
    pub fn all() -> Self {
        Self {
            actions: vec![
                ActionKind::Observe,
                ActionKind::Click,
                ActionKind::Move,
                ActionKind::Drag,
                ActionKind::Type,
                ActionKind::Keypress,
                ActionKind::Scroll,
                ActionKind::Wait,
                ActionKind::OpenApp,
                ActionKind::OpenUrl,
            ],
            buttons: vec![
                MouseButton::Left,
                MouseButton::Middle,
                MouseButton::Right,
                MouseButton::Back,
                MouseButton::Forward,
            ],
            scroll_units: vec![ScrollUnit::Pixels, ScrollUnit::WheelNotches],
        }
    }
}
