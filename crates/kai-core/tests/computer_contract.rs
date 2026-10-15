use kai_core::Rect;
use kai_core::computer::*;

fn space() -> ObservationSpace {
    ObservationSpace {
        session: 7,
        id: 9,
        image_width: 100,
        image_height: 50,
        region: Rect::new(-100., 20., 200., 200.),
    }
}
fn caps() -> Capabilities {
    Capabilities::all()
}
fn request(action: ComputerAction) -> ToolRequest {
    ToolRequest {
        version: 1,
        id: 1,
        session: 7,
        timeout_ms: 1000,
        action,
    }
}
fn target() -> Target {
    Target::Point {
        observation: 9,
        x: 25.,
        y: 10.,
    }
}

#[test]
fn points_map_each_axis_and_origin_without_clamping() {
    assert_eq!(space().point(25., 10.).unwrap(), (-50., 60.));
    for (x, y) in [
        (-1., 0.),
        (100., 0.),
        (0., 50.),
        (f64::NAN, 0.),
        (0., f64::INFINITY),
    ] {
        assert!(space().point(x, y).is_err());
    }
}

#[test]
fn unknown_version_session_and_stale_targets_are_rejected() {
    let mut r = request(ComputerAction::Move { target: target() });
    r.version = 2;
    assert_eq!(
        r.validate(&caps(), &space()).unwrap_err(),
        ToolError::InvalidArguments
    );
    r.version = 1;
    r.session = 8;
    assert_eq!(
        r.validate(&caps(), &space()).unwrap_err(),
        ToolError::StaleObservation
    );
    r.session = 7;
    r.action = ComputerAction::Move {
        target: Target::Point {
            observation: 8,
            x: 1.,
            y: 1.,
        },
    };
    assert_eq!(
        r.validate(&caps(), &space()).unwrap_err(),
        ToolError::StaleObservation
    );
}

#[test]
fn unsupported_buttons_and_units_do_not_fall_back() {
    let mut c = caps();
    c.buttons = vec![MouseButton::Left];
    let click = request(ComputerAction::Click {
        target: target(),
        button: MouseButton::Middle,
        count: 1,
    });
    assert_eq!(
        click.validate(&c, &space()).unwrap_err(),
        ToolError::Unsupported
    );
    c.scroll_units = vec![ScrollUnit::WheelNotches];
    let scroll = request(ComputerAction::Scroll {
        target: target(),
        dx: 1,
        dy: 2,
        unit: ScrollUnit::Pixels,
    });
    assert_eq!(
        scroll.validate(&c, &space()).unwrap_err(),
        ToolError::Unsupported
    );
}

#[test]
fn scroll_preserves_zero_and_both_signed_axes() {
    for (dx, dy) in [(0, 0), (-150, 250), (4096, -4096)] {
        let r = request(ComputerAction::Scroll {
            target: target(),
            dx,
            dy,
            unit: ScrollUnit::Pixels,
        });
        assert!(r.validate(&caps(), &space()).is_ok());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["action"]["dx"], dx);
        assert_eq!(json["action"]["dy"], dy);
    }
    let r = request(ComputerAction::Scroll {
        target: target(),
        dx: 4097,
        dy: 0,
        unit: ScrollUnit::Pixels,
    });
    assert_eq!(
        r.validate(&caps(), &space()).unwrap_err(),
        ToolError::InvalidArguments
    );
}

#[test]
fn drag_key_text_and_batch_limits_are_enforced() {
    let action = |points| ComputerAction::Drag {
        points,
        button: MouseButton::Left,
        duration_ms: 50,
    };
    assert!(
        request(action(vec![target(), target()]))
            .validate(&caps(), &space())
            .is_ok()
    );
    for points in [vec![target()], vec![target(); 257]] {
        assert_eq!(
            request(action(points))
                .validate(&caps(), &space())
                .unwrap_err(),
            ToolError::InvalidArguments
        );
    }
    for keys in [
        vec![],
        vec!["ctrl".into(), "bogus".into()],
        vec!["a".into(), "b".into()],
    ] {
        assert_eq!(
            request(ComputerAction::Keypress { keys })
                .validate(&caps(), &space())
                .unwrap_err(),
            ToolError::InvalidArguments
        );
    }
    let text = request(ComputerAction::Type {
        target: target(),
        text: "界".repeat(3000),
    });
    assert_eq!(
        text.validate(&caps(), &space()).unwrap_err(),
        ToolError::InvalidArguments
    );
    let r = request(ComputerAction::Wait { duration_ms: 1 });
    let requests: Vec<_> = (1..=4).map(|id| ToolRequest { id, ..r.clone() }).collect();
    assert!(validate_batch(&requests, &caps(), &space()).is_ok());
    assert_eq!(
        validate_batch(&vec![r.clone(); 2], &caps(), &space()).unwrap_err(),
        ToolError::InvalidArguments
    );
    assert_eq!(
        validate_batch(&vec![r; 5], &caps(), &space()).unwrap_err(),
        ToolError::InvalidArguments
    );
}

#[test]
fn serde_rejects_unknown_action_and_unknown_fields() {
    for value in [
        serde_json::json!({"type":"fly"}),
        serde_json::json!({"type":"wait","duration_ms":1,"extra":true}),
    ] {
        assert!(serde_json::from_value::<ComputerAction>(value).is_err());
    }
}
