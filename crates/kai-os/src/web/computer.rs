//! Narrow shared CDP surface. Other actions remain unavailable until their
//! cancellation and release semantics pass browser acceptance fixtures.
use super::lane::BrowserLane;
use kai_core::{Grounding, computer::*};
use serde_json::json;
tokio::task_local! {pub(super) static INPUT_CONTEXT:ExecutionContext;}

pub(super) fn capabilities() -> Capabilities {
    Capabilities {
        actions: vec![
            ActionKind::Observe,
            ActionKind::Move,
            ActionKind::Scroll,
            ActionKind::Wait,
        ],
        buttons: vec![],
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
    let params = match a {
        ComputerAction::Observe => return Ok(()),
        ComputerAction::Wait { duration_ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(*duration_ms as u64)).await;
            c.check()?;
            return Ok(());
        }
        ComputerAction::Move { target } => {
            let (x, y) = at(target, s, g)?;
            json!({"type":"mouseMoved","x":x,"y":y})
        }
        ComputerAction::Scroll { target, dx, dy, .. } => {
            if *dx == 0 && *dy == 0 {
                return Ok(());
            }
            let (x, y) = at(target, s, g)?;
            wheel(x, y, *dx, *dy)
        }
        _ => return Err(ToolError::Unsupported),
    };
    c.check()?;
    lane.page("Input.dispatchMouseEvent", params)
        .await
        .map(drop)
        .map_err(|_| ToolError::Failed)
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
    }
}
