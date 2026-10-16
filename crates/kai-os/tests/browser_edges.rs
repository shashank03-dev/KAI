//! The browser lane on the pages that break naive drivers, against a real
//! headless Chromium: page dialogs (which freeze the page), new tabs, tabs
//! that close themselves, native dropdowns, file pickers, downloads, a
//! browser that dies, and a profile already in use.

mod web_support;

use kai_core::{Actuator, Grounding, Perceptor, Role};
use kai_os::web::{BrowserLane, LaneOptions, find_browser};
use std::time::{Duration, Instant};
use web_support::{click, decision, key, lane, lane_with, mark, serve, type_into};

const DIALOGS: &str = r#"<title>Dialogs</title>
<button onclick="alert('Saved!')">Alert</button>
<button onclick="document.title = confirm('Delete all 12 files?') ? 'yes' : 'no'">Confirm</button>
<button onclick="document.title = 'name=' + prompt('Your name?', 'Ann')">Prompt</button>
<input aria-label="Notes" oninput="window.onbeforeunload = () => 'unsaved'">"#;

/// Observe without hanging: a frozen page must not cost the 15 s timeout.
async fn quick_look(lane: &BrowserLane) -> kai_core::Observation {
    let t = Instant::now();
    let o = lane.observe().await.unwrap();
    assert!(
        t.elapsed() < Duration::from_secs(3),
        "observe took {:?}",
        t.elapsed()
    );
    o
}

#[tokio::test(flavor = "multi_thread")]
async fn alert_confirm_and_prompt_become_marks_and_are_answered() {
    let Some((lane, _p)) = lane().await else {
        return;
    };
    let base = serve(vec![("/", DIALOGS.into())]).await;
    lane.open_url(&format!("{base}/")).await.unwrap();

    // alert: the click returns at once, the page shows one OK mark.
    let g = lane.observe().await.unwrap().grounding;
    let t = Instant::now();
    lane.act(&click(mark(&g, Role::Button, "Alert")), &g)
        .await
        .unwrap();
    assert!(
        t.elapsed() < Duration::from_secs(3),
        "click hung on the alert"
    );
    let o = quick_look(&lane).await;
    assert!(
        o.images.screenshot_jpeg.is_empty(),
        "a frozen page has no image"
    );
    let ok = mark(&o.grounding, Role::Button, "OK (page dialog: \"Saved!\")");
    // Anything but answering is refused with the reason.
    let err = lane
        .act(
            &decision(r#"{"thought":"","action":"scroll","risk":"none"}"#),
            &o.grounding,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("answer it first"), "{err}");
    lane.act(&click(ok), &o.grounding).await.unwrap();
    let g = lane.observe().await.unwrap().grounding;
    assert!(!g.as_text().contains("page dialog"), "{}", g.as_text());
    // The old OK mark is gone: clicking it again must not click the page.
    let stale = lane.act(&click(ok), &o.grounding).await.unwrap_err();
    assert!(stale.to_string().contains("already closed"), "{stale}");

    // confirm: Cancel says no, OK says yes; the message reaches the name.
    for (choice, want) in [("Cancel", "no"), ("OK", "yes")] {
        let g = lane.observe().await.unwrap().grounding;
        lane.act(&click(mark(&g, Role::Button, "Confirm")), &g)
            .await
            .unwrap();
        let d = quick_look(&lane).await.grounding;
        assert!(d.as_text().contains("Delete all 12 files?"));
        lane.act(&click(mark(&d, Role::Button, choice)), &d)
            .await
            .unwrap();
        let after = lane.observe_tree_only().await.unwrap();
        assert_eq!(after.window_title.as_deref(), Some(want));
    }

    let g = lane.observe().await.unwrap().grounding;
    lane.act(&click(mark(&g, Role::Button, "Confirm")), &g)
        .await
        .unwrap();
    let d = quick_look(&lane).await.grounding;
    let refused = lane.act(&key(r#"["enter"]"#), &d).await.unwrap_err();
    assert!(
        refused.to_string().contains("click its OK mark"),
        "{refused}"
    );
    lane.act(&click(mark(&d, Role::Button, "Cancel")), &d)
        .await
        .unwrap();

    // prompt: type an answer into the dialog's field, then OK.
    let g = lane.observe().await.unwrap().grounding;
    lane.act(&click(mark(&g, Role::Button, "Prompt")), &g)
        .await
        .unwrap();
    let d = quick_look(&lane).await.grounding;
    let field = mark(&d, Role::TextField, "Answer");
    assert_eq!(d.element(field).unwrap().value.as_deref(), Some("Ann"));
    lane.act(&type_into(field, "Bob"), &d).await.unwrap();
    let d = lane.observe().await.unwrap().grounding;
    assert_eq!(d.element(field).unwrap().value.as_deref(), Some("Bob"));
    lane.act(&key(r#"["enter"]"#), &d).await.unwrap();
    let after = lane.observe_tree_only().await.unwrap();
    assert_eq!(after.window_title.as_deref(), Some("name=Bob"));
}

#[tokio::test(flavor = "multi_thread")]
async fn leaving_a_page_with_unsaved_changes_asks_first() {
    let Some((lane, _p)) = lane().await else {
        return;
    };
    let base = serve(vec![("/", DIALOGS.into())]).await;
    lane.open_url(&format!("{base}/")).await.unwrap();
    let g = lane.observe().await.unwrap().grounding;
    lane.act(&type_into(mark(&g, Role::TextField, "Notes"), "draft"), &g)
        .await
        .unwrap();
    lane.open_url(&format!("{base}/echo?left")).await.unwrap();
    let d = quick_look(&lane).await.grounding;
    // Stay: still on the page.
    lane.act(&click(mark(&d, Role::Button, "Stay on page")), &d)
        .await
        .unwrap();
    let here = lane.observe_tree_only().await.unwrap();
    assert_eq!(here.url.as_deref(), Some(format!("{base}/").as_str()));
    // Leave: the navigation goes on.
    lane.open_url(&format!("{base}/echo?left")).await.unwrap();
    let d = quick_look(&lane).await.grounding;
    lane.act(&click(mark(&d, Role::Button, "Leave page")), &d)
        .await
        .unwrap();
    lane.settle(Duration::from_secs(5)).await;
    let gone = lane.observe_tree_only().await.unwrap();
    assert_eq!(gone.window_title.as_deref(), Some("echo left"));
}

#[tokio::test(flavor = "multi_thread")]
async fn new_tabs_are_followed_and_a_closed_tab_hands_back() {
    let Some((lane, _p)) = lane().await else {
        return;
    };
    let base = serve(vec![
        (
            "/",
            r#"<title>Home</title><a href="/second" target="_blank">Open second</a>
<a href="/x">Other</a>"#
                .into(),
        ),
        (
            "/second",
            r#"<title>Second</title><button onclick="window.close()">Close me</button>
<a href="/x">Other</a>"#
                .into(),
        ),
    ])
    .await;
    lane.open_url(&format!("{base}/")).await.unwrap();
    let g = lane.observe().await.unwrap().grounding;
    lane.act(&click(mark(&g, Role::Link, "Open second")), &g)
        .await
        .unwrap();
    lane.settle(Duration::from_secs(5)).await;
    let o = lane.observe().await.unwrap();
    assert_eq!(
        o.window_title.as_deref(),
        Some("Second"),
        "followed the new tab"
    );
    lane.act(
        &click(mark(&o.grounding, Role::Button, "Close me")),
        &o.grounding,
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let back = lane.observe().await.unwrap();
    assert_eq!(back.window_title.as_deref(), Some("Home"), "handed back");
}

#[tokio::test(flavor = "multi_thread")]
async fn native_dropdowns_are_chosen_by_label() {
    let Some((lane, _p)) = lane().await else {
        return;
    };
    let page = r#"<title>Shop</title>
<label>Size <select onchange="document.title='size=' + this.value">
<option value="s">Small</option><option value="m">Medium</option>
<option value="l" disabled>Large</option></select></label>
<button>Buy</button>"#;
    let base = serve(vec![("/", page.into())]).await;
    lane.open_url(&format!("{base}/")).await.unwrap();
    let g = lane.observe().await.unwrap().grounding;
    let size = mark(&g, Role::ComboBox, "Size");
    lane.act(&type_into(size, "medium"), &g).await.unwrap();
    // The engine presses Enter after typing into a dropdown; harmless here.
    lane.act(&key(r#"["enter"]"#), &g).await.unwrap();
    let o = lane.observe_tree_only().await.unwrap();
    assert_eq!(o.window_title.as_deref(), Some("size=m"));
    assert_eq!(
        o.grounding.element(size).unwrap().value.as_deref(),
        Some("Medium")
    );
    let missing = lane.act(&type_into(size, "XXL"), &g).await.unwrap_err();
    assert!(
        missing.to_string().contains("Small, Medium, Large"),
        "{missing}"
    );
    let disabled = lane.act(&type_into(size, "Large"), &g).await.unwrap_err();
    assert!(disabled.to_string().contains("disabled"), "{disabled}");

    // Clicking it opens the list as marks numbered after the page.
    let g = lane.observe().await.unwrap().grounding;
    lane.act(&click(size), &g).await.unwrap();
    let open = lane.observe().await.unwrap().grounding;
    let small = mark(&open, Role::ListItem, "Small (option)");
    assert!(
        small > mark(&open, Role::Button, "Buy"),
        "{}",
        open.as_text()
    );
    let large = mark(&open, Role::ListItem, "Large (option)");
    assert!(!open.element(large).unwrap().enabled);
    lane.act(&click(small), &open).await.unwrap();
    let chosen = lane.observe().await.unwrap();
    assert_eq!(chosen.window_title.as_deref(), Some("size=s"));
    assert!(
        !chosen.grounding.as_text().contains("(option)"),
        "the list closes"
    );
    // Esc closes an open list without choosing.
    lane.act(&click(size), &chosen.grounding).await.unwrap();
    let again = lane.observe().await.unwrap().grounding;
    assert!(again.as_text().contains("(option)"));
    lane.act(&key(r#"["esc"]"#), &again).await.unwrap();
    let closed = lane.observe().await.unwrap();
    assert!(!closed.grounding.as_text().contains("(option)"));
    assert_eq!(closed.window_title.as_deref(), Some("size=s"));
}

#[tokio::test(flavor = "multi_thread")]
async fn file_pickers_and_downloads_are_reported_without_writing_a_file() {
    let downloads = tempfile::tempdir().unwrap();
    let Some((lane, _p)) = lane_with(Some(downloads.path().to_path_buf())).await else {
        return;
    };
    let page = r#"<title>Files</title><label>Upload <input type="file"></label>
<a href="/file.txt">Get the file</a>"#;
    let base = serve(vec![("/", page.into())]).await;
    lane.open_url(&format!("{base}/")).await.unwrap();
    let g = lane.observe().await.unwrap().grounding;
    let upload = mark(&g, Role::Button, "Upload");
    let err = lane.act(&click(upload), &g).await.unwrap_err();
    assert!(err.to_string().contains("file picker"), "{err}");
    // Nothing is left blocking the page.
    let g = quick_look(&lane).await.grounding;
    let notice = lane
        .act(&click(mark(&g, Role::Link, "Get the file")), &g)
        .await
        .unwrap_err();
    assert!(notice.to_string().contains("download"), "{notice}");
    let file = downloads.path().join("file.txt");
    assert!(!file.exists());
}

/// Whether any process's command line contains `pattern`.
fn running(pattern: &str) -> bool {
    std::process::Command::new("pgrep")
        .args(["-f", pattern])
        .stdout(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dead_browser_and_a_busy_profile_fail_clearly() {
    let Some(browser) = find_browser() else {
        return;
    };
    let profile = tempfile::tempdir().unwrap();
    let opts = LaneOptions {
        browser,
        profile: profile.path().to_path_buf(),
        headless: true,
        size: (1280, 800),
        downloads: None,
    };
    let first = BrowserLane::launch(&opts, None).await.unwrap();
    // A second browser on the same profile hands over to the first and
    // exits; Kai must say so, not hang.
    let t = Instant::now();
    let second = BrowserLane::launch(&opts, None).await;
    assert!(t.elapsed() < Duration::from_secs(20));
    let err = second
        .err()
        .expect("a second browser on one profile must fail");
    assert!(err.to_string().contains("exited at start"), "{err}");
    drop(first);

    let lane = BrowserLane::launch(&opts, None).await.unwrap();
    lane.observe().await.unwrap();
    // Kill the browser behind Kai's back, and wait until it is really gone:
    // a busy Mac took more than the old fixed half second to shut Chrome
    // down, so the browser was not dead yet when the clock below started.
    let ours = format!("user-data-dir={}", profile.path().display());
    std::process::Command::new("pkill")
        .args(["-f", &ours])
        .status()
        .unwrap();
    let gone = Instant::now();
    while gone.elapsed() < Duration::from_secs(10) && running(&ours) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let t = Instant::now();
    let err = lane
        .observe()
        .await
        .err()
        .expect("observing a dead browser must fail");
    assert!(t.elapsed() < Duration::from_secs(3));
    assert!(err.to_string().contains("closed"), "{err}");
    let g = Grounding::Marks(vec![]);
    assert!(lane.act(&key(r#"["enter"]"#), &g).await.is_err());
}
