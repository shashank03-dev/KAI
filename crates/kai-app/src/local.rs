//! Local open models in the app: what this computer has, which models fit,
//! and one background job at a time (download, start). Jobs run on the
//! agent's tokio runtime and report through a channel the app entity drains,
//! so they keep going while the user switches pages. Nothing downloads or
//! starts without a tap: every job begins in a button handler.

use crate::AgentHandle;
use crate::app::KaiApp;
use crate::local_models::{
    HardwareSummary, LocalManager, LocalServer, ModelEntry, Progress, Ranked, catalog_label,
    detect_hardware, load_catalog, local_root, rank, reload_catalog,
};
use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use futures::future::Either;
use gpui::{Context, Entity};
use kai_core::settings::BackendChoice;
use std::sync::Arc;

/// One part of a job, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Install,
    Start,
    /// Make it Kai's backend: `BackendChoice::Local` with this model.
    Use,
}

pub struct Job {
    pub serial: u64,
    pub id: String,
    pub steps: Vec<Step>,
    pub progress: Progress,
    /// Past the download: waiting for the server to answer.
    pub starting: bool,
    cancel: Option<oneshot::Sender<()>>,
}

impl Job {
    /// A made-up job for the screenshot harness.
    pub fn fixture(id: &str, steps: Vec<Step>, progress: Progress) -> Self {
        Self {
            serial: 0,
            id: id.into(),
            steps,
            progress,
            starting: false,
            cancel: None,
        }
    }
}

/// A job that failed, kept so its card can offer Try again.
pub struct Failure {
    pub id: String,
    pub steps: Vec<Step>,
    pub message: String,
}

pub struct LocalState {
    pub manager: Arc<LocalManager>,
    pub hardware: Option<HardwareSummary>,
    /// The hardware probe ended without an answer; offer Try again.
    pub probe_failed: bool,
    pub ranked: Vec<Ranked>,
    pub installed: Vec<String>,
    /// The running server and the model it serves.
    pub server: Option<(String, LocalServer)>,
    pub job: Option<Job>,
    pub failure: Option<Failure>,
    /// A one-tap flow finished for this model: show "Ready".
    pub ready: Option<String>,
    serial: u64,
}

enum Msg {
    Progress(Progress),
    Starting,
    Started(LocalServer),
    Failed(String),
    Finished,
}

impl LocalState {
    pub fn new(running_model: &str) -> Self {
        let manager = Arc::new(LocalManager::new(local_root()));
        let installed = manager.installed();
        let server = manager.status().map(|s| (running_model.to_string(), s));
        Self {
            manager,
            hardware: None,
            probe_failed: false,
            ranked: Vec::new(),
            installed,
            server,
            job: None,
            failure: None,
            ready: None,
            serial: 0,
        }
    }

    pub fn set_hardware(&mut self, hw: HardwareSummary) {
        self.ranked = rank(&load_catalog(), &hw);
        self.hardware = Some(hw);
    }

    /// Re-read the catalog (a model was pulled) and rank it again.
    pub fn refresh_catalog(&mut self) {
        let catalog = reload_catalog();
        if let Some(hw) = &self.hardware {
            self.ranked = rank(&catalog, hw);
        }
    }

    pub fn entry(&self, id: &str) -> Option<&ModelEntry> {
        self.ranked.iter().map(|r| &r.entry).find(|e| e.id == id)
    }

    /// The catalog entry for `id`, even before the hardware is known.
    pub fn label(&self, id: &str) -> String {
        self.entry(id)
            .map(|e| e.label.clone())
            .or_else(|| catalog_label(id))
            .unwrap_or_else(|| id.to_string())
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.installed.iter().any(|i| i == id)
    }

    pub fn running(&self, id: &str) -> bool {
        self.server.as_ref().is_some_and(|(s, _)| s == id)
    }

    /// What a one-tap "Download and use" still has to do for `id`.
    pub fn plan(&self, id: &str) -> Vec<Step> {
        let mut steps = Vec::new();
        if !self.is_installed(id) {
            steps.push(Step::Install);
        }
        if !self.running(id) {
            steps.push(Step::Start);
        }
        steps.push(Step::Use);
        steps
    }
}

/// "Downloading the model", "Starting the local server"…
pub fn stage_text(p: &Progress, starting: bool) -> String {
    if starting {
        return "Starting the local server…".into();
    }
    let s = match p.stage.as_str() {
        "runtime" => "Downloading the runtime (llama.cpp)",
        "model" => "Downloading the model",
        "verify" => "Checking the download",
        "done" => "Downloaded",
        _ => "Getting ready",
    };
    if p.message.is_empty() {
        format!("{s}…")
    } else {
        format!("{s}: {}", p.message)
    }
}

/// 0..=1 when the total is known.
pub fn fraction(p: &Progress) -> Option<f32> {
    p.total
        .filter(|t| *t > 0)
        .map(|t| (p.bytes as f64 / t as f64).clamp(0.0, 1.0) as f32)
}

pub fn gb(bytes: u64) -> String {
    let g = bytes as f64 / 1e9;
    if g >= 10.0 {
        format!("{g:.0} GB")
    } else if g >= 0.1 {
        format!("{g:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / 1e6)
    }
}

/// "1.2 GB of 5.0 GB · 24%".
pub fn bytes_text(p: &Progress) -> String {
    match (p.total, fraction(p)) {
        (Some(t), Some(f)) => format!("{} of {} · {:.0}%", gb(p.bytes), gb(t), f * 100.0),
        _ if p.bytes > 0 => gb(p.bytes),
        _ => String::new(),
    }
}

impl KaiApp {
    /// Look at RAM, CPU and GPU off the UI thread, then rank the catalog.
    pub fn detect_local(&mut self, cx: &mut Context<Self>) {
        let (tx, rx) = oneshot::channel();
        cx.global::<AgentHandle>()
            .0
            .runtime
            .spawn_blocking(move || {
                let _ = tx.send(detect_hardware());
            });
        let local = self.local.clone();
        local.update(cx, |l, _| l.probe_failed = false);
        cx.spawn(async move |_, cx| {
            let hw = rx.await;
            local.update(cx, |l, cx| {
                match hw {
                    Ok(hw) => l.set_hardware(hw),
                    // The probe panicked: say so instead of spinning forever.
                    Err(_) => l.probe_failed = true,
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// One tap: download what is missing, start the server, use it.
    pub fn local_get_and_use(&mut self, id: &str, cx: &mut Context<Self>) {
        let steps = self.local.read(cx).plan(id);
        self.local_job(id, steps, cx);
    }

    pub fn local_job(&mut self, id: &str, steps: Vec<Step>, cx: &mut Context<Self>) {
        let local = self.local.clone();
        let (entry, manager, stop_first) = {
            let l = local.read(cx);
            if l.job.is_some() {
                return;
            }
            let Some(e) = l.entry(id).cloned() else {
                return;
            };
            (e, l.manager.clone(), l.server.is_some())
        };
        if steps == [Step::Use] {
            self.local_use_now(id, cx);
            return;
        }
        let (tx, mut rx) = mpsc::unbounded();
        let (cancel, cancelled) = oneshot::channel::<()>();
        let serial = local.update(cx, |l, cx| {
            l.serial += 1;
            l.failure = None;
            l.ready = None;
            l.job = Some(Job {
                serial: l.serial,
                id: id.to_string(),
                steps: steps.clone(),
                progress: Progress::default(),
                starting: false,
                cancel: Some(cancel),
            });
            cx.notify();
            l.serial
        });
        let port = self.settings.local_port;
        let rt = cx.global::<AgentHandle>().0.runtime.handle().clone();
        let (install, start) = (steps.contains(&Step::Install), steps.contains(&Step::Start));
        rt.clone().spawn_blocking(move || {
            let work = async {
                if install {
                    let report = |p: Progress| {
                        let _ = tx.unbounded_send(Msg::Progress(p));
                    };
                    manager.install(&entry, &report).await?;
                }
                if start {
                    let _ = tx.unbounded_send(Msg::Starting);
                    if stop_first {
                        manager.stop()?;
                    }
                    let server = manager.start(&entry, port).await?;
                    let _ = tx.unbounded_send(Msg::Started(server));
                }
                anyhow::Ok(())
            };
            // Cancel drops the work (downloads resume from where they were).
            let done = rt.block_on(async {
                match futures::future::select(Box::pin(work), cancelled).await {
                    Either::Left((r, _)) => Some(r),
                    Either::Right(_) => None,
                }
            });
            match done {
                Some(Ok(())) => {
                    let _ = tx.unbounded_send(Msg::Finished);
                }
                Some(Err(e)) => {
                    let _ = tx.unbounded_send(Msg::Failed(format!("{e:#}")));
                }
                None if start => {
                    let _ = manager.stop();
                }
                None => {}
            }
        });
        cx.spawn(async move |this, cx| {
            while let Some(msg) = rx.next().await {
                if this
                    .update(cx, |app, cx| app.local_msg(serial, msg, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn local_msg(&mut self, serial: u64, msg: Msg, cx: &mut Context<Self>) {
        let local = self.local.clone();
        let current = local
            .read(cx)
            .job
            .as_ref()
            .map(|j| (j.serial, j.id.clone()));
        let Some((s, id)) = current.filter(|(s, _)| *s == serial) else {
            return; // a cancelled job's late news
        };
        debug_assert_eq!(s, serial);
        match msg {
            Msg::Progress(p) => local.update(cx, |l, _| {
                if let Some(j) = &mut l.job {
                    j.progress = p;
                }
            }),
            Msg::Starting => local.update(cx, |l, l_cx| {
                l.installed = l.manager.installed();
                if let Some(j) = &mut l.job {
                    j.starting = true;
                }
                l_cx.notify();
            }),
            Msg::Started(server) => local.update(cx, |l, _| l.server = Some((id.clone(), server))),
            Msg::Failed(message) => local.update(cx, |l, _| {
                let job = l.job.take();
                l.installed = l.manager.installed();
                l.failure = Some(Failure {
                    id: id.clone(),
                    steps: job.map(|j| j.steps).unwrap_or_default(),
                    message,
                });
            }),
            Msg::Finished => {
                let uses = local.update(cx, |l, _| {
                    l.installed = l.manager.installed();
                    l.job.take().is_some_and(|j| j.steps.contains(&Step::Use))
                });
                if uses {
                    self.local_use_now(&id, cx);
                }
            }
        }
        local.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    /// The server for `id` is up: make it Kai's backend.
    fn local_use_now(&mut self, id: &str, cx: &mut Context<Self>) {
        self.settings.backend = BackendChoice::Local;
        self.settings.local_model = id.to_string();
        self.save_settings(cx);
        self.local.update(cx, |l, cx| {
            l.ready = Some(id.to_string());
            cx.notify();
        });
        self.recheck_connection(cx);
    }

    /// Use an installed model, starting it if needed (the switcher's pick).
    pub fn local_use(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut steps = vec![Step::Use];
        if !self.local.read(cx).running(id) {
            steps.insert(0, Step::Start);
        }
        self.local_job(id, steps, cx);
    }

    pub fn local_cancel(&mut self, cx: &mut Context<Self>) {
        self.local.update(cx, |l, cx| {
            if let Some(mut j) = l.job.take()
                && let Some(c) = j.cancel.take()
            {
                let _ = c.send(());
                // Also stop the downloader itself; partial files stay for resuming.
                l.manager.cancel();
            }
            l.installed = l.manager.installed();
            cx.notify();
        });
    }

    pub fn local_retry(&mut self, cx: &mut Context<Self>) {
        let failed = self.local.update(cx, |l, _| l.failure.take());
        if let Some(f) = failed {
            self.local_job(&f.id, f.steps, cx);
        }
    }

    pub fn local_stop(&mut self, cx: &mut Context<Self>) {
        let manager = self.local.read(cx).manager.clone();
        let stopped = manager.stop();
        if let Err(e) = &stopped {
            log::error!("local server did not stop: {e:#}");
        }
        self.local.update(cx, |l, cx| {
            if stopped.is_ok() {
                l.server = None;
            }
            l.ready = None;
            cx.notify();
        });
    }

    pub fn local_remove(&mut self, id: &str, cx: &mut Context<Self>) {
        let manager = self.local.read(cx).manager.clone();
        let r = manager.remove(id);
        self.local.update(cx, |l, cx| {
            l.installed = l.manager.installed();
            if let Err(e) = r {
                l.failure = Some(Failure {
                    id: id.to_string(),
                    steps: Vec::new(),
                    message: format!("{e:#}"),
                });
            }
            cx.notify();
        });
    }
}

/// The local server is ours: stop it when Kai quits.
pub fn stop_on_quit(local: &Entity<LocalState>, cx: &mut gpui::App) {
    let manager = local.read(cx).manager.clone();
    cx.on_app_quit(move |_| {
        let _ = manager.stop();
        async {}
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(stage: &str, bytes: u64, total: Option<u64>) -> Progress {
        Progress {
            stage: stage.into(),
            bytes,
            total,
            message: String::new(),
        }
    }

    #[test]
    fn progress_reads_in_plain_words() {
        let q = p("model", 1_200_000_000, Some(5_000_000_000));
        assert_eq!(bytes_text(&q), "1.2 GB of 5.0 GB · 24%");
        assert_eq!(stage_text(&q, false), "Downloading the model…");
        assert_eq!(stage_text(&q, true), "Starting the local server…");
        assert_eq!(fraction(&p("model", 9, Some(3))), Some(1.0));
        assert_eq!(fraction(&p("model", 9, Some(0))), None);
        assert_eq!(gb(30_000_000), "30 MB");
    }

    #[test]
    fn one_tap_plans_only_what_is_missing() {
        let mut l = LocalState::new("");
        l.set_hardware(HardwareSummary {
            ram_gb: 16.0,
            cpu_threads: 8,
            gpu_vram_gb: None,
            gpu_name: None,
            os: "linux".into(),
            arch: "x86_64".into(),
        });
        let id = l.ranked[0].entry.id.clone();
        assert_eq!(l.plan(&id), [Step::Install, Step::Start, Step::Use]);
        l.installed.push(id.clone());
        assert_eq!(l.plan(&id), [Step::Start, Step::Use]);
    }
}
