//! `KAI_CAPTURE=<dir> kai-app` walks every screen and state, in light and then
//! dark, and saves a PNG of each into `<dir>`, then quits. Runs use made-up
//! data so no model or desktop control is needed; it is for reviewing the UI.

use crate::agent::{Pending, Ui};
use crate::app::{Conn, KaiApp, Page};
use crate::state::RunState;
use ely_gpui_component::theme::{Mode, Theme};
use gpui::{Context, Window};
use kai_core::engine::EngineEvent;
use kai_core::trace::{Outcome, StepRecord, Timings, Trace};
use kai_core::{Decision, Rect};
use std::path::PathBuf;
use std::time::Duration;

const SHOTS: &[&str] = &[
    "01-onboarding-welcome", "02-onboarding-connect", "03-onboarding-access", "04-onboarding-safety", "05-onboarding-try",
    "10-home", "11-home-offline", "20-running", "21-approval", "30-done-success", "31-done-failure",
    "40-skills", "41-skill-detail", "42-save-as-skill", "50-history", "60-bench",
    "70-settings-model", "71-settings-safety", "72-settings-privacy", "80-palette", "81-shortcuts",
];

pub fn start(dir: PathBuf, window: &mut Window, cx: &mut Context<KaiApp>) {
    let _ = std::fs::create_dir_all(&dir);
    cx.spawn_in(window, async move |this, cx| {
        for mode in [Mode::Light, Mode::Dark] {
            for (i, name) in SHOTS.iter().enumerate() {
                let ok = this.update_in(cx, |app, window, cx| {
                    Theme::set_mode_now(mode, cx);
                    stage(app, i, window, cx);
                    cx.notify();
                });
                if ok.is_err() {
                    return;
                }
                cx.background_executor().timer(Duration::from_millis(900)).await;
                let suffix = if mode == Mode::Dark { "dark" } else { "light" };
                let path = dir.join(format!("{name}-{suffix}.png"));
                let bounds = this.update_in(cx, |_, window, _| {
                    let b = window.bounds();
                    let s = window.scale_factor();
                    (f32::from(b.origin.x) * s, f32::from(b.origin.y) * s, f32::from(b.size.width) * s, f32::from(b.size.height) * s)
                });
                if let Ok(b) = bounds {
                    cx.background_executor().spawn(async move { shoot(&path, b) }).await;
                }
            }
        }
        let _ = cx.update(|_, cx| cx.quit());
    })
    .detach();
}

fn shoot(path: &std::path::Path, (x, y, w, h): (f32, f32, f32, f32)) {
    let r = (|| -> anyhow::Result<()> {
        let m = xcap::Monitor::all()?.into_iter().next().ok_or_else(|| anyhow::anyhow!("no display"))?;
        let img = m.capture_image()?;
        let (x, y) = (x.max(0.0) as u32, y.max(0.0) as u32);
        let w = (w as u32).min(img.width().saturating_sub(x));
        let h = (h as u32).min(img.height().saturating_sub(y));
        image::imageops::crop_imm(&img, x, y, w, h).to_image().save(path)?;
        Ok(())
    })();
    if let Err(e) = r {
        log::error!("capture: {}: {e:#}", path.display());
    }
}

fn decision(json: &str) -> Decision {
    Decision::parse_checked(json).expect("capture fixtures are valid")
}

fn sample_frame() -> Option<PathBuf> {
    ["docs/img/set-of-marks-gtk.jpg", "../../docs/img/set-of-marks-gtk.jpg"].iter().map(PathBuf::from).find(|p| p.is_file()).and_then(|p| p.canonicalize().ok())
}

fn fake_run(r: &mut RunState, done: Option<Outcome>) {
    r.start("Open bmsce.ac.in and find the IEEE CS page".into(), false, 40);
    let screen = Some(Rect::new(0.0, 0.0, 1280.0, 720.0));
    if let Some(f) = sample_frame() {
        for step in 0..3 {
            r.apply(Ui::Frame { step, path: f.clone() });
        }
    }
    let steps = [
        (r#"{"thought":"Go straight to the college site","action":"open_url","text":"https://bmsce.ac.in","risk":"none"}"#, None),
        (r#"{"thought":"The Clubs menu should list IEEE","action":"click","target":3,"risk":"none"}"#, Some(Rect::new(480.0, 10.0, 80.0, 24.0))),
        (r#"{"thought":"IEEE CS appears under Technical clubs","action":"click","target":42,"risk":"low"}"#, Some(Rect::new(40.0, 180.0, 200.0, 30.0))),
    ];
    let mut trace = Trace::new(&r.task);
    for (i, (json, target)) in steps.iter().enumerate() {
        let d = decision(json);
        r.apply(Ui::Event(EngineEvent::Observed { step: i as u32, elements: String::new(), screenshot: std::sync::Arc::from(&[][..]), screen }));
        r.apply(Ui::Event(EngineEvent::Decided { step: i as u32, decision: Box::new(d.clone()), target: *target }));
        let tm = Timings { capture_ms: 21, tree_ms: 150, encode_ms: 44, model_ms: 610 + 40 * i as u64, first_token_ms: None, act_ms: 60 };
        if i < 2 || done.is_some() {
            r.apply(Ui::Event(EngineEvent::Acted { step: i as u32, timings: tm }));
        }
        trace.push(StepRecord { index: i as u32, decision: d, timings: tm, backend: "DigitalOcean (Gemma 4)".into(), elements: String::new(), screenshot: None, approval: None, verified: true, output: None });
    }
    r.thinking = done.is_none();
    r.elapsed = Duration::from_millis(4_200);
    if let Some(o) = done {
        trace.finish(o);
        r.apply(Ui::Finished(Box::new(trace)));
        r.elapsed = Duration::from_millis(4_200);
    }
}

fn stage(app: &mut KaiApp, i: usize, _: &mut Window, cx: &mut Context<KaiApp>) {
    app.palette = false;
    app.shortcuts = false;
    app.pages.open_skill = None;
    app.pages.drafting = false;
    app.conn = Conn::Online("DigitalOcean (Gemma 4) · 640 ms".into());
    if i < 5 {
        if let Some(o) = &app.onboarding {
            o.update(cx, |o, cx| o.set_step(i, cx));
        }
        return;
    }
    if app.onboarding.is_some() {
        app.onboarding = None;
    }
    let run = app.run.clone();
    match SHOTS[i] {
        "10-home" | "11-home-offline" => {
            run.update(cx, |r, _| *r = RunState::default());
            if SHOTS[i] == "11-home-offline" {
                app.conn = Conn::Offline("Kai couldn't reach the model server. Check the address and your internet connection.".into());
            }
            app.go(Page::Run, cx);
        }
        "20-running" => {
            run.update(cx, |r, _| fake_run(r, None));
            app.go(Page::Run, cx);
        }
        "21-approval" => {
            run.update(cx, |r, _| {
                fake_run(r, None);
                // The receiver is dropped: answering in a capture does nothing.
                let (reply, _) = futures::channel::oneshot::channel();
                r.apply(Ui::Approval(Pending {
                    decision: decision(r#"{"thought":"Send the drafted email to the professor","action":"click","target":12,"risk":"high"}"#),
                    element: Some("Send".into()),
                    reply,
                }));
            });
            app.go(Page::Run, cx);
        }
        "30-done-success" => run.update(cx, |r, _| fake_run(r, Some(Outcome::Success))),
        "31-done-failure" => run.update(cx, |r, _| fake_run(r, Some(Outcome::StepLimit))),
        "40-skills" => app.go(Page::Skills, cx),
        "41-skill-detail" => {
            app.pages.open_skill = app.skills.first().map(|s| s.front.name.clone());
            app.go(Page::Skills, cx);
        }
        "42-save-as-skill" => {
            run.update(cx, |r, _| fake_run(r, Some(Outcome::Success)));
            crate::pages::skills::open_draft(app, cx);
        }
        "50-history" => {
            app.pages.reload_history();
            app.pages.open_trace = app.pages.history.first().map(|e| e.dir.clone());
            app.go(Page::History, cx);
        }
        "60-bench" => {
            app.pages.bench = crate::pages::bench::load();
            app.go(Page::Bench, cx);
        }
        "70-settings-model" | "71-settings-safety" | "72-settings-privacy" => {
            app.pages.settings_section = match SHOTS[i] { "71-settings-safety" => "safety", "72-settings-privacy" => "privacy", _ => "model" }.into();
            app.go(Page::Settings, cx);
        }
        "80-palette" => {
            app.go(Page::Run, cx);
            run.update(cx, |r, _| *r = RunState::default());
            app.palette = true;
        }
        "81-shortcuts" => app.shortcuts = true,
        _ => {}
    }
}
