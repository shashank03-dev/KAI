use super::*;

#[test]
fn parses_named_and_single_keys() {
    assert!(matches!(parse_key("ctrl").unwrap(), Key::Control));
    assert!(matches!(parse_key("enter").unwrap(), Key::Return));
    assert!(matches!(parse_key("s").unwrap(), Key::Unicode('s')));
    assert!(matches!(parse_key("f5").unwrap(), Key::F5));
    assert!(parse_key("hyperdrive").is_err());
}

#[derive(Default)]
struct Fake {
    log: Vec<String>,
    moves: usize,
    cancel: Option<ExecutionContext>,
    fail_release: bool,
}
impl Keyboard for Fake {
    fn fast_text(&mut self, _: &str) -> enigo::InputResult<Option<()>> {
        Ok(Some(()))
    }
    fn raw(&mut self, _: u16, _: Press) -> enigo::InputResult<()> {
        Ok(())
    }
    fn key(&mut self, key: Key, dir: Press) -> enigo::InputResult<()> {
        self.log.push(format!("{key:?} {dir:?}"));
        if key == Key::Unicode('x') {
            return Err(enigo::InputError::Simulate("failed"));
        }
        Ok(())
    }
}
impl Mouse for Fake {
    fn button(&mut self, button: Button, dir: Press) -> enigo::InputResult<()> {
        self.log.push(format!("{button:?} {dir:?}"));
        if self.fail_release && dir == Press::Release {
            return Err(enigo::InputError::Simulate("release failed"));
        }
        Ok(())
    }
    fn move_mouse(&mut self, _: i32, _: i32, _: Coordinate) -> enigo::InputResult<()> {
        self.moves += 1;
        if self.moves == 2
            && let Some(ctx) = &self.cancel
        {
            ctx.cancel();
        }
        Ok(())
    }
    fn scroll(&mut self, amount: i32, axis: Axis) -> enigo::InputResult<()> {
        self.log.push(format!("scroll {axis:?} {amount}"));
        Ok(())
    }
    fn main_display(&self) -> enigo::InputResult<(i32, i32)> {
        Ok((100, 100))
    }
    fn location(&self) -> enigo::InputResult<(i32, i32)> {
        Ok((0, 0))
    }
}
#[test]
fn a_failed_chord_releases_modifiers_already_pressed() {
    let mut fake = Fake::default();
    assert!(
        run(
            &mut fake,
            Cmd::Keys(vec![Key::Control, Key::Unicode('x')]),
            None
        )
        .is_err()
    );
    assert_eq!(fake.log.last().unwrap(), "Control Release");
}

#[test]
fn cancelled_drag_releases_button_and_skips_remaining_points() {
    let ctx = ExecutionContext::new(Duration::from_secs(1));
    let mut fake = Fake {
        cancel: Some(ctx.clone()),
        ..Fake::default()
    };
    assert!(
        run(
            &mut fake,
            Cmd::Drag {
                points: vec![(10, 10), (20, 20), (30, 30)],
                button: Button::Left,
                duration_ms: 0
            },
            Some(&ctx)
        )
        .is_err()
    );
    assert_eq!(fake.moves, 2);
    assert_eq!(fake.log, vec!["Left Press", "Left Release"]);
}

#[test]
fn queued_cancelled_input_does_nothing() {
    let ctx = ExecutionContext::new(Duration::from_secs(1));
    ctx.cancel();
    let mut fake = Fake::default();
    assert!(run(&mut fake, Cmd::Move { x: 10, y: 10 }, Some(&ctx)).is_err());
    assert_eq!(fake.moves, 0);
}

#[test]
fn wheel_scroll_preserves_both_axes_and_zero_does_nothing() {
    let mut fake = Fake::default();
    run(
        &mut fake,
        Cmd::Scroll2 {
            x: 10,
            y: 10,
            dx: -2,
            dy: 3,
        },
        None,
    )
    .unwrap();
    assert_eq!(fake.log, vec!["scroll Horizontal -2", "scroll Vertical 3"]);
    fake.log.clear();
    run(
        &mut fake,
        Cmd::Scroll2 {
            x: 10,
            y: 10,
            dx: 0,
            dy: 0,
        },
        None,
    )
    .unwrap();
    assert!(fake.log.is_empty());
}

#[test]
fn failed_drag_cleanup_marks_the_session_unsafe_even_when_cancelled() {
    let ctx = ExecutionContext::new(Duration::from_secs(1));
    let mut fake = Fake {
        cancel: Some(ctx.clone()),
        fail_release: true,
        ..Fake::default()
    };
    let error = run(
        &mut fake,
        Cmd::Drag {
            points: vec![(0, 0), (1, 1), (2, 2)],
            button: Button::Left,
            duration_ms: 0,
        },
        Some(&ctx),
    )
    .unwrap_err();
    assert!(error.downcast_ref::<execute::CleanupFailed>().is_some());
}

#[test]
fn held_modifiers_wrap_the_click_and_are_released() {
    let mut fake = Fake::default();
    let click = Cmd::Click {
        x: 1,
        y: 2,
        button: Button::Left,
        count: 1,
    };
    let held = Cmd::Held {
        keys: vec![Key::Shift],
        inner: Box::new(click),
    };
    run(&mut fake, held, None).unwrap();
    assert_eq!(
        fake.log,
        ["Shift Press", "Left Press", "Left Release", "Shift Release"]
    );
}

#[test]
fn held_modifiers_are_released_when_the_action_fails() {
    // The inner chord fails on its second key; Shift must still come up.
    let mut fake = Fake::default();
    let held = Cmd::Held {
        keys: vec![Key::Shift],
        inner: Box::new(Cmd::Keys(vec![Key::Control, Key::Unicode('x')])),
    };
    assert!(run(&mut fake, held, None).is_err());
    assert_eq!(fake.log.last().unwrap(), "Shift Release");
}
