//! `KAI_CAPTURE=<dir> kai-app` walks every screen and state, in light and then
//! dark, and saves a PNG of each into `<dir>`, then quits. Runs use made-up
//! data so no model or desktop control is needed; it is for reviewing the UI.

use crate::agent::{Pending, Ui};
use crate::app::{Conn, KaiApp, Page};
use crate::state::RunState;
use ely_gpui_component::theme::{Mode, Theme};
use gpui::{AppContext, Context, Window};
use kai_core::engine::EngineEvent;
use kai_core::trace::{Outcome, StepRecord, Timings, Trace};
use kai_core::{Decision, Rect};
use std::path::PathBuf;
use std::time::Duration;

const SHOTS: &[&str] = &[
    "01-onboarding-welcome",
    "02-onboarding-connect",
    "03-onboarding-access",
    "04-onboarding-safety",
    "05-onboarding-try",
    "10-home",
    "11-home-offline",
    "20-running",
    "21-approval",
    "22-run-terminal",
    "23-run-team",
    "24-run-steered",
    "25-run-queued",
    "30-done-success",
    "31-done-failure",
    "40-skills",
    "41-skill-detail",
    "42-save-as-skill",
    "50-history",
    "60-bench",
    "70-settings-model",
    "71-settings-safety",
    "72-settings-privacy",
    "80-palette",
    "81-shortcuts",
];

/// Every shot, or those whose names contain one of the comma-separated
/// words in `KAI_CAPTURE_ONLY` (e.g. `models,onboarding-local`).
fn shots() -> Vec<&'static str> {
    let only = std::env::var("KAI_CAPTURE_ONLY").unwrap_or_default();
    let words: Vec<&str> = only.split(',').filter(|w| !w.is_empty()).collect();
    SHOTS
        .iter()
        .chain(super::capture_models::SHOTS)
        .copied()
        .filter(|s| words.is_empty() || words.iter().any(|w| s.contains(w)))
        .collect()
}

pub fn start(dir: PathBuf, window: &mut Window, cx: &mut Context<KaiApp>) {
    let _ = std::fs::create_dir_all(&dir);
    cx.spawn_in(window, async move |this, cx| {
        for mode in [Mode::Light, Mode::Dark] {
            for name in shots() {
                let ok = this.update_in(cx, |app, window, cx| {
                    Theme::set_mode_now(mode, cx);
                    if !super::capture_models::stage(app, name, window, cx) {
                        let i = SHOTS.iter().position(|s| *s == name).unwrap_or(0);
                        stage(app, i, window, cx);
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    return;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(900))
                    .await;
                let suffix = if mode == Mode::Dark { "dark" } else { "light" };
                let path = dir.join(format!("{name}-{suffix}.png"));
                let bounds = this.update_in(cx, |_, window, _| {
                    let b = window.bounds();
                    let s = window.scale_factor();
                    (
                        f32::from(b.origin.x) * s,
                        f32::from(b.origin.y) * s,
                        f32::from(b.size.width) * s,
                        f32::from(b.size.height) * s,
                    )
                });
                if let Ok(b) = bounds {
                    cx.background_executor()
                        .spawn(async move { shoot(&path, b) })
                        .await;
                }
            }
        }
        let _ = cx.update(|_, cx| cx.quit());
    })
    .detach();
}

fn shoot(path: &std::path::Path, (x, y, w, h): (f32, f32, f32, f32)) {
    let r = (|| -> anyhow::Result<()> {
        let m = xcap::Monitor::all()?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no display"))?;
        let img = m.capture_image()?;
        let (x, y) = (x.max(0.0) as u32, y.max(0.0) as u32);
        let w = (w as u32).min(img.width().saturating_sub(x));
        let h = (h as u32).min(img.height().saturating_sub(y));
        image::imageops::crop_imm(&img, x, y, w, h)
            .to_image()
            .save(path)?;
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
    [
        "docs/img/set-of-marks-gtk.jpg",
        "../../docs/img/set-of-marks-gtk.jpg",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
    .and_then(|p| p.canonicalize().ok())
}

fn fake_run(r: &mut RunState, done: Option<Outcome>) {
    r.start(
        "Open bmsce.ac.in and find the IEEE CS page".into(),
        false,
        40,
    );
    r.model = "Gemma 4 26B A4B · DigitalOcean + Gemini".into();
    // The sample frame's own size, so the viewer shows it without bars.
    let (w, h) = sample_frame()
        .and_then(|f| image::image_dimensions(f).ok())
        .unwrap_or((1280, 720));
    let screen = Some(Rect::new(0.0, 0.0, w as f32, h as f32));
    if let Some(f) = sample_frame() {
        for step in 0..3 {
            r.apply(Ui::Frame {
                step,
                path: f.clone(),
            });
        }
    }
    let steps = [
        (
            r#"{"thought":"Go straight to the college site","action":"open_url","text":"https://bmsce.ac.in","risk":"none"}"#,
            None,
        ),
        (
            r#"{"thought":"The Clubs menu should list IEEE","action":"click","target":3,"risk":"none"}"#,
            Some(Rect::new(480.0, 10.0, 80.0, 24.0)),
        ),
        (
            r#"{"thought":"IEEE CS appears under Technical clubs","action":"click","target":42,"risk":"low"}"#,
            Some(Rect::new(40.0, 180.0, 200.0, 30.0)),
        ),
    ];
    let mut trace = Trace::new(&r.task);
    for (i, (json, target)) in steps.iter().enumerate() {
        let d = decision(json);
        r.apply(Ui::Event(EngineEvent::Observed {
            step: i as u32,
            elements: String::new(),
            screenshot: std::sync::Arc::from(&[][..]),
            screen,
        }));
        r.apply(Ui::Event(EngineEvent::Decided {
            step: i as u32,
            decision: Box::new(d.clone()),
            target: *target,
        }));
        let tm = Timings {
            capture_ms: 21,
            tree_ms: 150,
            encode_ms: 44,
            model_ms: 610 + 40 * i as u64,
            first_token_ms: None,
            act_ms: 60,
            verify_ms: 0,
        };
        if i < 2 || done.is_some() {
            r.apply(Ui::Event(EngineEvent::Acted {
                step: i as u32,
                timings: tm,
            }));
        }
        trace.push(StepRecord {
            index: i as u32,
            decision: d,
            timings: tm,
            backend: "DigitalOcean (Gemma 4)".into(),
            elements: String::new(),
            screenshot: None,
            approval: None,
            verified: true,
            output: None,
            path: None,
            call: Default::default(),
            executor: None,
        });
    }
    r.thinking = done.is_none();
    r.elapsed = Duration::from_millis(4_200);
    if let Some(o) = done {
        trace.finish(o);
        r.apply(Ui::Finished(Box::new(trace)));
        r.elapsed = Duration::from_millis(4_200);
    }
}

/// Replace the sessions list with made-up runs: one finished, the given one
/// (selected, and active when live), and one queued behind it.
fn fake_runs(app: &mut KaiApp, cx: &mut Context<KaiApp>, f: impl FnOnce(&mut RunState)) {
    app.runs.clear();
    app.active = None;
    app.queue_held = false;
    let kill = cx.global::<crate::AgentHandle>().0.kill.clone();
    let add = |app: &mut KaiApp, cx: &mut Context<KaiApp>, f: &mut dyn FnMut(&mut RunState)| {
        let state = cx.new(|_| RunState::default());
        state.update(cx, |r, _| f(r));
        app.next_id += 1;
        let id = app.next_id;
        app.runs.push(crate::runs::RunEntry {
            id,
            state,
            control: std::sync::Arc::new(crate::control::RunControl::new(kill.clone())),
            stopped_alone: false,
        });
        id
    };
    add(app, cx, &mut |r| {
        fake_run(r, Some(Outcome::Success));
        r.task = "Find the 5 biggest files in my Downloads folder".into();
    });
    let mut f = Some(f);
    let id = add(app, cx, &mut |r| {
        if let Some(f) = f.take() {
            f(r)
        }
    });
    add(app, cx, &mut |r| {
        r.queue(
            "Switch my computer to dark mode".into(),
            false,
            40,
            "Gemma 4 26B A4B · DigitalOcean + Gemini".into(),
        )
    });
    if app.entry(id).is_some_and(|e| e.state.read(cx).is_live()) {
        app.active = Some(id);
    }
    app.select(Some(id), cx);
}

fn add_outputs(r: &mut RunState) {
    for (step, command, output) in [
        (
            1,
            "du -ah ~/Downloads | sort -rh | head -5",
            "1.2G\t/home/kai/Downloads\n700M\t/home/kai/Downloads/ubuntu.iso\n[exit 0]",
        ),
        (
            2,
            "ls ~/Downloads/missing",
            "ls: cannot access '/home/kai/Downloads/missing': No such file or directory\n[exit 2]",
        ),
    ] {
        r.apply(Ui::Event(EngineEvent::Output {
            step,
            command: command.into(),
            output: output.into(),
        }));
    }
}

fn stage(app: &mut KaiApp, i: usize, window: &mut Window, cx: &mut Context<KaiApp>) {
    app.palette = false;
    app.run_tab = "steps".into();
    app.shortcuts = false;
    app.pages.open_skill = None;
    app.pages.drafting = false;
    app.conn = Conn::Online("DigitalOcean (Gemma 4) · 640 ms".into());
    if i < 5 {
        // The dark pass needs onboarding again after the light pass closed it.
        if app.onboarding.is_none() {
            let (settings, me, local) = (
                app.settings.clone(),
                cx.entity().downgrade(),
                app.local.clone(),
            );
            app.onboarding = Some(
                cx.new(|cx| crate::onboarding::Onboarding::new(settings, me, local, window, cx)),
            );
        }
        if let Some(o) = &app.onboarding {
            o.update(cx, |o, cx| o.set_step(i, cx));
        }
        return;
    }
    if app.onboarding.is_some() {
        app.onboarding = None;
    }
    match SHOTS[i] {
        "10-home" | "11-home-offline" => {
            app.runs.clear();
            app.active = None;
            app.select(None, cx);
            if SHOTS[i] == "11-home-offline" {
                app.conn = Conn::Offline("Kai couldn't reach the model server. Check the address and your internet connection.".into());
            }
            app.go(Page::Run, cx);
        }
        "20-running" => fake_runs(app, cx, |r| fake_run(r, None)),
        "22-run-terminal" => {
            fake_runs(app, cx, |r| {
                fake_run(r, None);
                add_outputs(r);
            });
            app.run_tab = "terminal".into();
        }
        "23-run-team" => {
            fake_runs(app, cx, |r| {
                fake_run(r, None);
                r.team = true;
                let plan = kai_core::orchestrator::Plan::parse(
                    r#"{"subtasks":[
                        {"id":"1","goal":"Check the firewall status","kind":"shell"},
                        {"id":"2","goal":"List pending updates","kind":"shell"},
                        {"id":"3","goal":"Open Settings and check screen lock","kind":"gui"},
                        {"id":"4","goal":"Write the summary","kind":"shell","depends_on":["1","2"]}]}"#,
                )
                .expect("capture plan is valid");
                r.apply(Ui::Plan(plan));
                r.apply(Ui::Event(EngineEvent::Finished {
                    outcome: Outcome::Success,
                }));
            });
            app.run_tab = "team".into();
        }
        "24-run-steered" => fake_runs(app, cx, |r| {
            fake_run(r, None);
            r.sent("Use Firefox, not Chrome".into());
            r.apply(Ui::Event(EngineEvent::Steered {
                step: 2,
                message: "Use Firefox, not Chrome".into(),
            }));
            r.sent("Then close the tab".into());
            r.paused = true;
            r.held = true;
        }),
        "25-run-queued" => {
            fake_runs(app, cx, |r| fake_run(r, None));
            let queued = app.runs.last().map(|e| e.id);
            app.select(queued, cx);
        }
        "21-approval" => {
            fake_runs(app, cx, |r| {
                fake_run(r, None);
                // The receiver is dropped: answering in a capture does nothing.
                let (reply, _) = futures::channel::oneshot::channel();
                r.apply(Ui::Approval(Pending {
                    decision: decision(r#"{"thought":"Send the drafted email to the professor","action":"click","target":12,"risk":"high"}"#),
                    element: Some("Send".into()),
                    reply,
                }));
            });
        }
        "30-done-success" => fake_runs(app, cx, |r| {
            fake_run(r, Some(Outcome::Success));
            add_outputs(r);
        }),
        "31-done-failure" => fake_runs(app, cx, |r| fake_run(r, Some(Outcome::StepLimit))),
        "40-skills" => app.go(Page::Skills, cx),
        "41-skill-detail" => {
            app.pages.open_skill = app.skills.first().map(|s| s.front.name.clone());
            app.go(Page::Skills, cx);
        }
        "42-save-as-skill" => {
            fake_runs(app, cx, |r| fake_run(r, Some(Outcome::Success)));
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
            app.pages.settings_section = match SHOTS[i] {
                "71-settings-safety" => "safety",
                "72-settings-privacy" => "privacy",
                _ => "model",
            }
            .into();
            app.go(Page::Settings, cx);
        }
        "80-palette" => {
            app.runs.clear();
            app.active = None;
            app.select(None, cx);
            app.palette = true;
        }
        "81-shortcuts" => app.shortcuts = true,
        _ => {}
    }
}
