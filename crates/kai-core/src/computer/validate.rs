use super::*;

fn require(ok: bool) -> Result<(), ToolError> {
    if ok {
        Ok(())
    } else {
        Err(ToolError::InvalidArguments)
    }
}

impl Target {
    pub fn validate(&self, space: &ObservationSpace) -> Result<(), ToolError> {
        let id = match self {
            Self::Point { observation, .. } | Self::Element { observation, .. } => *observation,
        };
        if id != space.id {
            return Err(ToolError::StaleObservation);
        }
        match self {
            Self::Point { x, y, .. } => space.point(*x, *y).map(drop),
            Self::Element { mark, .. } => require(*mark > 0),
        }
    }
}

/// Canonical key vocabulary shared with the native driver. Only modifiers
/// may precede the final key; arbitrary strings never reach OS injection,
/// and neither does a lone control or whitespace character (a newline, ESC
/// or NUL would be injected as a raw Unicode key).
pub fn valid_keys(keys: &[String]) -> bool {
    let modifier = |k: &str| matches!(k, "ctrl" | "alt" | "shift" | "cmd");
    let named = |k: &str| {
        matches!(
            k,
            "enter"
                | "esc"
                | "tab"
                | "space"
                | "backspace"
                | "delete"
                | "del"
                | "up"
                | "down"
                | "left"
                | "right"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
                | "f1"
                | "f2"
                | "f3"
                | "f4"
                | "f5"
                | "f6"
                | "f7"
                | "f8"
                | "f9"
                | "f10"
                | "f11"
                | "f12"
        )
    };
    let printable = |k: &str| {
        let mut chars = k.chars();
        matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_control() && !c.is_whitespace())
    };
    if keys.is_empty() || keys.len() > 8 {
        return false;
    }
    let (last, mods) = keys.split_last().unwrap();
    mods.iter()
        .enumerate()
        .all(|(i, k)| modifier(k) && !mods[..i].contains(k))
        && (named(last) || printable(last))
}

impl ToolRequest {
    pub fn validate(&self, caps: &Capabilities, space: &ObservationSpace) -> Result<(), ToolError> {
        require(self.version == 1 && self.timeout_ms > 0 && self.timeout_ms <= 30_000)?;
        if self.session != space.session {
            return Err(ToolError::StaleObservation);
        }
        if !caps.actions.contains(&self.action.kind()) {
            return Err(ToolError::Unsupported);
        }
        let button = |b: &MouseButton| {
            if caps.buttons.contains(b) {
                Ok(())
            } else {
                Err(ToolError::Unsupported)
            }
        };
        match &self.action {
            ComputerAction::Observe => Ok(()),
            ComputerAction::Click {
                target,
                button: b,
                count,
            } => {
                require(matches!(count, 1 | 2))?;
                button(b)?;
                target.validate(space)
            }
            ComputerAction::Move { target } => target.validate(space),
            ComputerAction::Drag {
                points,
                button: b,
                duration_ms,
            } => {
                require((2..=256).contains(&points.len()) && *duration_ms <= 5000)?;
                button(b)?;
                points.iter().try_for_each(|p| p.validate(space))
            }
            ComputerAction::Type { target, text } => {
                require(!text.is_empty() && text.len() <= 8192)?;
                target.validate(space)
            }
            ComputerAction::Keypress { keys } => require(valid_keys(keys)),
            ComputerAction::Scroll {
                target,
                dx,
                dy,
                unit,
            } => {
                if !caps.scroll_units.contains(unit) {
                    return Err(ToolError::Unsupported);
                }
                let limit = match unit {
                    ScrollUnit::Pixels => 4096,
                    ScrollUnit::WheelNotches => 20,
                };
                require(dx.unsigned_abs() <= limit && dy.unsigned_abs() <= limit)?;
                target.validate(space)
            }
            ComputerAction::Wait { duration_ms } => require(*duration_ms <= 3000),
            ComputerAction::OpenApp { name } => {
                require(!name.trim().is_empty() && name.len() <= 8192)
            }
            ComputerAction::OpenUrl { url } => {
                require(url.len() <= 8192)?;
                crate::web::normalise_url(url)
                    .map(drop)
                    .map_err(|_| ToolError::InvalidArguments)
            }
        }
    }
}

pub fn validate_batch(
    requests: &[ToolRequest],
    caps: &Capabilities,
    space: &ObservationSpace,
) -> Result<(), ToolError> {
    require(!requests.is_empty() && requests.len() <= 4)?;
    for (i, r) in requests.iter().enumerate() {
        require(!requests[..i].iter().any(|prior| prior.id == r.id))?;
        r.validate(caps, space)?;
    }
    Ok(())
}
