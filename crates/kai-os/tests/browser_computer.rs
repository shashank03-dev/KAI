//! Shared computer-tool acceptance against real headless Chromium. A local
//! pointer-events page is the independent witness for modifiers and drags.
mod web_support;

use kai_core::computer::{
    ComputerAction, ExecutionContext, MouseButton, ObservationSpace, Target, ToolError,
};
use kai_core::{Actuator, Grounding, Perceptor, Rect};
use std::sync::OnceLock;
use std::time::Duration;
use web_support::{lane, serve};

// Chrome's DevTools pipe can time out during simultaneous test launches on
// loaded CI machines; these browser acceptance cases share a launch slot.
async fn browser_launch_slot() -> tokio::sync::MutexGuard<'static, ()> {
    static SLOT: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    SLOT.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

const PAGE: &str = r#"<!doctype html><html><head><title>idle</title></head>
<body style="margin:0">
<button style="position:absolute;left:20px;top:10px;width:140px;height:40px"
    onclick="document.title='click:'+event.shiftKey">Shift click</button>
<div id="handle" style="position:absolute;left:60px;top:70px;width:90px;height:90px;
    background:red;touch-action:none">Drag</div>
<script>
let held=false;
document.getElementById('handle').addEventListener('pointerdown', e => {
  held=true; e.target.setPointerCapture(e.pointerId);
});
document.addEventListener('pointermove', e => {
  if (held) document.title='drag:'+Math.round(e.clientX)+':'+e.buttons+':'+e.shiftKey;
});
document.addEventListener('pointerup', e => { held=false; });
</script></body></html>"#;

fn space() -> ObservationSpace {
    ObservationSpace {
        session: 1,
        id: 1,
        image_width: 1280,
        image_height: 800,
        region: Rect::new(0., 0., 1280., 800.),
    }
}

fn at(x: f64, y: f64) -> Target {
    Target::Point {
        observation: 1,
        x,
        y,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn computer_navigation_reaches_page_and_rejects_unsafe_or_cancelled_requests() {
    let _slot = browser_launch_slot().await;
    let Some((browser, _profile)) = lane().await else {
        return;
    };
    let base = serve(vec![
        ("/first", "<title>First page</title>".to_string()),
        ("/second", "<title>Second page</title>".to_string()),
    ])
    .await;
    browser.open_url(&format!("{base}/first")).await.unwrap();
    let initial = browser.observe_tree_only().await.unwrap();
    assert_eq!(initial.window_title.as_deref(), Some("First page"));

    let ctx = ExecutionContext::new(Duration::from_secs(10));
    browser
        .computer_act(
            &ComputerAction::OpenUrl {
                url: format!("{base}/second"),
            },
            &space(),
            &initial.grounding,
            &ctx,
        )
        .await
        .unwrap();
    let arrived = browser.observe_tree_only().await.unwrap();
    assert_eq!(arrived.window_title.as_deref(), Some("Second page"));
    assert_eq!(
        arrived.url.as_deref(),
        Some(format!("{base}/second").as_str())
    );

    assert_eq!(
        browser
            .computer_act(
                &ComputerAction::OpenUrl {
                    url: "javascript:alert(1)".into(),
                },
                &space(),
                &arrived.grounding,
                &ctx,
            )
            .await,
        Err(ToolError::InvalidArguments)
    );

    let cancelled = ExecutionContext::new(Duration::from_secs(10));
    cancelled.cancel();
    assert_eq!(
        browser
            .computer_act(
                &ComputerAction::OpenUrl {
                    url: format!("{base}/first"),
                },
                &space(),
                &arrived.grounding,
                &cancelled,
            )
            .await,
        Err(ToolError::Cancelled)
    );
    let after = browser.observe_tree_only().await.unwrap();
    assert_eq!(after.window_title.as_deref(), Some("Second page"));
    assert_eq!(
        after.url.as_deref(),
        Some(format!("{base}/second").as_str())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn held_shift_click_and_pointer_drag_reach_the_page() {
    let _slot = browser_launch_slot().await;
    let Some((browser, _profile)) = lane().await else {
        return;
    };
    let base = serve(vec![("/", PAGE.to_string())]).await;
    browser.open_url(&format!("{base}/")).await.unwrap();
    let g = browser.observe().await.unwrap().grounding;
    let ctx = ExecutionContext::new(Duration::from_secs(10));
    let click = |modifiers| ComputerAction::Click {
        target: at(90., 30.),
        button: MouseButton::Left,
        count: 1,
        modifiers,
    };
    browser
        .computer_act(&click(vec!["shift".into()]), &space(), &g, &ctx)
        .await
        .unwrap();
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("click:true")
    );
    browser
        .computer_act(&click(vec![]), &space(), &g, &ctx)
        .await
        .unwrap();
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("click:false")
    );

    let drag = ComputerAction::Drag {
        points: vec![at(100., 110.), at(240., 150.), at(360., 200.)],
        button: MouseButton::Left,
        duration_ms: 100,
        modifiers: vec!["shift".into()],
    };
    browser
        .computer_act(&drag, &space(), &g, &ctx)
        .await
        .unwrap();
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("drag:360:1:true")
    );

    let unsupported = ComputerAction::Drag {
        points: vec![at(100., 110.), at(360., 200.)],
        button: MouseButton::Back,
        duration_ms: 100,
        modifiers: vec![],
    };
    assert_eq!(
        browser.computer_act(&unsupported, &space(), &g, &ctx).await,
        Err(ToolError::Unsupported)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn dropped_drag_poisoned_until_restart_and_releases_input() {
    let _slot = browser_launch_slot().await;
    let Some((browser, _profile)) = lane().await else {
        return;
    };
    let base = serve(vec![("/", PAGE.to_string())]).await;
    browser.open_url(&format!("{base}/")).await.unwrap();
    let g = Grounding::Marks(vec![]);
    let action = ComputerAction::Drag {
        points: vec![at(100., 110.), at(360., 200.)],
        button: MouseButton::Left,
        duration_ms: 4000,
        modifiers: vec!["ctrl".into()],
    };
    let ctx = ExecutionContext::new(Duration::from_secs(10));
    let interrupted = tokio::time::timeout(
        Duration::from_millis(200),
        browser.computer_act(&action, &space(), &g, &ctx),
    )
    .await;
    assert!(
        interrupted.is_err(),
        "the drag must still be held at cancellation"
    );
    assert!(
        browser.is_poisoned(),
        "unacknowledged release requires a fresh browser"
    );
    assert_eq!(
        browser.computer_act(&action, &space(), &g, &ctx).await,
        Err(ToolError::Failed)
    );
}
