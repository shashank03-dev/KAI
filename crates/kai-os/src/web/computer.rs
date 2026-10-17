//! The shared computer tools over CDP. Click, keys and typing reuse the
//! lane's own paths (release guards, page dialogs, the typed-text wait).
use super::lane::BrowserLane;
use super::{keys, release};
use kai_core::{Grounding, computer::*};
use serde_json::json;
use std::future::Future;
use std::time::Duration;
tokio::task_local! {pub(super) static INPUT_CONTEXT:ExecutionContext;}

pub(super) fn capabilities() -> Capabilities {
    Capabilities {
        actions: vec![
            ActionKind::Observe,
            ActionKind::Click,
            ActionKind::Move,
            ActionKind::Drag,
            ActionKind::Type,
            ActionKind::Keypress,
            ActionKind::Scroll,
            ActionKind::Wait,
            ActionKind::OpenUrl,
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
    INPUT_CONTEXT
        .scope(c.clone(), async {
            let (mods, events) = keys::held_modifiers(a.modifiers());
            with_modifiers(lane, events, dispatch(lane, a, s, g, c, mods)).await
        })
        .await
}

/// Hold modifier keys for the entire mouse action. Every release is tied to
/// the original CDP page session and survives future cancellation.
async fn with_modifiers(
    lane: &BrowserLane,
    events: Vec<(serde_json::Value, serde_json::Value)>,
    action: impl Future<Output = Result<(), ToolError>>,
) -> Result<(), ToolError> {
    let mut releases: Vec<(release::Release<'_>, serde_json::Value)> =
        Vec::with_capacity(events.len());
    for (down, up) in events {
        let release = release::Release::new(
            &lane.cdp,
            &lane.input_poisoned,
            "Input.dispatchKeyEvent",
            up.clone(),
            lane.current().session,
        );
        if lane.page("Input.dispatchKeyEvent", down).await.is_err() {
            release.settle();
            for (prior, _) in releases.into_iter().rev() {
                prior.settle();
            }
            return Err(ToolError::Failed);
        }
        releases.push((release, up));
    }
    let result = action.await;
    if result.is_err() {
        for (held, _) in releases.into_iter().rev() {
            held.settle();
        }
        return result;
    }
    for (held, up) in releases.into_iter().rev() {
        release::guarded(held, async {
            lane.page("Input.dispatchKeyEvent", up).await.map(drop)
        })
        .await
        .map_err(|_| ToolError::Failed)?;
    }
    Ok(())
}

async fn dispatch(
    lane: &BrowserLane,
    a: &ComputerAction,
    s: &ObservationSpace,
    g: &Grounding,
    c: &ExecutionContext,
    mods: i64,
) -> Result<(), ToolError> {
    if let Some(done) = input(lane, a, s, g, mods).await {
        return done.map_err(|_| c.check().err().unwrap_or(ToolError::Failed));
    }
    let params = match a {
        ComputerAction::Observe => return Ok(()),
        ComputerAction::OpenUrl { url } => {
            c.check()?;
            lane.navigate(url).await.map_err(|_| ToolError::Failed)?;
            c.check()?;
            return Ok(());
        }
        ComputerAction::Wait { duration_ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(*duration_ms as u64)).await;
            c.check()?;
            return Ok(());
        }
        ComputerAction::Move { target, .. } => {
            let (x, y) = at(target, s, g)?;
            json!({"type":"mouseMoved","x":x,"y":y,"modifiers":mods})
        }
        ComputerAction::Drag {
            points,
            button,
            duration_ms,
            ..
        } => {
            return drag(lane, points, *button, *duration_ms, (s, g), c, mods).await;
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
            let mut event = wheel(
                x,
                y,
                css(*dx, r.w, s.image_width),
                css(*dy, r.h, s.image_height),
            );
            event["modifiers"] = json!(mods);
            event
        }
        _ => return Err(ToolError::Unsupported),
    };
    c.check()?;
    lane.page("Input.dispatchMouseEvent", params)
        .await
        .map(drop)
        .map_err(|_| ToolError::Failed)
}

async fn drag(
    lane: &BrowserLane,
    points: &[Target],
    button: MouseButton,
    duration_ms: u32,
    observation: (&ObservationSpace, &Grounding),
    c: &ExecutionContext,
    mods: i64,
) -> Result<(), ToolError> {
    let (s, g) = observation;
    let button = match button {
        MouseButton::Left => "left",
        MouseButton::Middle => "middle",
        MouseButton::Right => "right",
        MouseButton::Back | MouseButton::Forward => return Err(ToolError::Unsupported),
    };
    let points: Vec<_> = points
        .iter()
        .map(|p| at(p, s, g))
        .collect::<Result<_, _>>()?;
    let first = points[0];
    let last = *points.last().expect("validated drag has two points");
    let button_mask = match button {
        "left" => 1,
        "right" => 2,
        _ => 4,
    };
    let event =
        |kind: &str, point: (f64, f64), held: i64| drag_event(kind, point, button, held, mods);
    lane.page("Input.dispatchMouseEvent", event("mouseMoved", first, 0))
        .await
        .map_err(|_| ToolError::Failed)?;
    let release = release::Release::new(
        &lane.cdp,
        &lane.input_poisoned,
        "Input.dispatchMouseEvent",
        event("mouseReleased", last, 0),
        lane.current().session,
    );
    release::guarded(release, async {
        lane.page(
            "Input.dispatchMouseEvent",
            event("mousePressed", first, button_mask),
        )
        .await?;
        let interval = u64::from(duration_ms) / (points.len() - 1) as u64;
        for point in points.iter().skip(1).copied() {
            if interval != 0 {
                tokio::time::sleep(Duration::from_millis(interval)).await;
            }
            c.check().map_err(anyhow::Error::from)?;
            lane.page(
                "Input.dispatchMouseEvent",
                event("mouseMoved", point, button_mask),
            )
            .await?;
        }
        lane.page("Input.dispatchMouseEvent", event("mouseReleased", last, 0))
            .await?;
        Ok(())
    })
    .await
    .map_err(|_| c.check().err().unwrap_or(ToolError::Failed))
}

fn drag_event(
    kind: &str,
    (x, y): (f64, f64),
    button: &str,
    held: i64,
    mods: i64,
) -> serde_json::Value {
    json!({"type":kind, "x":x, "y":y, "button":button,
        "buttons":held, "clickCount":1, "modifiers":mods})
}

/// Click, keys and typing; `None` for the actions handled below.
async fn input(
    lane: &BrowserLane,
    a: &ComputerAction,
    s: &ObservationSpace,
    g: &Grounding,
    mods: i64,
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
                Ok(p) => {
                    super::act::landed(lane.click_modified(p, name, *count as u32, mods).await)
                }
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
        assert!(capabilities().actions.contains(&ActionKind::Drag));
        assert!(capabilities().actions.contains(&ActionKind::Click));
    }

    #[test]
    fn browser_computer_actions_advertise_navigation() {
        assert!(capabilities().actions.contains(&ActionKind::OpenUrl));
    }

    #[test]
    fn drag_events_keep_the_button_and_modifiers_held_until_release() {
        let down = drag_event("mousePressed", (100., 90.), "left", 1, 8);
        let move_held = drag_event("mouseMoved", (210., 190.), "left", 1, 8);
        let up = drag_event("mouseReleased", (210., 190.), "left", 0, 8);
        assert_eq!(down["buttons"], 1);
        assert_eq!(move_held["buttons"], 1);
        assert_eq!(move_held["modifiers"], 8);
        assert_eq!(up["buttons"], 0);
        assert_eq!(up["x"], 210.0);
    }
}
