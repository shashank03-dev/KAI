//! The driver regression matrix (`docs/DRIVER_MATRIX.md`): for each toolkit
//! fixture in `bench/driver/` (GTK 3, Qt 5, a web page in Firefox), press a
//! button, toggle a checkbox and fill an empty field through Kai's
//! accessibility path, and check two things for each:
//!
//! 1. the path Kai took is the expected one (`a11y::act` returned `true`,
//!    not "fall back to the mouse"), so a driver change cannot silently
//!    drop to the pointer;
//! 2. the widget really changed, read from the fixture's window title (an
//!    X property the fixture sets itself), not from the accessibility tree
//!    under test.
//!
//! It needs a desktop (X11 + the AT-SPI bus, `scripts/desktop/start.sh`),
//! `xdotool`, and the fixtures' toolkits, so it is ignored by default:
//!
//! ```sh
//! source scripts/desktop/start.sh
//! KAI_FIXTURE_PYTHON=python3.12 cargo test -p kai-os --test driver_matrix -- --ignored --nocapture
//! ```
//!
//! A toolkit whose fixture cannot start is reported as skipped, not passed.
//! `KAI_MATRIX_TOOLKITS=gtk3,qt5` runs a subset.

use kai_core::{Decision, Element, Grounding, Rect, Role};
use kai_os::a11y;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// What Kai should do for one widget on one toolkit.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Path {
    /// Through the accessibility interface.
    A11y,
    /// A known toolkit limit: Kai must decline and use the mouse.
    Mouse,
}

struct Toolkit {
    name: &'static str,
    /// Expected path for press, toggle and fill.
    expect: [Path; 3],
    /// A known gap: reported, not counted as a pass or a failure, and
    /// flagged when it starts working so the matrix gets updated.
    known_gap: Option<&'static str>,
}

const TOOLKITS: &[Toolkit] = &[
    Toolkit {
        name: "gtk3",
        expect: [Path::A11y, Path::A11y, Path::A11y],
        known_gap: None,
    },
    Toolkit {
        name: "qt5",
        expect: [Path::A11y, Path::A11y, Path::A11y],
        // Measured 2026-10-12, Qt 5.15.13 / at-spi2-core 2.52 (Ubuntu 24.04):
        // the app connects to the accessibility bus but publishes no
        // accessible root, so there is no tree to read (Kai uses the grid).
        known_gap: Some("Qt 5 publishes no accessibility tree here"),
    },
    // Measured 2026-10-12 on Firefox ESR: a page's push buttons expose no
    // AT-SPI action at all (`<button>` with an onclick attribute, a listener
    // or type=submit alike), so Kai presses them with the mouse. Gecko also
    // reports EditableText edits on page inputs as done without applying
    // them, so Kai types those with the keyboard (`linux_act.rs`).
    Toolkit {
        name: "web",
        expect: [Path::Mouse, Path::A11y, Path::Mouse],
        known_gap: None,
    },
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn launch(tk: &str) -> Option<Child> {
    let dir = root().join("bench/driver");
    let mut cmd = match tk {
        "web" => {
            let profile = std::env::temp_dir().join("kai-driver-matrix-firefox");
            std::fs::create_dir_all(&profile).ok()?;
            let mut c = Command::new("firefox");
            c.arg("--new-instance")
                .arg("--profile")
                .arg(profile)
                .arg(format!(
                    "file://{}",
                    dir.join("web.html").canonicalize().ok()?.display()
                ));
            c
        }
        _ => {
            let py = std::env::var("KAI_FIXTURE_PYTHON").unwrap_or_else(|_| "python3".into());
            let mut c = Command::new(py);
            c.arg(dir.join(format!("{tk}.py")));
            c
        }
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// The fixture's window title, read from X (not from AT-SPI).
fn title(tk: &str) -> Option<String> {
    let prefix = format!("kai-fixture {tk}");
    xcap::Window::all()
        .ok()?
        .into_iter()
        .filter_map(|w| w.title().ok())
        .find(|t| t.starts_with(&prefix))
}

fn wait_for(what: impl Fn() -> bool, max: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < max {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn focus(tk: &str) {
    let _ = Command::new("xdotool")
        .args([
            "search",
            "--name",
            &format!("kai-fixture {tk}"),
            "windowactivate",
            "--sync",
        ])
        .status();
    std::thread::sleep(Duration::from_millis(400));
}

fn screen() -> Rect {
    let m = xcap::Monitor::all().expect("a display").remove(0);
    Rect::new(
        m.x().unwrap() as f32,
        m.y().unwrap() as f32,
        m.width().unwrap() as f32,
        m.height().unwrap() as f32,
    )
}

/// Ground the active window and find the widget by role and name.
async fn find(role: Role, name: &str) -> Option<(Grounding, Element)> {
    for _ in 0..20 {
        let g = a11y::ground_live(&a11y::read_tree().await, screen());
        if let Grounding::Marks(els) = &g
            && let Some(e) = els
                .iter()
                .find(|e| e.role == role && e.name.trim().eq_ignore_ascii_case(name))
        {
            let e = e.clone();
            return Some((g, e));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    None
}

fn decision(json: &str) -> Decision {
    Decision::parse_checked(json).expect("valid decision")
}

/// One row: did Kai take the expected path, and did the widget change?
async fn row(tk: &str, role: Role, name: &str, d: &str, want: &str, expect: Path) -> String {
    let Some((g, el)) = find(role, name).await else {
        return format!("{tk:<5} {name:<12} FAIL  not found as {role:?} in the tree");
    };
    let d = decision(&d.replace("{mark}", &el.mark.to_string()));
    let took = match a11y::act(&d, &g).await {
        Ok(true) => Path::A11y,
        Ok(false) => Path::Mouse,
        Err(e) => return format!("{tk:<5} {name:<12} FAIL  a11y error: {e}"),
    };
    if took != expect {
        return format!("{tk:<5} {name:<12} FAIL  took {took:?}, expected {expect:?}");
    }
    if took == Path::Mouse {
        return format!("{tk:<5} {name:<12} ok    declined as expected (mouse)");
    }
    let changed = wait_for(
        || title(tk).is_some_and(|t| t.contains(want)),
        Duration::from_secs(3),
    );
    if changed {
        format!("{tk:<5} {name:<12} ok    a11y, title shows {want}")
    } else {
        format!(
            "{tk:<5} {name:<12} FAIL  a11y said done but the widget did not change ({:?})",
            title(tk)
        )
    }
}

#[tokio::test]
#[ignore = "needs a desktop: see the module docs"]
async fn every_toolkit_takes_the_accessibility_path_and_the_widget_changes() {
    let mut report = Vec::new();
    let mut failed = false;
    // `KAI_MATRIX_TOOLKITS=gtk3,qt5` runs a subset.
    let only = std::env::var("KAI_MATRIX_TOOLKITS").ok();
    for tk in TOOLKITS {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|t| t.trim() == tk.name))
        {
            continue;
        }
        let Some(mut child) = launch(tk.name) else {
            report.push(format!("{:<5} skipped: fixture did not start", tk.name));
            continue;
        };
        if !wait_for(|| title(tk.name).is_some(), Duration::from_secs(20)) {
            report.push(format!("{:<5} skipped: no fixture window", tk.name));
            let _ = child.kill();
            continue;
        }
        focus(tk.name);
        let rows = [
            row(
                tk.name,
                Role::Button,
                "Apply",
                r#"{"thought":"","action":"click","target":{mark},"risk":"none"}"#,
                "clicks=1",
                tk.expect[0],
            )
            .await,
            row(
                tk.name,
                Role::Checkbox,
                "Remember me",
                r#"{"thought":"","action":"click","target":{mark},"risk":"none"}"#,
                "checked=1",
                tk.expect[1],
            )
            .await,
            row(
                tk.name,
                Role::TextField,
                "Name",
                r#"{"thought":"","action":"type","target":{mark},"text":"Asha","risk":"none"}"#,
                "name=Asha",
                tk.expect[2],
            )
            .await,
        ];
        let bad = rows.iter().any(|r| r.contains("FAIL"));
        match tk.known_gap {
            Some(gap) if bad => report.push(format!("{:<5} known gap: {gap}", tk.name)),
            Some(gap) => {
                failed = true;
                report.push(format!(
                    "{:<5} FAIL  known gap \"{gap}\" now works: update the matrix",
                    tk.name
                ));
            }
            None => failed |= bad,
        }
        report.extend(rows);
        let _ = child.kill();
        let _ = child.wait();
    }
    eprintln!("\n{}", report.join("\n"));
    assert!(!failed, "driver matrix:\n{}", report.join("\n"));
}
