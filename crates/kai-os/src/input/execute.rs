use super::*;

pub(super) fn run(
    e: &mut (impl Mouse + Keyboard),
    cmd: Cmd,
    context: Option<&ExecutionContext>,
) -> anyhow::Result<(i32, i32)> {
    check(context)?;
    match cmd {
        Cmd::Move { x, y } => e.move_mouse(x, y, Coordinate::Abs)?,
        Cmd::Drag {
            points,
            button,
            duration_ms,
        } => {
            anyhow::ensure!(
                (2..=256).contains(&points.len()) && duration_ms <= 5000,
                "invalid drag"
            );
            e.move_mouse(points[0].0, points[0].1, Coordinate::Abs)?;
            check(context)?;
            let result = (|| -> anyhow::Result<()> {
                e.button(button, Press::Press)?;
                let step = Duration::from_millis(duration_ms as u64 / (points.len() - 1) as u64);
                for (x, y) in points.into_iter().skip(1) {
                    pause(context, step)?;
                    check(context)?;
                    e.move_mouse(x, y, Coordinate::Abs)?;
                }
                check(context)
            })();
            let released = e.button(button, Press::Release);
            released.map_err(|_| anyhow::anyhow!(CleanupFailed))?;
            result?;
        }
        Cmd::Scroll2 { x, y, dx, dy } => {
            if dx == 0 && dy == 0 {
                return Ok((0, 0));
            }
            e.move_mouse(x, y, Coordinate::Abs)?;
            if dx != 0 {
                check(context)?;
                e.scroll(dx, Axis::Horizontal)?;
            }
            if dy != 0 {
                check(context)?;
                e.scroll(dy, Axis::Vertical)?;
            }
        }
        Cmd::Click {
            x,
            y,
            button,
            count,
        } => {
            e.move_mouse(x, y, Coordinate::Abs)?;
            pause(context, Duration::from_millis(30))?;
            for _ in 0..count.max(1) {
                check(context)?;
                let pressed = e.button(button, Press::Press);
                let released = e.button(button, Press::Release);
                released.map_err(|_| anyhow::anyhow!(CleanupFailed))?;
                pressed?;
            }
        }
        Cmd::Type(t) => e.text(&t)?,
        Cmd::Held { keys, inner } => {
            let mut held = Vec::new();
            let result = (|| -> anyhow::Result<(i32, i32)> {
                for k in keys {
                    check(context)?;
                    held.push(k);
                    e.key(k, Press::Press)?;
                }
                run(e, *inner, context)
            })();
            release(e, &held)?;
            return result;
        }
        Cmd::Keys(keys) => {
            let mut held = Vec::new();
            let result = (|| -> anyhow::Result<()> {
                for k in keys {
                    check(context)?;
                    held.push(k);
                    e.key(k, Press::Press)?;
                }
                Ok(())
            })();
            release(e, &held)?;
            result?;
        }
        Cmd::Scroll { x, y, dir, amount } => {
            e.move_mouse(x, y, Coordinate::Abs)?;
            let (len, axis) = match dir {
                Direction::Down => (amount, Axis::Vertical),
                Direction::Up => (-amount, Axis::Vertical),
                Direction::Right => (amount, Axis::Horizontal),
                Direction::Left => (-amount, Axis::Horizontal),
            };
            check(context)?;
            e.scroll(len, axis)?;
        }
        Cmd::Cursor => return Ok(e.location()?),
    }
    Ok((0, 0))
}

/// Release `held` in reverse; a key left down makes further input unsafe.
fn release(e: &mut impl Keyboard, held: &[Key]) -> anyhow::Result<()> {
    let mut cleanup = Ok(());
    for k in held.iter().rev() {
        if let Err(error) = e.key(*k, Press::Release) {
            cleanup = Err(error);
        }
    }
    cleanup.map_err(|_| anyhow::anyhow!(CleanupFailed))
}

fn check(context: Option<&ExecutionContext>) -> anyhow::Result<()> {
    if let Some(c) = context {
        c.check()?;
    }
    Ok(())
}

fn pause(context: Option<&ExecutionContext>, duration: Duration) -> anyhow::Result<()> {
    let until = std::time::Instant::now() + duration;
    while std::time::Instant::now() < until {
        check(context)?;
        thread::sleep(
            until
                .saturating_duration_since(std::time::Instant::now())
                .min(Duration::from_millis(25)),
        );
    }
    check(context)
}

#[derive(Debug)]
pub(super) struct CleanupFailed;
impl std::fmt::Display for CleanupFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("input release failed; restart the input session")
    }
}
impl std::error::Error for CleanupFailed {}
