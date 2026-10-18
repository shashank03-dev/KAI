use super::*;

fn save_button() -> RawNode {
    RawNode {
        role: Role::Button,
        name: "Save".into(),
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        enabled: true,
        visible: true,
        secure: false,
        focused: false,
        value: None,
        handle: None,
    }
}

#[tokio::test]
async fn a_read_that_stops_at_its_deadline_keeps_what_it_read() {
    // Like the readers: work until the deadline, then hand back the
    // partial tree (which arrives just after the budget).
    let nodes = within_budget(Duration::from_millis(50), |deadline| async move {
        tokio::time::sleep_until((deadline + Duration::from_millis(5)).into()).await;
        Ok(vec![save_button()])
    })
    .await;
    assert_eq!(nodes.len(), 1);
}

#[tokio::test]
async fn a_hung_read_falls_back_to_the_grid() {
    let nodes = within_budget(Duration::from_millis(50), |_| async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok(vec![save_button()])
    })
    .await;
    assert!(nodes.is_empty());
}

#[tokio::test]
async fn cancelled_accessibility_work_is_rejected_before_live_resolution() {
    let ctx = kai_core::computer::ExecutionContext::new(std::time::Duration::from_secs(1));
    ctx.cancel();
    let d =
        serde_json::from_value(serde_json::json!({"thought":"click","action":"click","target":1}))
            .unwrap();
    assert!(
        act_context(&d, &Grounding::Marks(vec![]), &ctx)
            .await
            .is_err()
    );
}

#[test]
fn field_values_are_text_fields_only_never_secure_and_clipped() {
    let some = |t: &str| Some(t.to_string());
    assert_eq!(field_value(Role::TextField, false, some("hi")), some("hi"));
    assert_eq!(field_value(Role::TextField, true, some("hunter2")), None);
    assert_eq!(field_value(Role::PasswordField, false, some("x")), None);
    assert_eq!(field_value(Role::Button, false, some("OK")), None);
    assert_eq!(field_value(Role::TextField, false, some("  ")), None);
    let long = "é".repeat(500);
    let kept = field_value(Role::TextField, false, Some(long)).unwrap();
    assert_eq!(kept.chars().count(), MAX_VALUE_CHARS);
}

const SCREEN: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 1920.0,
    h: 1080.0,
};

fn node(role: Role, name: &str, x: f32, y: f32) -> RawNode {
    RawNode {
        role,
        name: name.into(),
        bounds: Rect::new(x, y, 80.0, 24.0),
        enabled: true,
        visible: true,
        secure: false,
        focused: false,
        value: None,
        handle: None,
    }
}

#[test]
fn picked_indices_follow_marks() {
    let nodes = [
        node(Role::Button, "B", 300.0, 10.0),
        node(Role::Image, "logo", 10.0, 90.0),
        node(Role::Button, "A", 10.0, 12.0),
    ];
    let (g, picked) = ground_indexed(&nodes, SCREEN);
    assert!(matches!(g, Grounding::Marks(_)));
    assert_eq!(picked, [2, 0]);
    assert!(ground_indexed(&[], SCREEN).1.is_empty());
}

#[test]
fn numbers_in_reading_order() {
    let g = ground(
        &[
            node(Role::Button, "B", 300.0, 10.0),
            node(Role::Button, "A", 10.0, 12.0),
            node(Role::Link, "C", 10.0, 400.0),
        ],
        SCREEN,
    );
    let Grounding::Marks(m) = g else {
        panic!("expected marks")
    };
    assert_eq!(
        m.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["A", "B", "C"]
    );
    assert_eq!(m.iter().map(|e| e.mark).collect::<Vec<_>>(), [1, 2, 3]);
}

#[test]
fn hidden_tiny_offscreen_and_decorative_are_dropped() {
    let mut hidden = node(Role::Button, "hidden", 10.0, 10.0);
    hidden.visible = false;
    let mut tiny = node(Role::Button, "tiny", 10.0, 50.0);
    tiny.bounds.w = 1.0;
    let nodes = [
        hidden,
        tiny,
        node(Role::Button, "off", 5000.0, 10.0),
        node(Role::Image, "logo", 10.0, 90.0),
        node(Role::Button, "ok", 10.0, 200.0),
        node(Role::Link, "home", 10.0, 300.0),
    ];
    let Grounding::Marks(m) = ground(&nodes, SCREEN) else {
        panic!()
    };
    assert_eq!(m.len(), 2);
}

#[test]
fn duplicates_collapse() {
    let Grounding::Marks(m) = ground(
        &[
            node(Role::Button, "Save", 10.0, 10.0),
            node(Role::Button, "Save", 10.5, 10.5),
            node(Role::Link, "x", 10.0, 300.0),
        ],
        SCREEN,
    ) else {
        panic!()
    };
    assert_eq!(m.len(), 2);
}

#[test]
fn password_role_is_marked_secure() {
    let Grounding::Marks(m) = ground(
        &[
            node(Role::PasswordField, "pw", 10.0, 10.0),
            node(Role::Button, "go", 10.0, 100.0),
        ],
        SCREEN,
    ) else {
        panic!()
    };
    assert!(m[0].secure);
}

#[test]
fn empty_tree_falls_back_to_grid() {
    assert!(matches!(
        ground(&[], SCREEN),
        Grounding::Grid {
            cols: 16,
            rows: 9,
            ..
        }
    ));
}

#[test]
fn mark_count_is_capped() {
    let nodes: Vec<RawNode> = (0..400)
        .map(|i| {
            node(
                Role::Button,
                "b",
                (i % 20) as f32 * 90.0,
                (i / 20) as f32 * 30.0,
            )
        })
        .collect();
    let Grounding::Marks(m) = ground(&nodes, SCREEN) else {
        panic!()
    };
    assert_eq!(m.len(), MAX_MARKS);
}

#[test]
fn long_names_are_trimmed_to_one_line() {
    assert_eq!(clean("a\n  b \"c\""), "a b 'c'");
    assert_eq!(clean(&"x".repeat(100)).chars().count(), 61);
}
