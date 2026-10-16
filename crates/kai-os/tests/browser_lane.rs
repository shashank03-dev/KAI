//! The browser lane end to end against a real Chromium (headless) and a tiny
//! local web server: marks from the page's tree, a password field forced
//! secure, typing, a form submit, the address policy, back navigation, and
//! a full engine run with scripted replies. Skipped (passes with a note)
//! when no Chromium-family browser is found; set `KAI_BROWSER` to point at one.

use kai_core::engine::{Engine, NoSteering};
use kai_core::{
    Action, Actuator, ApprovalLevel, Decision, Grounding, MockClient, Outcome, Perceptor, Role,
};
use kai_os::web::{BrowserLane, LaneOptions, find_browser};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const FORM: &str = r#"<!doctype html><html><head><title>Contact us</title></head>
<body style="margin:0;font:16px sans-serif">
<h1>Contact</h1>
<form action="/thanks" method="get">
<label>Name <input name="n"></label>
<label>Password <input type="password" name="p"></label>
<button type="submit">Send</button>
</form>
<a href="/about">About us</a>
<div style="height:1500px"></div>
<button onclick="this.textContent='Pressed'">Far below</button> <a href="/">Back to top</a>
</body></html>"#;

/// A busy page: 150 controls in a grid that fits one 1280x800 viewport
/// (the mark cap), for timing.
fn big_page() -> String {
    let mut b = String::from("<title>Big</title><body style='margin:0;font:11px sans-serif'>");
    for i in 0..150 {
        let el = match i % 3 {
            0 => format!("<a href='/x{i}'>Link {i}</a>"),
            1 => format!("<button>Button {i}</button>"),
            _ => format!("<input aria-label='Field {i}' size=6>"),
        };
        b.push_str(&format!(
            "<div style='display:inline-block;width:120px;height:44px'>{el}</div>"
        ));
    }
    b
}

/// Serve `FORM` at `/`, a 150-control page at `/big`, a thanks page at `/thanks?…`, 404 elsewhere.
async fn serve() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body) = if path == "/" {
                    ("200 OK", FORM.to_string())
                } else if path == "/big" {
                    ("200 OK", big_page())
                } else if path.starts_with("/thanks") {
                    let who = path
                        .split("n=")
                        .nth(1)
                        .unwrap_or("")
                        .split('&')
                        .next()
                        .unwrap_or("");
                    (
                        "200 OK",
                        format!("<title>Thanks {who}</title><p>Thanks, {who}!</p>"),
                    )
                } else {
                    ("404 Not Found", "<title>Missing</title>".to_string())
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

async fn lane() -> Option<(BrowserLane, tempfile::TempDir)> {
    let Some(browser) = find_browser() else {
        // CI sets KAI_REQUIRE_BROWSER so a missing browser fails, not passes.
        assert!(
            std::env::var_os("KAI_REQUIRE_BROWSER").is_none(),
            "KAI_REQUIRE_BROWSER is set but no Chromium-family browser was found"
        );
        eprintln!("no Chromium-family browser found (set KAI_BROWSER); skipping");
        return None;
    };
    let profile = tempfile::tempdir().unwrap();
    let opts = LaneOptions {
        browser,
        profile: profile.path().to_path_buf(),
        headless: true,
        size: (1280, 800),
        downloads: None,
    };
    Some((BrowserLane::launch(&opts, None).await.unwrap(), profile))
}

fn mark(g: &Grounding, role: Role, name: &str) -> u32 {
    let Grounding::Marks(els) = g else {
        panic!("expected marks, got {g:?}")
    };
    els.iter()
        .find(|e| e.role == role && e.name.contains(name))
        .unwrap_or_else(|| panic!("no {role:?} {name:?} in {}", g.as_text()))
        .mark
}

fn decision(json: &str) -> Decision {
    Decision::parse_checked(json).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn page_marks_typing_submit_policy_and_back() {
    let Some((lane, _profile)) = lane().await else {
        return;
    };
    let base = serve().await;
    lane.open_url(&format!("{base}/")).await.unwrap();

    let obs = lane.observe().await.unwrap();
    assert_eq!(obs.window_title.as_deref(), Some("Contact us"));
    assert_eq!(obs.url.as_deref(), Some(format!("{base}/").as_str()));
    assert!(!obs.images.screenshot_jpeg.is_empty());
    let g = obs.grounding;
    let name = mark(&g, Role::TextField, "Name");
    let pw = mark(&g, Role::PasswordField, "Password");
    assert!(g.element(pw).unwrap().secure, "password fields are secure");
    assert!(
        !g.as_text().contains("Far below"),
        "below the fold is not marked"
    );

    // Type, select all, delete, type again: the field holds only "Bob".
    let typed = |t: &str| {
        decision(&format!(
            r#"{{"thought":"","action":"type","target":{name},"text":"{t}","risk":"low"}}"#
        ))
    };
    let key = |k: &str| {
        decision(&format!(
            r#"{{"thought":"","action":"key","keys":{k},"risk":"none"}}"#
        ))
    };
    lane.act(&typed("Bobby"), &g).await.unwrap();
    lane.act(&key(r#"["ctrl","a"]"#), &g).await.unwrap();
    lane.act(&key(r#"["backspace"]"#), &g).await.unwrap();
    lane.act(&typed("Bob"), &g).await.unwrap();
    let after = lane.observe_tree_only().await.unwrap();
    assert!(after.images.screenshot_jpeg.is_empty());
    let field = after.grounding.element(name).unwrap();
    assert_eq!(field.value.as_deref(), Some("Bob"));
    // Typing into a filled field replaces its value; it never appends.
    lane.act(&typed("Alice"), &g).await.unwrap();
    lane.act(&typed("Bob"), &g).await.unwrap();
    let again = lane.observe_tree_only().await.unwrap();
    assert_eq!(
        again.grounding.element(name).unwrap().value.as_deref(),
        Some("Bob")
    );

    // Enter submits the form; the lane reports the real address and title.
    lane.act(
        &decision(r#"{"thought":"","action":"key","keys":["enter"],"risk":"high"}"#),
        &g,
    )
    .await
    .unwrap();
    lane.settle(Duration::from_secs(5)).await;
    let done = lane.observe_tree_only().await.unwrap();
    assert!(done.url.unwrap().contains("/thanks?n=Bob"), "submitted");
    assert_eq!(done.window_title.as_deref(), Some("Thanks Bob"));

    // Back returns to the form.
    lane.act(
        &decision(r#"{"thought":"","action":"key","keys":["alt","left"],"risk":"none"}"#),
        &g,
    )
    .await
    .unwrap();
    let back = lane.observe_tree_only().await.unwrap();
    assert_eq!(back.url.as_deref(), Some(format!("{base}/").as_str()));

    // Scrolling brings the far button into view and gives it a mark.
    lane.act(
        &decision(
            r#"{"thought":"","action":"scroll","direction":"down","amount":20,"risk":"none"}"#,
        ),
        &back.grounding,
    )
    .await
    .unwrap();
    lane.settle(Duration::from_secs(2)).await;
    let low = lane.observe_tree_only().await.unwrap();
    let far = mark(&low.grounding, Role::Button, "Far below");
    let click = format!(r#"{{"thought":"","action":"click","target":{far},"risk":"none"}}"#);
    lane.act(&decision(&click), &low.grounding).await.unwrap();
    let pressed = lane.observe_tree_only().await.unwrap();
    mark(&pressed.grounding, Role::Button, "Pressed");

    // The page's text is readable (for `read_page`), field values are not.
    assert!(lane.reads_pages());
    let (text, total) = lane.page_text(10_000).await.unwrap();
    assert!(text.contains("Contact"), "{text}");
    assert_eq!(total, text.chars().count());
    let (short, _) = lane.page_text(5).await.unwrap();
    assert_eq!(short.chars().count(), 5);

    // The address policy holds whatever the model asks for.
    for bad in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "chrome://settings",
    ] {
        assert!(lane.open_url(bad).await.is_err(), "{bad} must be refused");
    }
    let still = lane.observe_tree_only().await.unwrap();
    assert!(still.url.unwrap().starts_with("http://127.0.0.1"));
}

/// Observe timings, printed for docs/BROWSER.md: the small form and a page
/// with 150 controls.
#[tokio::test(flavor = "multi_thread")]
async fn observe_timings() {
    let Some((lane, _profile)) = lane().await else {
        return;
    };
    let base = serve().await;
    let p = |v: &mut Vec<f64>, q: f64| {
        v.sort_by(f64::total_cmp);
        v[((v.len() - 1) as f64 * q).round() as usize]
    };
    for page in ["/", "/big"] {
        lane.open_url(&format!("{base}{page}")).await.unwrap();
        let (mut full, mut tree) = (Vec::new(), Vec::new());
        let mut marks = 0;
        for _ in 0..20 {
            let t = Instant::now();
            lane.observe().await.unwrap();
            full.push(t.elapsed().as_secs_f64() * 1000.0);
            let t = Instant::now();
            marks = lane
                .observe_tree_only()
                .await
                .unwrap()
                .grounding
                .mark_count();
            tree.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        eprintln!(
            "browser lane observe {page} ({marks} marks): full p50 {:.0} ms p95 {:.0} ms; tree-only p50 {:.0} ms p95 {:.0} ms",
            p(&mut full, 0.5),
            p(&mut full, 0.95),
            p(&mut tree, 0.5),
            p(&mut tree, 0.95)
        );
    }
}

struct Yes;
#[async_trait::async_trait]
impl kai_core::ports::Approver for Yes {
    async fn approve(&self, _: &Decision, _: &Grounding) -> kai_core::ports::ApprovalChoice {
        kai_core::ports::ApprovalChoice::Approve
    }
}

/// The unchanged engine drives the lane: open, type, submit, done.
#[tokio::test(flavor = "multi_thread")]
async fn engine_runs_a_web_task_through_the_lane() {
    let Some((lane, _profile)) = lane().await else {
        return;
    };
    let base = serve().await;
    // Marks on the fixture page in reading order: 1 Name, 2 Password, 3 Send, 4 About.
    let client = MockClient::new([
        format!(r#"{{"thought":"open the form","action":"open_url","text":"{base}/","risk":"none","expect":"contact form"}}"#),
        r#"{"thought":"type my name","action":"type","target":1,"text":"Ada","risk":"low","expect":"name typed"}"#.to_string(),
        r#"{"thought":"send it","action":"click","target":3,"risk":"high","expect":"thanks page"}"#.to_string(),
        r#"{"thought":"sent","action":"done","risk":"none"}"#.to_string(),
    ]);
    let engine = Engine {
        grounder: None,
        client: &client,
        careful: None,
        perceptor: &lane,
        actuator: &lane,
        approver: &Yes,
        interrupt: &kai_core::ports::NeverStops,
        sink: &kai_core::ports::NullSink,
        steering: &NoSteering,
        verifier: None,
        human: None,
        skills: &[],
        config: kai_core::ports::Config {
            approval: ApprovalLevel::HighRiskOnly,
            learn: false,
            ..Default::default()
        },
    };
    let trace = engine.run("send the contact form as Ada").await;
    assert_eq!(trace.outcome, Some(Outcome::Success), "{trace:#?}");
    let now = lane.observe_tree_only().await.unwrap();
    assert!(now.url.unwrap().contains("/thanks?n=Ada"));
    assert!(
        trace
            .steps
            .iter()
            .any(|s| s.decision.action == Action::Click)
    );
}

/// The same page through both of Kai's eyes: the desktop path (screen
/// capture + AT-SPI tree of a headed browser) and the browser lane. Needs a
/// display and the accessibility bus; run with `--ignored` (docs/BROWSER.md).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a display, the AT-SPI bus and a headed browser"]
async fn desktop_vs_lane_on_one_page() {
    let Some(browser) = find_browser() else {
        return;
    };
    let base = serve().await;
    let profile = tempfile::tempdir().unwrap();
    let opts = LaneOptions {
        browser,
        profile: profile.path().to_path_buf(),
        headless: false,
        size: (1280, 800),
        downloads: None,
    };
    let lane = BrowserLane::launch(&opts, None).await.unwrap();
    lane.open_url(&format!("{base}/")).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let desktop = kai_os::DesktopPerceptor::new();
    let stats = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        (
            v[v.len() / 2],
            v[((v.len() - 1) as f64 * 0.95).round() as usize],
        )
    };
    let mut rows = Vec::new();
    for (label, tree_only) in [("full", false), ("tree-only", true)] {
        let (mut d, mut l) = (Vec::new(), Vec::new());
        let (mut d_marks, mut l_marks) = (0, 0);
        for _ in 0..20 {
            let t = Instant::now();
            let o = if tree_only {
                desktop.observe_tree_only().await.unwrap()
            } else {
                desktop.observe().await.unwrap()
            };
            d.push(t.elapsed().as_secs_f64() * 1000.0);
            d_marks = o.grounding.mark_count();
            let t = Instant::now();
            let o = if tree_only {
                lane.observe_tree_only().await.unwrap()
            } else {
                lane.observe().await.unwrap()
            };
            l.push(t.elapsed().as_secs_f64() * 1000.0);
            l_marks = o.grounding.mark_count();
        }
        rows.push((label, stats(d), d_marks, stats(l), l_marks));
    }
    for (label, (dp50, dp95), dm, (lp50, lp95), lm) in rows {
        eprintln!(
            "{label}: desktop p50 {dp50:.0} ms p95 {dp95:.0} ms ({dm} marks) | lane p50 {lp50:.0} ms p95 {lp95:.0} ms ({lm} marks)"
        );
    }
    eprintln!(
        "desktop list:\n{}",
        desktop
            .observe_tree_only()
            .await
            .unwrap()
            .grounding
            .as_text()
    );
    eprintln!(
        "lane list:\n{}",
        lane.observe_tree_only().await.unwrap().grounding.as_text()
    );
}
