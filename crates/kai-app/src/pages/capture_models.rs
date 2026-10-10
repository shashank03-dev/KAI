//! Screenshot fixtures for local models: the Models page ranked, mid-download
//! and ready, and onboarding's "Run on this computer" choice. Made-up
//! hardware; the catalog is whatever `local_api` provides. Settings are only
//! changed in memory, never saved.

use crate::app::{KaiApp, Page};
use crate::local::{Job, LocalState, Step};
use crate::local_api::{HardwareSummary, LocalServer, Progress};
use gpui::{AppContext, Context, Window};
use kai_core::settings::BackendChoice;

pub const SHOTS: &[&str] = &[
    "06-onboarding-local",
    "90-models-ranked",
    "91-models-downloading",
    "92-models-ready",
];

fn hardware() -> HardwareSummary {
    HardwareSummary {
        ram_gb: 16.0,
        cpu_threads: 12,
        gpu_vram_gb: Some(8.0),
        gpu_name: Some("NVIDIA GeForce RTX 4060".into()),
        os: "linux".into(),
        arch: "x86_64".into(),
    }
}

fn best(l: &LocalState) -> String {
    l.ranked
        .first()
        .map(|r| r.entry.id.clone())
        .unwrap_or_default()
}

/// Back to the plain ranked list, and off the local backend.
pub fn reset(app: &mut KaiApp, cx: &mut Context<KaiApp>) {
    if app.settings.backend == BackendChoice::Local {
        app.settings.backend = BackendChoice::Both;
        app.settings.local_model.clear();
    }
    app.local.update(cx, |l, cx| {
        l.set_hardware(hardware());
        l.installed.clear();
        l.server = None;
        l.job = None;
        l.failure = None;
        l.ready = None;
        cx.notify();
    });
}

/// Stage `name` if it is one of ours; false otherwise.
pub fn stage(app: &mut KaiApp, name: &str, window: &mut Window, cx: &mut Context<KaiApp>) -> bool {
    reset(app, cx);
    if !SHOTS.contains(&name) {
        return false;
    }
    if name == "06-onboarding-local" {
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
            o.update(cx, |o, cx| o.show_local(cx));
        }
        return true;
    }
    app.onboarding = None;
    let other = "qwen2.5-vl-7b-q4".to_string();
    let mut use_it = None;
    app.local.update(cx, |l, cx| {
        let id = best(l);
        match name {
            "91-models-downloading" => {
                l.installed.push(other.clone());
                l.job = Some(Job::fixture(
                    &id,
                    vec![Step::Install, Step::Start, Step::Use],
                    Progress {
                        stage: "model".into(),
                        bytes: 1_930_000_000,
                        total: Some(5_000_000_000),
                        message: String::new(),
                    },
                ));
            }
            "92-models-ready" => {
                l.installed = vec![id.clone(), other.clone()];
                l.server = Some((
                    id.clone(),
                    LocalServer {
                        base_url: "http://127.0.0.1:8098/v1".into(),
                        pid: 4242,
                    },
                ));
                l.ready = Some(id.clone());
                use_it = Some(id);
            }
            _ => {}
        }
        cx.notify();
    });
    if let Some(id) = use_it {
        app.settings.backend = BackendChoice::Local;
        app.settings.local_model = id;
        app.conn = crate::app::Conn::Online("This computer (llama.cpp)".into());
    }
    app.go(Page::Models, cx);
    true
}
