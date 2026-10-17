//! The bridge between the agent and the UI. The engine runs on a small tokio
//! runtime; it reports through a channel the UI drains, and asks for approval
//! (or for the user's help at a CAPTCHA, one-time code or question) through a
//! one-shot the UI answers. The UI thread never waits on the network, the
//! screen or the model.

use crate::control::RunControl;
pub use crate::errors::friendly;
use futures::channel::{mpsc, oneshot};
use kai_core::engine::{ApprovalChoice, EngineEvent, EventSink};
use kai_core::human::{HumanCheck, HumanReply};
use kai_core::orchestrator::{Orchestrator, Plan};
use kai_core::settings::{Keys, Settings};
use kai_core::{Decision, Engine, Images, Rect, Skill, Trace};
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
    /// For sensitive data or a secure field: the action, its risk and how
    /// the data is used (`kai_core::risk::explain`).
    pub explain: Option<String>,
    pub reply: oneshot::Sender<ApprovalChoice>,
}

/// A CAPTCHA, one-time code, sign-in approval or question waiting on the
/// user. Dropping `reply` (window closed, run stopped) counts as Stop.
pub struct HumanPending {
    pub check: HumanCheck,
    pub reply: oneshot::Sender<HumanReply>,
}

/// How often a missing mouse/keyboard handle is tried again.
const HANDS_RETRY: Duration = Duration::from_secs(2);

struct Hands {
    now: Option<Arc<DesktopActuator>>,
    tried: Instant,
    /// A retry is running on the blocking pool.
    trying: bool,
}

type HandsSlot = Arc<std::sync::Mutex<Hands>>;

fn lock(slot: &HandsSlot) -> std::sync::MutexGuard<'_, Hands> {
    slot.lock().unwrap_or_else(|e| e.into_inner())
}

fn hands(kill: &KillSwitch) -> anyhow::Result<Arc<DesktopActuator>> {
    DesktopActuator::new(kill.clone()).map(Arc::new)
}

/// Lives for the app: the runtime, the hands (one input thread) and the stop flag.
pub struct Agent {
    pub runtime: tokio::runtime::Runtime,
    pub kill: KillSwitch,
    pub perceptor: Arc<DesktopPerceptor>,
    /// `None` until the OS lets Kai control input (a Mac asks for the
    /// Accessibility permission first); retried now and then.
    actuator: HandsSlot,
    pub running: Arc<AtomicBool>,
    /// Kai's own browser, when Settings › use_browser is on.
    lanes: Arc<crate::browser::Lanes>,
}

impl Agent {
    pub fn start() -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("kai-agent")
            .enable_all()
            .build()?;
        let kill = KillSwitch::new();
        let actuator = hands(&kill)
            .map_err(|e| log::error!("agent: no mouse/keyboard control: {e}"))
            .ok();
        Ok(Self {
            runtime,
            kill,
            perceptor: Arc::new(DesktopPerceptor::new()),
            actuator: Arc::new(std::sync::Mutex::new(Hands {
                now: actuator,
                tried: Instant::now(),
                trying: false,
            })),
            running: Arc::new(AtomicBool::new(false)),
            lanes: Arc::default(),
        })
    }

    /// Whether Kai has the mouse and keyboard. Cheap: Home asks on every
    /// repaint. Without them, a retry starts off the UI thread (at most once
    /// per [`HANDS_RETRY`]), since building them waits on the OS.
    pub fn can_act(&self) -> bool {
        let mut h = lock(&self.actuator);
        if h.now.is_none() && !h.trying && h.tried.elapsed() >= HANDS_RETRY {
            h.trying = true;
            let (slot, kill) = (self.actuator.clone(), self.kill.clone());
            self.runtime.spawn_blocking(move || {
                retry_hands(&slot, &kill);
            });
        }
        h.now.is_some()
    }

    pub fn stop(&self) {
        self.kill.trip("stop button");
    }

    /// Start a run. Events arrive on the returned receiver until `Finished`;
    /// the folder is where its trace is saved. `control` pauses, stops and
    /// steers this run.
    pub fn run(
        &self,
        task: String,
        team: bool,
        settings: Settings,
        skills: Vec<Skill>,
        control: Arc<RunControl>,
    ) -> (mpsc::UnboundedReceiver<Ui>, PathBuf) {
        let (tx, rx) = mpsc::unbounded();
        let dir = run_dir();
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
        let (lanes, slot, kill) = (self.lanes.clone(), self.actuator.clone(), self.kill.clone());
        let trace_dir = dir.clone();
        self.runtime.spawn(async move {
            // The hands, tried once more off the UI thread if still missing.
            let have = lock(&slot).now.clone();
            let actuator = match have {
                Some(a) => Some(a),
                None => {
                    let (slot, kill) = (slot.clone(), kill.clone());
                    tokio::task::spawn_blocking(move || retry_hands(&slot, &kill))
                        .await
                        .ok()
                        .flatten()
                }
            };
            let Some(actuator) = actuator else {
                running.store(false, Ordering::SeqCst);
                control.detach_reporter();
                let _ = tx.unbounded_send(Ui::Failed(
                    "Kai can't move the mouse or type on this desktop. Open Settings › Permissions."
                        .into(),
                ));
                return;
            };
            tokio::spawn(kai_os::kill::watch_corner(
                kill,
                actuator.input.clone(),
                running.clone(),
            ));
            let keys = if matches!(
                settings.backend,
                kai_core::settings::BackendChoice::Codex
                    | kai_core::settings::BackendChoice::Claude
            ) {
                Keys::default()
            } else {
                crate::keys::resolve(Keys::default()).await
            };
            // Kai's own browser (Settings): its eyes and hands for this run.
            let lane = match settings.use_browser && !settings.gui_only {
                true => Some(lanes.get(actuator.clone()).await),
                false => None,
            };
            let result = match &lane {
                Some(Err(e)) => Err(anyhow::anyhow!("{e:#}")),
                Some(Ok(l)) => {
                    let (t, c) = (&tx, control.as_ref());
                    drive(
                        task,
                        team,
                        settings,
                        keys,
                        skills,
                        &dir,
                        t,
                        c,
                        l.as_ref(),
                        l.as_ref(),
                    )
                    .await
                }
                None => {
                    let (t, c) = (&tx, control.as_ref());
                    let (p, a) = (perceptor.as_ref(), actuator.as_ref());
                    drive(task, team, settings, keys, skills, &dir, t, c, p, a).await
                }
            };
            // Close the pause reporter so the UI's receiver ends with the run.
            control.detach_reporter();
            running.store(false, Ordering::SeqCst);
            if let Err(e) = result {
                let _ = tx.unbounded_send(Ui::Failed(friendly(&e)));
            }
        });
        (rx, trace_dir)
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
    /// `typed` keys (not saved yet) win over stored ones.
    pub fn test_connection(
        &self,
        settings: Settings,
        typed: Keys,
    ) -> oneshot::Receiver<Result<(Duration, String), String>> {
        let (tx, rx) = oneshot::channel();
        self.runtime.spawn(async move {
            let keys = if matches!(
                settings.backend,
                kai_core::settings::BackendChoice::Codex
                    | kai_core::settings::BackendChoice::Claude
            ) {
                typed
            } else {
                crate::keys::resolve(typed).await
            };
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
    dir: &std::path::Path,
    tx: &mpsc::UnboundedSender<Ui>,
    control: &RunControl,
    perceptor: &dyn kai_core::Perceptor,
    actuator: &dyn kai_core::Actuator,
) -> anyhow::Result<()> {
    let gui_hands = crate::gui_hands::GuiHands(actuator);
    let actuator: &dyn kai_core::Actuator = if settings.gui_only {
        &gui_hands
    } else {
        actuator
    };
    let team = team && !settings.gui_only;
    let skills = if settings.gui_only {
        Vec::new()
    } else {
        skills
    };
    let task = if settings.gui_only {
        format!(
            "{task}\nUse visible GUI controls only. Never open a terminal or execute shell commands, scripts, or code. Ask the user if blocked."
        )
    } else {
        task
    };
    let (fast, careful) = settings.clients(&keys)?;
    let mut config = settings.engine_config();
    if settings.gui_only {
        config.executor = false;
        config.procedures = false;
        config.best_of = 0;
        config.reflect = false;
        config.learn = false;
        config.outcomes = false;
        config.task_checks = false;
    }
    std::fs::create_dir_all(dir)?;
    let sink = Sink {
        tx: tx.clone(),
        dir: dir.to_path_buf(),
    };
    let approver = crate::asks::UiApprover { tx: tx.clone() };
    let human = crate::asks::UiHuman { tx: tx.clone() };

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
        save(dir, &trace);
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
            grounder: None,
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
                    injection_monitor: None,
                    plan_cache: if settings.gui_only {
                        None
                    } else {
                        cache.as_ref()
                    },
                    recorder: recorder.as_ref(),
                    executor: if settings.gui_only {
                        None
                    } else {
                        executor.as_ref().map(|c| c as _)
                    },
                    memory: if settings.gui_only {
                        None
                    } else {
                        memory.as_ref()
                    },
                    learn_later: false,
                    probe: if settings.gui_only {
                        None
                    } else {
                        Some(&kai_os::DesktopProbe)
                    },
                    procedures: if settings.gui_only {
                        None
                    } else {
                        procs.as_ref()
                    },
                },
            )
            .await;
        save(dir, &trace);
        let _ = tx.unbounded_send(Ui::Finished(Box::new(trace)));
    }
    Ok(())
}

/// Build the hands now (blocking: it waits for the OS input backend) and
/// keep them for later runs.
fn retry_hands(slot: &HandsSlot, kill: &KillSwitch) -> Option<Arc<DesktopActuator>> {
    let made = hands(kill)
        .map_err(|e| log::debug!("agent: still no mouse/keyboard control: {e}"))
        .ok();
    let mut h = lock(slot);
    h.tried = Instant::now();
    h.trying = false;
    if h.now.is_none() {
        h.now = made;
    }
    h.now.clone()
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

/// A small real JPEG so the connection test exercises image input.
fn probe_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_fn(64, 40, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 6) as u8, 160])
    });
    kai_os::frame::encode_jpeg(&img)
}

/// Kai's windows, so Linux can mask them out of every frame.
pub fn exclude(perceptor: &DesktopPerceptor, rects: Vec<Rect>) {
    perceptor.set_excluded(rects);
}
