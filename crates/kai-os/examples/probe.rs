//! Measure Kai's eyes and hands on the real desktop.
//!
//! ```text
//! cargo run -p kai-os --example probe -- tree [N]           # full vs tree-only observe
//! cargo run -p kai-os --example probe -- frames [OUT_DIR]   # encode size/time per frame size
//! cargo run -p kai-os --example probe -- a11y "<name>" [text]   # accessibility-first action
//! cargo run -p kai-os --example probe -- mouse "<name>" [text]  # the mouse/keyboard path
//! cargo run -p kai-os --example probe -- zoom "<name>" OUT.jpg
//! ```
//! `<name>` picks the first mark whose name contains it (case-insensitive),
//! or `#N` picks mark N;
//! with `text` the action is `type`, otherwise `click`.

use kai_core::{Actuator, Decision, Grounding, Perceptor};
use kai_os::frame::{self, FrameConfig};
use std::time::{Duration, Instant};

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn marks(g: &Grounding) -> usize {
    match g {
        Grounding::Marks(m) => m.len(),
        Grounding::Grid { .. } => 0,
    }
}

async fn tree(n: usize) -> anyhow::Result<()> {
    let p = kai_os::DesktopPerceptor::new();
    let (mut full, mut only, mut tree_ms) = (Vec::new(), Vec::new(), Vec::new());
    let mut count = 0;
    let raw = kai_os::a11y::read_tree().await.len();
    for _ in 0..n {
        let t = Instant::now();
        let o = p.observe().await?;
        full.push(t.elapsed().as_secs_f64() * 1e3);
        tree_ms.push(o.timing.tree_ms as f64);
        let t = Instant::now();
        let o = p.observe_tree_only().await?;
        only.push(t.elapsed().as_secs_f64() * 1e3);
        assert!(o.images.screenshot_jpeg.is_empty() || marks(&o.grounding) == 0);
        count = marks(&o.grounding);
    }
    println!(
        "raw nodes {raw} · marks {count} · tree read {:.0} ms · full observe {:.0} ms · tree-only {:.0} ms (median of {n})",
        median(tree_ms),
        median(full),
        median(only)
    );
    Ok(())
}

/// Frame sizes as the model gets them: downscaled, marks drawn, JPEG.
/// Assumes the primary display sits at (0,0), as on Xvfb.
async fn frames(out: Option<String>) -> anyhow::Result<()> {
    let m = xcap::Monitor::all()?.into_iter().next().unwrap();
    let img = m.capture_image()?;
    let (w, h) = img.dimensions();
    let screen = kai_core::Rect::new(0.0, 0.0, w as f32, h as f32);
    let grounding = kai_os::a11y::ground(&kai_os::a11y::read_tree().await, screen);
    println!("source {w}x{h} · {} marks", marks(&grounding));
    let budgets = [
        ("side", 1280, 0),
        ("side", 1056, 0),
        ("side", 1024, 0),
        ("side", 896, 0),
        ("side", 768, 0),
    ];
    let tokens = [
        ("280 tok", 4096, 280 * 48 * 48),
        ("560 tok", 4096, 560 * 48 * 48),
    ];
    for (label, side, px) in budgets.iter().chain(tokens.iter()).copied() {
        for q in [75u8, 60] {
            let cfg = FrameConfig {
                max_side: side,
                max_pixels: px,
                quality: q,
            };
            let mut ts = Vec::new();
            let mut ds = Vec::new();
            let mut bytes = Vec::new();
            for _ in 0..7 {
                let t = Instant::now();
                let f = frame::downscale_with(&img, &cfg);
                ds.push(t.elapsed().as_secs_f64() * 1e3);
                let mut marked = f.image;
                kai_os::marks::draw(&mut marked, &grounding, f.scale);
                let t = Instant::now();
                bytes = frame::encode_jpeg_q(&marked, q);
                ts.push(t.elapsed().as_secs_f64() * 1e3);
            }
            let (fw, fh) = cfg.target_size(w, h);
            println!(
                "{label:>8} {side:>4} q{q}: {fw}x{fh} ({:.0} soft tokens at 48px) · {} KB · resize {:.1} ms · encode {:.1} ms",
                (fw * fh) as f64 / 2304.0,
                bytes.len() / 1024,
                median(ds),
                median(ts)
            );
            if let Some(dir) = &out {
                std::fs::write(format!("{dir}/frame_{fw}x{fh}_q{q}.jpg"), &bytes)?;
            }
        }
    }
    Ok(())
}

async fn find(p: &kai_os::DesktopPerceptor, name: &str) -> anyhow::Result<(Grounding, u32)> {
    let o = p.observe().await?;
    let needle = name.to_lowercase();
    let by_number = name.strip_prefix('#').and_then(|n| n.parse::<u32>().ok());
    let mark = match &o.grounding {
        Grounding::Marks(_) if by_number.is_some() => by_number,
        Grounding::Marks(els) => els
            .iter()
            .find(|e| e.name.to_lowercase().contains(&needle))
            .map(|e| e.mark),
        _ => None,
    }
    .ok_or_else(|| anyhow::anyhow!("no mark named {name:?}:\n{}", o.grounding.as_text()))?;
    Ok((o.grounding, mark))
}

fn decision(mark: u32, text: Option<&str>) -> Decision {
    let json = match text {
        Some(t) => {
            serde_json::json!({"thought":"","action":"type","target":mark,"text":t,"risk":"none"})
        }
        None => serde_json::json!({"thought":"","action":"click","target":mark,"risk":"none"}),
    };
    Decision::parse_checked(&json.to_string()).expect("valid decision")
}

async fn act(a11y: bool, name: &str, text: Option<&str>) -> anyhow::Result<()> {
    let p = kai_os::DesktopPerceptor::new();
    let (g, mark) = find(&p, name).await?;
    let d = decision(mark, text);
    println!(
        "target #{mark}: {}",
        g.element(mark).map(|e| e.name.as_str()).unwrap_or("")
    );
    let hands = kai_os::DesktopActuator::new(kai_os::KillSwitch::default())?;
    // Optional: change the screen between observing and acting (e.g. raise
    // another window over the target) to test acting on a covered window.
    if let Ok(cmd) = std::env::var("PROBE_BEFORE_ACT") {
        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .status()?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let t = Instant::now();
    let done = if a11y {
        hands.a11y_act(&d, &g).await?
    } else {
        hands.act(&d, &g).await?;
        true
    };
    let call = t.elapsed();
    p.settle(Duration::from_secs(2)).await;
    println!(
        "{} done={done} · call {:.1} ms · call+settle {:.0} ms",
        if a11y { "a11y" } else { "mouse" },
        call.as_secs_f64() * 1e3,
        t.elapsed().as_secs_f64() * 1e3
    );
    Ok(())
}

async fn zoom(name: &str, out: &str) -> anyhow::Result<()> {
    let p = kai_os::DesktopPerceptor::new();
    let (g, mark) = find(&p, name).await?;
    let r = g.target_rect(mark).unwrap();
    let t = Instant::now();
    let jpg = p.zoom(r).await.ok_or_else(|| anyhow::anyhow!("no zoom"))?;
    println!(
        "zoom {:.0} ms · {} KB",
        t.elapsed().as_secs_f64() * 1e3,
        jpg.len() / 1024
    );
    std::fs::write(out, jpg)?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    match arg(0) {
        Some("tree") => tree(arg(1).and_then(|s| s.parse().ok()).unwrap_or(5)).await,
        Some("frames") => frames(arg(1).map(String::from)).await,
        Some("a11y") => act(true, arg(1).unwrap_or(""), arg(2)).await,
        Some("mouse") => act(false, arg(1).unwrap_or(""), arg(2)).await,
        Some("zoom") => zoom(arg(1).unwrap_or(""), arg(2).unwrap_or("zoom.jpg")).await,
        _ => anyhow::bail!("usage: probe tree|frames|a11y|mouse|zoom ..."),
    }
}
