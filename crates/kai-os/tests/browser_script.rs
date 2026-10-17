//! Required-Chromium acceptance of restricted, persistent browser scripts.
mod web_support;

use async_trait::async_trait;
use kai_core::computer::{ComputerPolicy, ToolError, ToolRequest};
use kai_core::ports::NeverStops;
use kai_core::{Actuator, Interrupt, Perceptor, Role};
use kai_os::web::BrowserLane;
use kai_os::web::script::{BrowserScript, ScriptError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use web_support::{click, lane, mark, serve};

struct AllowForFixture;
#[async_trait]
impl ComputerPolicy for AllowForFixture {
    async fn authorize(&self, _: &ToolRequest) -> Result<(), ToolError> {
        Ok(())
    }
}

struct Reject;
#[async_trait]
impl ComputerPolicy for Reject {
    async fn authorize(&self, _: &ToolRequest) -> Result<(), ToolError> {
        Err(ToolError::PermissionDenied)
    }
}

struct RedirectDuringApproval<'a> {
    browser: &'a BrowserLane,
    url: String,
}

#[async_trait]
impl ComputerPolicy for RedirectDuringApproval<'_> {
    async fn authorize(&self, _: &ToolRequest) -> Result<(), ToolError> {
        self.browser
            .open_url(&self.url)
            .await
            .map_err(|_| ToolError::Failed)
    }
}

struct ClickDuringApproval<'a> {
    browser: &'a BrowserLane,
    button: &'static str,
}

#[async_trait]
impl ComputerPolicy for ClickDuringApproval<'_> {
    async fn authorize(&self, _: &ToolRequest) -> Result<(), ToolError> {
        // Simulates an external user or another client updating this page
        // while the script's mutation is waiting for approval.
        let observed = self
            .browser
            .observe()
            .await
            .map_err(|_| ToolError::Failed)?;
        let target = mark(&observed.grounding, Role::Button, self.button);
        self.browser
            .act(&click(target), &observed.grounding)
            .await
            .map_err(|_| ToolError::Failed)
    }
}

struct StopFlag(Arc<AtomicBool>);
impl Interrupt for StopFlag {
    fn stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

const PAGE: &str = r#"<!doctype html><html><head><title>idle</title></head>
<body><button id="start" onclick="setTimeout(() => {
  document.title = 'stage-one';
  const button = document.createElement('button');
  button.id = 'second'; button.textContent = 'Finish';
  button.onclick = () => { document.title = 'complete'; };
  document.getElementById('slot').appendChild(button);
}, 65)">Start</button><div id="slot"></div></body></html>"#;

#[tokio::test(flavor = "multi_thread")]
async fn persistent_script_uses_live_dynamic_page_and_dispatches_every_mutation() {
    let Some((browser, _profile)) = lane().await else {
        return;
    };
    let base = serve(vec![("/", PAGE.to_string())]).await;
    browser.open_url(&base).await.unwrap();
    let script = BrowserScript::new().unwrap();
    let title = || async {
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .unwrap()
    };
    assert_eq!(
        title().await,
        "idle",
        "untouched page must fail the completion oracle"
    );
    let first = script
        .run(
            &browser,
            "const nextButton = '#second'; kai.click('#start'); kai.log('started')",
            &AllowForFixture,
            &NeverStops,
            Duration::from_secs(4),
        )
        .await
        .unwrap();
    assert_eq!(first.results.len(), 1);
    assert!(first.results.iter().all(|r| r.error.is_none()));
    assert_eq!(first.logs, vec!["started"]);
    assert_ne!(
        title().await,
        "complete",
        "partial progress is not completion"
    );

    // A saved lexical variable from the first call now targets a button
    // created asynchronously by the *same authenticated browser session*.
    let second = script
        .run(
            &browser,
            "Promise.resolve().then(() => { kai.wait(180); kai.click(nextButton); });",
            &AllowForFixture,
            &NeverStops,
            Duration::from_secs(4),
        )
        .await
        .unwrap();
    assert_eq!(second.results.len(), 2);
    assert_eq!(first.results[0].session, second.results[0].session);
    assert!(
        first.results[0].id < second.results[0].id,
        "request IDs must remain monotonic across persistent script calls"
    );
    assert!(
        second.results.iter().all(|r| r.error.is_none()),
        "{second:?}"
    );
    assert_eq!(
        title().await,
        "complete",
        "independent browser title oracle"
    );

    let before = title().await;
    let denied = script
        .run(
            &browser,
            "kai.click('#start')",
            &Reject,
            &NeverStops,
            Duration::from_secs(3),
        )
        .await
        .unwrap();
    assert_eq!(denied.results[0].error, Some(ToolError::PermissionDenied));
    assert_eq!(
        title().await,
        before,
        "policy denial must prevent side effects"
    );

    let unsafe_navigation = script
        .run(
            &browser,
            "kai.open('javascript:alert(1)')",
            &AllowForFixture,
            &NeverStops,
            Duration::from_secs(3),
        )
        .await
        .unwrap();
    assert_eq!(
        unsafe_navigation.results[0].error,
        Some(ToolError::InvalidArguments)
    );
    assert_eq!(title().await, before);

    let missing = script
        .run(
            &browser,
            "kai.click('#missing')",
            &AllowForFixture,
            &NeverStops,
            Duration::from_secs(3),
        )
        .await;
    assert!(matches!(missing, Err(ScriptError::Selector(_))));
    assert_eq!(title().await, before);

    let interrupted = script
        .run(
            &browser,
            "while(true) {}",
            &AllowForFixture,
            &NeverStops,
            Duration::from_millis(35),
        )
        .await;
    assert!(
        interrupted.is_err(),
        "infinite script must stop within budget"
    );
    assert_eq!(title().await, before);
}

#[tokio::test(flavor = "multi_thread")]
async fn external_cancel_aborts_infinite_script_before_any_queued_click() {
    let Some((browser, _profile)) = lane().await else {
        return;
    };
    let base = serve(vec![("/", "<title>safe</title><button id='danger' onclick=\"document.title='changed'\">Danger</button>".into())]).await;
    browser.open_url(&base).await.unwrap();
    let script = BrowserScript::new().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let trigger = stop.clone();
    let timer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(70)).await;
        trigger.store(true, Ordering::SeqCst);
    });
    let now = Instant::now();
    let result = script
        .run(
            &browser,
            "kai.click('#danger'); while(true) {}",
            &AllowForFixture,
            &StopFlag(stop),
            Duration::from_secs(5),
        )
        .await;
    timer.await.unwrap();
    assert!(
        matches!(result, Err(ScriptError::Computer(ToolError::Cancelled))),
        "{result:?}"
    );
    assert!(
        now.elapsed() < Duration::from_secs(2),
        "cancellation must not wait for the five-second budget"
    );
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("safe"),
        "a queued click must not run after JS cancellation"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn navigating_during_approval_invalidates_click_and_typed_text() {
    let Some((browser, _profile)) = lane().await else {
        return;
    };
    let source = r#"<title>source</title><button id='submit' onclick="document.title='source clicked'">Go</button><input id='entry' autofocus>"#;
    let decoy = r#"<title>decoy</title><button id='submit' onclick="document.title='decoy clicked'">Go</button><input id='entry' autofocus oninput="document.title='decoy typed'">"#;
    let base = serve(vec![("/source", source.into()), ("/decoy", decoy.into())]).await;
    let redirect = RedirectDuringApproval {
        browser: &browser,
        url: format!("{base}/decoy"),
    };
    browser.open_url(&format!("{base}/source")).await.unwrap();
    let script = BrowserScript::new().unwrap();
    let clicked = script
        .run(
            &browser,
            "kai.click('#submit')",
            &redirect,
            &NeverStops,
            Duration::from_secs(3),
        )
        .await
        .unwrap();
    assert_eq!(clicked.results[0].error, Some(ToolError::StaleObservation));
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("decoy")
    );

    browser.open_url(&format!("{base}/source")).await.unwrap();
    let typed = script
        .run(
            &browser,
            "kai.type('secret')",
            &redirect,
            &NeverStops,
            Duration::from_secs(3),
        )
        .await
        .unwrap();
    assert_eq!(typed.results[0].error, Some(ToolError::StaleObservation));
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("decoy")
    );

    // Reloading the *same* URL also replaces the document even though
    // its selector, geometry, title and location remain identical.
    browser.open_url(&format!("{base}/source")).await.unwrap();
    let reload = RedirectDuringApproval {
        browser: &browser,
        url: format!("{base}/source"),
    };
    let clicked = script
        .run(
            &browser,
            "kai.click('#submit')",
            &reload,
            &NeverStops,
            Duration::from_secs(3),
        )
        .await
        .unwrap();
    assert_eq!(clicked.results[0].error, Some(ToolError::StaleObservation));
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("source")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn non_interactable_dom_targets_are_rejected_without_side_effects() {
    let Some((browser, _profile)) = lane().await else {
        return;
    };
    let page = r#"<title>safe</title>
      <button id='disabled' disabled onclick="document.title='disabled clicked'">Disabled</button>
      <button id='obscured' style='position:absolute;left:100px;top:100px;width:100px;height:50px' onclick="document.title='obscured clicked'">Obscured</button>
      <div style='position:absolute;left:100px;top:100px;width:100px;height:50px;background:#888;z-index:5'></div>"#;
    let base = serve(vec![("/", page.into())]).await;
    browser.open_url(&base).await.unwrap();
    let script = BrowserScript::new().unwrap();
    for selector in ["#disabled", "#obscured"] {
        let result = script
            .run(
                &browser,
                &format!("kai.click('{selector}')"),
                &AllowForFixture,
                &NeverStops,
                Duration::from_secs(3),
            )
            .await;
        assert!(
            matches!(result, Err(ScriptError::Selector(_))),
            "{selector}: {result:?}"
        );
        assert_eq!(
            browser
                .observe_tree_only()
                .await
                .unwrap()
                .window_title
                .as_deref(),
            Some("safe")
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn identical_node_replacement_and_focus_shift_invalidate_approved_actions() {
    let Some((browser, _profile)) = lane().await else {
        return;
    };
    let page = r#"<title>safe</title>
      <button id='target' onclick="document.title='unsafe'">Submit</button>
      <button id='swap' onclick="const old=document.getElementById('target');old.replaceWith(old.cloneNode(true))">Swap</button>
      <input id='first' aria-label='First' autofocus oninput="document.title='first typed'">
      <input id='second' aria-label='Second' oninput="document.title='second typed'">
      <button id='shift' onclick="document.getElementById('second').focus()">Shift Focus</button>"#;
    let base = serve(vec![("/", page.into())]).await;
    browser.open_url(&base).await.unwrap();
    let script = BrowserScript::new().unwrap();

    let swapped = script
        .run(
            &browser,
            "kai.click('#target')",
            &ClickDuringApproval {
                browser: &browser,
                button: "Swap",
            },
            &NeverStops,
            Duration::from_secs(4),
        )
        .await
        .unwrap();
    assert_eq!(swapped.results[0].error, Some(ToolError::StaleObservation));
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("safe")
    );

    let first = browser.observe().await.unwrap();
    browser
        .act(
            &click(mark(&first.grounding, Role::TextField, "First")),
            &first.grounding,
        )
        .await
        .unwrap();
    let typed = script
        .run(
            &browser,
            "kai.type('must-not-insert')",
            &ClickDuringApproval {
                browser: &browser,
                button: "Shift Focus",
            },
            &NeverStops,
            Duration::from_secs(4),
        )
        .await
        .unwrap();
    assert_eq!(typed.results[0].error, Some(ToolError::StaleObservation));
    assert_eq!(
        browser
            .observe_tree_only()
            .await
            .unwrap()
            .window_title
            .as_deref(),
        Some("safe")
    );
}
