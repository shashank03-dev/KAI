//! The shared computer tools over CDP. Click, keys and typing reuse the
//! lane's own paths (release guards, page dialogs, the typed-text wait),
//! which the live browser tests cover. Drag and held modifiers are not
//! offered: CDP drag and modifier masks have no acceptance fixture yet.
use super::lane::BrowserLane;
use kai_core::{Grounding, computer::*};
use serde_json::json;
tokio::task_local! {pub(super) static INPUT_CONTEXT:ExecutionContext;}

pub(super) fn capabilities() -> Capabilities {
    Capabilities {
        actions: vec![
            ActionKind::Observe,
            ActionKind::Click,
            ActionKind::Move,
            ActionKind::Type,
            ActionKind::Keypress,
            ActionKind::Scroll,
            ActionKind::Wait,
        ],
        buttons: vec![
            MouseButton::Left,
            MouseButton::Middle,
            MouseButton::Right,
            MouseButton::Back,
            MouseButton::Forward,
        ],
        scroll_units: vec![ScrollUnit::Pixels],
    }
}
fn at(target: &Target, space: &ObservationSpace, g: &Grounding) -> Result<(f64, f64), ToolError> {
    target.validate(space)?;
    match target {
        Target::Point { x, y, .. } => space.point(*x, *y),
        Target::Element { mark, .. } => {
            let el = g
                .element(*mark)
                .filter(|e| e.enabled)
                .ok_or(ToolError::StaleObservation)?;
            let (x, y) = el.bounds.center();
            if !x.is_finite() || !y.is_finite() {
                return Err(ToolError::InvalidArguments);
            }
            Ok((x as f64, y as f64))
        }
    }
}
pub(super) async fn act(
    lane: &BrowserLane,
    a: &ComputerAction,
    s: &ObservationSpace,
    g: &Grounding,
    c: &ExecutionContext,
) -> Result<(), ToolError> {
    ToolRequest {
        version: 1,
        id: 0,
        session: s.session,
        timeout_ms: 30_000,
        action: a.clone(),
    }
    .validate(&capabilities(), s)?;
    c.check()?;
    if lane.dialog().is_some() {
        return Err(ToolError::NeedsUser);
    }
    if !a.modifiers().is_empty() {
        return Err(ToolError::Unsupported);
    }
    if let Some(done) = INPUT_CONTEXT.scope(c.clone(), input(lane, a, s, g)).await {
        return done.map_err(|_| c.check().err().unwrap_or(ToolError::Failed));
    }
    let params = match a {
        ComputerAction::Observe => return Ok(()),
        ComputerAction::Wait { duration_ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(*duration_ms as u64)).await;
            c.check()?;
            return Ok(());
        }
        ComputerAction::Move { target, .. } => {
            let (x, y) = at(target, s, g)?;
            json!({"type":"mouseMoved","x":x,"y":y})
        }
        ComputerAction::Scroll { target, dx, dy, .. } => {
            if *dx == 0 && *dy == 0 {
                return Ok(());
            }
            let (x, y) = at(target, s, g)?;
            // Deltas are screenshot pixels; CDP takes CSS pixels.
            let r = s.region;
            let css = |d: i32, side: f32, image: u32| {
                (d as f64 * side as f64 / image.max(1) as f64).round() as i32
            };
            wheel(
                x,
                y,
                css(*dx, r.w, s.image_width),
                css(*dy, r.h, s.image_height),
            )
        }
        _ => return Err(ToolError::Unsupported),
    };
    c.check()?;
    lane.page("Input.dispatchMouseEvent", params)
        .await
        .map(drop)
        .map_err(|_| ToolError::Failed)
}
/// Click, keys and typing; `None` for the actions handled below.
async fn input(
    lane: &BrowserLane,
    a: &ComputerAction,
    s: &ObservationSpace,
    g: &Grounding,
) -> Option<anyhow::Result<()>> {
    Some(match a {
        ComputerAction::Click {
            button: MouseButton::Back,
            ..
        } => lane.history(-1).await,
        ComputerAction::Click {
            button: MouseButton::Forward,
            ..
        } => lane.history(1).await,
        ComputerAction::Click {
            target,
            button,
            count,
            ..
        } => {
            let name = match button {
                MouseButton::Right => "right",
                MouseButton::Middle => "middle",
                _ => "left",
            };
            match at(target, s, g) {
                Ok(p) => super::act::landed(lane.click(p, name, *count as u32).await),
                Err(e) => Err(e.into()),
            }
        }
        ComputerAction::Keypress { keys } => super::act::landed(lane.press(keys).await),
        ComputerAction::Type { text, .. } => {
            let typed = lane
                .page("Input.insertText", json!({ "text": text }))
                .await
                .map(drop);
            if typed.is_ok() {
                lane.wait_typed(text).await;
            }
            super::act::landed(typed)
        }
        _ => return None,
    })
}

fn wheel(x: f64, y: f64, dx: i32, dy: i32) -> serde_json::Value {
    json!({"type":"mouseWheel","x":x,"y":y,"deltaX":dx,"deltaY":dy})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pixel_wheel_keeps_signed_axes_without_notch_conversion() {
        let p = wheel(5., 9., -21, 300);
        assert_eq!(p["deltaX"], -21);
        assert_eq!(p["deltaY"], 300);
        assert_eq!(capabilities().scroll_units, vec![ScrollUnit::Pixels]);
        assert!(!capabilities().actions.contains(&ActionKind::Drag));
        assert!(capabilities().actions.contains(&ActionKind::Click));
    }
}
