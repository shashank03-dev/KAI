//! The bridge between the agent and the UI. The engine runs on a small tokio
//! runtime; it reports through a channel the UI drains, and asks for approval
//! (or for the user's help at a CAPTCHA, one-time code or question) through a
//! one-shot the UI answers. The UI thread never waits on the network, the
//! screen or the model.

use crate::control::RunControl;
use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use kai_core::engine::{ApprovalChoice, Approver, EngineEvent, EventSink};
use kai_core::human::{HumanCheck, HumanHelper, HumanReply};
use kai_core::orchestrator::{Orchestrator, Plan};
use kai_core::settings::{Keys, Settings};
use kai_core::{Config, Decision, Engine, Grounding, Images, Rect, Skill, Trace};
use kai_os::{DesktopActuator, DesktopPerceptor, KillSwitch};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// What the agent tells the UI.
pub enum Ui {
    Event(EngineEvent),
    /// A frame saved to disk for the viewer.
    Frame {
        step: u32,
        path: PathBuf,
    },
    Approval(Pending),
    /// The run is paused until the user gets past a human check.
    Human(HumanPending),
    Plan(Plan),
    /// Team mode's final report: the plan that ran and each sub-agent's end.
    Report {
        summary: String,
        plan: Plan,
        results: Vec<SubOutcome>,
    },
    /// The engine is holding at a pause (true) or went on (false).
    Paused(bool),
    Finished(Box<Trace>),
    Failed(String),
}

/// How one sub-agent ended.
#[derive(Debug, Clone)]
pub struct SubOutcome {
    pub id: String,
    pub outcome: Option<kai_core::Outcome>,
    pub steps: usize,
}

/// A risky step waiting on the user.
pub struct Pending {
    pub decision: Decision,
    /// The element's name, when the step targets one.
    pub element: Option<String>,
    pub reply: oneshot::Sender<ApprovalChoice>,
}

/// A CAPTCHA, one-time code, sign-in approval or question waiting on the
/// user. Dropping `reply` (window closed, run stopped) counts as Stop.
pub struct HumanPending {
    pub check: HumanCheck,
    pub reply: oneshot::Sender<HumanReply>,
}

/// Lives for the app: the runtime, the hands (one input thread) and the stop flag.
pub struct Agent {
    pub runtime: tokio::runtime::Runtime,
    pub kill: KillSwitch,
    pub perceptor: Arc<DesktopPerceptor>,
    actuator: Option<Arc<DesktopActuator>>,
    pub running: Arc<AtomicBool>,
}

impl Agent {
    pub fn start() -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("kai-agent")
            .enable_all()
            .build()?;
        let kill = KillSwitch::new();
        let actuator = match DesktopActuator::new(kill.clone()) {
            Ok(a) => Some(Arc::new(a)),
            Err(e) => {
                log::error!("agent: no mouse/keyboard control: {e}");
                None
            }
        };
        Ok(Self {
            runtime,
            kill,
            perceptor: Arc::new(DesktopPerceptor::new()),
            actuator,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn can_act(&self) -> bool {
        self.actuator.is_some()
    }

    pub fn stop(&self) {
        self.kill.trip("stop button");
    }

    /// Start a run. Events arrive on the returned receiver until `Finished`.
    /// `control` pauses, stops and steers this run.
    pub fn run(
        &self,
        task: String,
        team: bool,
        settings: Settings,
        keys: Keys,
        skills: Vec<Skill>,
        control: Arc<RunControl>,
    ) -> mpsc::UnboundedReceiver<Ui> {
        let (tx, rx) = mpsc::unbounded();
        let Some(actuator) = self.actuator.clone() else {
            let _ = tx.unbounded_send(Ui::Failed(
                "Kai can't move the mouse or type on this desktop. Open Settings › Permissions."
                    .into(),
            ));
            return rx;
        };
        self.kill.reset();
        self.running.store(true, Ordering::SeqCst);
        let (held_tx, mut held_rx) = mpsc::unbounded();
        control.report_to(held_tx);
        let forward = tx.clone();
        self.runtime.spawn(async move {
            use futures::StreamExt;
            while let Some(held) = held_rx.next().await {
                let _ = forward.unbounded_send(Ui::Paused(held));
            }
        });
        let (perceptor, running) = (self.perceptor.clone(), self.running.clone());
        self.runtime.spawn(kai_os::kill::watch_corner(
            self.kill.clone(),
            actuator.input.clone(),
            running.clone(),
        ));
        self.runtime.spawn(async move {
            let result = drive(
                task,
                team,
                settings,
                keys,
                skills,
                &tx,
                control.as_ref(),
                perceptor.as_ref(),
                actuator.as_ref(),
            )
            .await;
            // Close the pause reporter so the UI's receiver ends with the run.
            control.detach_reporter();
            running.store(false, Ordering::SeqCst);
            if let Err(e) = result {
                let _ = tx.unbounded_send(Ui::Failed(friendly(&e)));
            }
        });
        rx
    }

    /// Check screen, accessibility and input access (runs on the agent runtime,
    /// since the accessibility bus needs it).
    pub fn permissions(&self) -> oneshot::Receiver<Vec<kai_os::permissions::Check>> {
        let (tx, rx) = oneshot::channel();
        self.runtime.spawn(async move {
            let _ = tx.send(kai_os::permissions::check_all().await);
        });
        rx
    }

    pub fn enable_accessibility(&self) {
        self.runtime.spawn(async {
            if let Err(e) = kai_os::permissions::enable_accessibility().await {
                log::error!("agent: could not turn on accessibility: {e}");
            }
        });
    }

    /// Send one real screenshot to the configured backend and time it.
    pub fn test_connection(
        &self,
        settings: Settings,
        keys: Keys,
    ) -> oneshot::Receiver<Result<(Duration, String), String>> {
        let (tx, rx) = oneshot::channel();
        self.runtime.spawn(async move {
            let r = async {
                let (client, _) = settings.clients(&keys)?;
                let images = Images {
                    screenshot_jpeg: probe_jpeg(),
                    crop_jpeg: None,
                };
                let started = Instant::now();
                let reply = client.warm_up(&images).await?;
                anyhow::Ok((started.elapsed(), reply.backend.label().to_string()))
            }
            .await
            .map_err(|e| friendly(&e));
            let _ = tx.send(r);
        });
        rx
    }
}

#[allow(clippy::too_many_arguments)]
async fn drive(
    task: String,
    team: bool,
    settings: Settings,
    keys: Keys,
    skills: Vec<Skill>,
    tx: &mpsc::UnboundedSender<Ui>,
    control: &RunControl,
    perceptor: &DesktopPerceptor,
    actuator: &DesktopActuator,
) -> anyhow::Result<()> {
    let (fast, careful) = settings.clients(&keys)?;
    let (max_steps, max_time) = settings.limits();
    let config = Config {
        max_steps,
        max_time,
        approval: settings.approval,
        executor: settings.use_executor,
        outcomes: settings.verify_outcomes,
        macro_history: settings.macro_history,
        procedures: settings.use_procedures,
        best_of: if settings.best_of_plans { 3 } else { 0 },
        learn: settings.learn,
        reflect: settings.reflect,
        ..Config::default()
    };
    let dir = run_dir();
    std::fs::create_dir_all(&dir)?;
    let sink = Sink {
        tx: tx.clone(),
        dir: dir.clone(),
    };
    let approver = UiApprover { tx: tx.clone() };
    let human = UiHuman { tx: tx.clone() };

    if team {
        let orch = Orchestrator {
            client: fast.as_ref(),
            careful: careful.as_deref(),
            screen: perceptor,
            actuator,
            approver: &approver,
            interrupt: control,
            sink: &sink,
            steering: control,
            human: Some(&human),
            skills: &skills,
            config,
            parallelism: settings.parallelism,
        };
        let plan = orch.plan(&task).await.ok();
        if let Some(p) = &plan {
            let _ = tx.unbounded_send(Ui::Plan(p.clone()));
        }
        // Run the plan the UI shows; plan again only if planning failed.
        let report = match plan {
            Some(p) => orch.run_plan(&task, p).await?,
            None => orch.run(&task).await?,
        };
        let mut trace = Trace::new(&task);
        for r in &report.results {
            for s in &r.trace.steps {
                trace.push(s.clone());
            }
        }
        trace.finish(report.outcome());
        save(&dir, &trace);
        let results = report
            .results
            .iter()
            .map(|r| SubOutcome {
                id: r.subtask.id.clone(),
                outcome: r.trace.outcome,
                steps: r.trace.steps.len(),
            })
            .collect();
        let _ = tx.unbounded_send(Ui::Report {
            summary: report.summary,
            plan: report.plan,
            results,
        });
        let _ = tx.unbounded_send(Ui::Finished(Box::new(trace)));
    } else {
        let engine = Engine {
            client: fast.as_ref(),
            careful: careful.as_deref(),
            perceptor,
            actuator,
            approver: &approver,
            interrupt: control,
            sink: &sink,
            steering: control,
            verifier: None,
            human: Some(&human),
            skills: &skills,
            config,
        };
        // Opt-in (Settings › record_traces): keep this run as local training
        // data. Plan cache: the known-good opening of a repeated task.
        let run_id = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "run".into());
        let recorder = settings
            .record_traces
            .then(|| {
                let meta = kai_core::recorder::RunMeta::new(
                    run_id.clone(),
                    task.clone(),
                    "app",
                    format!("{:?}", settings.backend).to_lowercase(),
                    settings.fast_model.clone(),
                );
                kai_core::recorder::TraceRecorder::new(
                    kai_core::settings::Settings::runs_dir().join(&run_id),
                    meta,
                )
                .map(|r| r.with_careful_model(settings.careful_model.clone()))
                .map_err(|e| log::warn!("trace recording off: {e}"))
                .ok()
            })
            .flatten();
        let executor = settings.use_executor.then(|| settings.executor_client());
        let cache =
            kai_core::PlanCache::load(kai_core::settings::Settings::dir().join("plans")).ok();
        let procs = kai_core::ProcedureStore::load(Settings::dir().join("procedures")).ok();
        // Experience memory (docs/MEMORY.md): lessons from earlier runs on
        // this computer as hints; this run is learned from when
        // Settings › Privacy › "Learn from mistakes" is on.
        let memory = kai_core::MemoryStore::load(Settings::memory_dir())
            .map_err(|e| log::warn!("agent: memory off: {e}"))
            .ok();
        let trace = engine
            .run_with(
                &task,
                kai_core::RunOptions {
                    plan_cache: cache.as_ref(),
                    recorder: recorder.as_ref(),
                    executor: executor.as_ref().map(|c| c as _),
                    memory: memory.as_ref(),
                    learn_later: false,
                    probe: Some(&kai_os::DesktopProbe),
                    procedures: procs.as_ref(),
                },
            )
            .await;
        save(&dir, &trace);
        let _ = tx.unbounded_send(Ui::Finished(Box::new(trace)));
    }
    Ok(())
}

/// Each run gets a folder of its own under the local traces folder.
fn run_dir() -> PathBuf {
    Settings::traces_dir().join(jiff::Timestamp::now().as_millisecond().to_string())
}

fn save(dir: &std::path::Path, trace: &Trace) {
    match serde_json::to_string_pretty(trace) {
        Ok(json) => {
            if let Err(e) = std::fs::write(dir.join("trace.json"), json) {
                log::error!("agent: trace not saved: {e}");
            }
        }
        Err(e) => log::error!("agent: trace not serialised: {e}"),
    }
}

struct Sink {
    tx: mpsc::UnboundedSender<Ui>,
    dir: PathBuf,
}

impl EventSink for Sink {
    fn emit(&self, ev: &EngineEvent) {
        if let EngineEvent::Observed {
            step, screenshot, ..
        } = ev
            && !screenshot.is_empty()
        {
            let path = self.dir.join(format!("{step:03}.jpg"));
            match std::fs::write(&path, screenshot) {
                Ok(()) => {
                    let _ = self.tx.unbounded_send(Ui::Frame { step: *step, path });
                }
                Err(e) => log::error!("agent: frame {step} not saved: {e}"),
            }
        }
        let _ = self.tx.unbounded_send(Ui::Event(ev.clone()));
    }
}

struct UiApprover {
    tx: mpsc::UnboundedSender<Ui>,
}

#[async_trait]
impl Approver for UiApprover {
    async fn approve(&self, decision: &Decision, grounding: &Grounding) -> ApprovalChoice {
        let (reply, answer) = oneshot::channel();
        let element = decision
            .target
            .and_then(|t| grounding.element(t))
            .map(|e| e.name.clone());
        if self
            .tx
            .unbounded_send(Ui::Approval(Pending {
                decision: decision.clone(),
                element,
                reply,
            }))
            .is_err()
        {
            return ApprovalChoice::Stop;
        }
        // A closed window or dropped dialog counts as Stop.
        answer.await.unwrap_or(ApprovalChoice::Stop)
    }
}

/// Asks the user through the app: a desktop notification, then the mini
/// window and the main window's dialog (see `human.rs`).
struct UiHuman {
    tx: mpsc::UnboundedSender<Ui>,
}

#[async_trait]
impl HumanHelper for UiHuman {
    async fn ask(&self, check: &HumanCheck) -> HumanReply {
        let (reply, answer) = oneshot::channel();
        if self
            .tx
            .unbounded_send(Ui::Human(HumanPending {
                check: check.clone(),
                reply,
            }))
            .is_err()
        {
            return HumanReply::Stop;
        }
        // The dialog is up; now the notification (bounded, best effort).
        kai_os::notify::desktop(check.kind.title(), &check.prompt).await;
        // A closed window or dropped dialog counts as Stop.
        answer.await.unwrap_or(HumanReply::Stop)
    }
}

/// A small real JPEG so the connection test exercises image input.
fn probe_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_fn(64, 40, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 6) as u8, 160])
    });
    kai_os::frame::encode_jpeg(&img)
}

/// Turn an error into words a person can act on.
pub fn friendly(e: &anyhow::Error) -> String {
    let s = format!("{e:#}");
    let l = s.to_ascii_lowercase();
    if l.contains("401")
        || l.contains("403")
        || l.contains("api key not valid")
        || l.contains("unauthorized")
    {
        "The key was refused (401/403). Check the API key in Settings › Model & server.".into()
    } else if l.contains("timed out") || l.contains("timeout") {
        "The model didn't answer in time. Is the Droplet up? Run deploy/digitalocean/up.sh, or add a Gemini key so Kai can fall back.".into()
    } else if l.contains("dns") || l.contains("connect") || l.contains("transport") {
        "Kai couldn't reach the model server. Check the address and your internet connection."
            .into()
    } else if l.contains("no digitalocean")
        || l.contains("no gemini")
        || l.contains("set up the digitalocean")
    {
        "No model is set up yet. Add the DigitalOcean server or a Gemini key in Settings › Model & server.".into()
    } else if l.contains("404") {
        "The server answered 404: the model name or address is wrong. The address should end in /v1.".into()
    } else {
        format!("Something went wrong: {s}")
    }
}

/// Kai's windows, so Linux can mask them out of every frame.
pub fn exclude(perceptor: &DesktopPerceptor, rects: Vec<Rect>) {
    perceptor.set_excluded(rects);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_read_as_next_steps() {
        assert!(
            friendly(&anyhow::anyhow!("backend returned status 401: nope"))
                .contains("key was refused")
        );
        assert!(friendly(&anyhow::anyhow!("backend timed out after 45s")).contains("Droplet up"));
        assert!(
            friendly(&anyhow::anyhow!(
                "set up the DigitalOcean server or a Gemini key"
            ))
            .contains("No model is set up")
        );
    }
}
