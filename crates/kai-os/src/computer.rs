//! Native implementation of the shared tools, reusing Enigo and launchers.
use crate::DesktopActuator;
use kai_core::computer::*;
use kai_core::{Actuator, Grounding};

pub fn capabilities() -> Capabilities {
    let mut c = Capabilities::all();
    c.scroll_units = vec![ScrollUnit::WheelNotches];
    // macOS back/forward buttons have platform-specific behavior; do not
    // advertise them until the native fixture verifies their semantics.
    c.buttons = vec![MouseButton::Left, MouseButton::Middle, MouseButton::Right];
    c
}

pub fn point(
    target: &Target,
    space: &ObservationSpace,
    g: &Grounding,
) -> Result<(i32, i32), ToolError> {
    target.validate(space)?;
    let (x, y) = match target {
        Target::Point { x, y, .. } => space.point(*x, *y)?,
        Target::Element { mark, .. } => {
            let (x, y) = g
                .target_rect(*mark)
                .ok_or(ToolError::StaleObservation)?
                .center();
            (x as f64, y as f64)
        }
    };
    if !x.is_finite()
        || !y.is_finite()
        || x < i32::MIN as f64
        || x > i32::MAX as f64
        || y < i32::MIN as f64
        || y > i32::MAX as f64
    {
        return Err(ToolError::InvalidArguments);
    }
    Ok((x.floor() as i32, y.floor() as i32))
}

pub async fn act(
    hands: &DesktopActuator,
    action: &ComputerAction,
    space: &ObservationSpace,
    grounding: &Grounding,
    ctx: &ExecutionContext,
) -> Result<(), ToolError> {
    let request = ToolRequest {
        version: 1,
        id: 0,
        session: space.session,
        timeout_ms: 30_000,
        action: action.clone(),
    };
    request.validate(&capabilities(), space)?;
    ctx.check()?;
    let input = hands.input.with_context(ctx);
    let at = |target| point(target, space, grounding);
    let result = match action {
        ComputerAction::Observe => Ok(()),
        ComputerAction::Move { target } => {
            let (x, y) = at(target)?;
            input.move_to(x, y).await
        }
        ComputerAction::Click {
            target,
            button,
            count,
        } => {
            let (x, y) = at(target)?;
            input.click_button(x, y, *button, *count).await
        }
        ComputerAction::Drag {
            points,
            button,
            duration_ms,
        } => {
            let points = points.iter().map(at).collect::<Result<Vec<_>, _>>()?;
            input.drag(points, *button, *duration_ms).await
        }
        ComputerAction::Type { target, text } => {
            // A click is not evidence that focus moved. Only type into the
            // eligible field which the fresh tree confirms already has focus.
            let Target::Element { mark, .. } = target else {
                return Err(ToolError::NeedsUser);
            };
            let el = grounding
                .element(*mark)
                .ok_or(ToolError::StaleObservation)?;
            if !el.enabled
                || !el.focused
                || !matches!(
                    el.role,
                    kai_core::Role::TextField
                        | kai_core::Role::PasswordField
                        | kai_core::Role::ComboBox
                )
            {
                return Err(ToolError::NeedsUser);
            }
            ctx.check()?;
            input.type_text(text).await
        }
        ComputerAction::Keypress { keys } => input.keys(keys).await,
        ComputerAction::Scroll { target, dx, dy, .. } => {
            let (x, y) = at(target)?;
            input.wheel(x, y, *dx, *dy).await
        }
        ComputerAction::Wait { duration_ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(*duration_ms as u64)).await;
            Ok(())
        }
        ComputerAction::OpenApp { name } => {
            let scoped = DesktopActuator {
                input: input.clone(),
                shell: hands.shell.clone(),
            };
            scoped.open_app(name).await
        }
        ComputerAction::OpenUrl { url } => {
            let scoped = DesktopActuator {
                input: input.clone(),
                shell: hands.shell.clone(),
            };
            scoped.open_url(url).await
        }
    };
    result.map_err(|_| ctx.check().err().unwrap_or(ToolError::Failed))
}
