//! Running the tasks: the mock backend (scripted screen, dry-run hands) and
//! the live/replay backends (the real desktop). Every model call goes through
//! [`Metered`], which counts calls and sums the token usage each reply reports,
//! because the trace itself does not record tokens.

use super::record::{self, Recording};
use super::tasks::{Available, ReplayMode, Task};
use crate::console::{Printer, TerminalApprover};
use async_trait::async_trait;
use kai_core::client::{Backend, ClientError, ModelReply, ModelRequest};
use kai_core::engine::{Actuator, Engine, NullSink, Perceptor};
use kai_core::executor::Decider;
use kai_core::trace::Trace;
use kai_core::{
    Config, Decision, Element, Grounding, Images, MockClient, ModelClient, Observation, Rect, Role,
    Skill,
};
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One model call as seen from outside the client.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
pub struct Call {
    pub ms: u64,
    pub ok: bool,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

/// Wraps a client and records every call into a shared log.
pub struct Metered<'a> {
    inner: &'a dyn ModelClient,
    log: Arc<Mutex<Vec<Call>>>,
}

impl<'a> Metered<'a> {
    pub fn new(inner: &'a dyn ModelClient, log: Arc<Mutex<Vec<Call>>>) -> Self {
        Self { inner, log }
    }
}

#[async_trait]
impl ModelClient for Metered<'_> {
    fn backend(&self) -> Backend {
        self.inner.backend()
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let started = Instant::now();
        let r = self.inner.decide(req).await;
        let call = match &r {
            Ok(reply) => Call {
                ms: reply.total.as_millis() as u64,
                ok: true,
                prompt_tokens: reply.prompt_tokens,
                completion_tokens: reply.completion_tokens,
            },
            Err(_) => Call {
                ms: started.elapsed().as_millis() as u64,
                ..Default::default()
            },
        };
        self.log.lock().unwrap().push(call);
        r
    }
}

/// What one task produced.
pub struct Run {
    pub task: Task,
    pub trace: Option<Trace>,
    pub took: Duration,
    pub calls: Vec<Call>,
    /// Set when the task did not run here, with the reason.
    pub skipped: Option<String>,
    /// The task's check command, run right after the task (before teardown).
    /// `None` when there is none for this OS or nothing real was touched.
    pub check: Option<bool>,
    /// The task's setup command failed (the task ran anyway).
    pub setup_failed: bool,
    /// 0 for the first run of the task, 1 for the second (`--repeat`).
    pub pass: u32,
}

/// Experience memory for a bench run (`--memory`, `--repeat`): a fresh
/// in-memory store per bench, never the user's own lessons.
#[derive(Clone, Copy, Default)]
pub struct MemoryOpts<'a> {
    pub store: Option<&'a kai_core::MemoryStore>,
    /// Runs per task, back to back (at least 1).
    pub repeat: u32,
}

impl MemoryOpts<'_> {
    fn passes(&self) -> u32 {
        self.repeat.max(1)
    }
}

/// A scripted screen for the mock backend: a browser-like page whose hash
/// changes after every action, so the harness sees progress.
struct ScriptedScreen {
    frame: AtomicU64,
}

#[async_trait]
impl Perceptor for ScriptedScreen {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let names = [
            "Address bar",
            "Search",
            "First result",
            "Save",
            "Name",
            "Email",
        ];
        let roles = [
            Role::TextField,
            Role::TextField,
            Role::Link,
            Role::Button,
            Role::TextField,
            Role::TextField,
        ];
        let els = names
            .iter()
            .zip(roles)
            .enumerate()
            .map(|(i, (n, r))| Element {
                mark: i as u32 + 1,
                role: r,
                name: n.to_string(),
                bounds: Rect::new(40.0, 60.0 + i as f32 * 50.0, 300.0, 32.0),
                enabled: true,
                secure: false,
                value: None,
            })
            .collect();
        Ok(Observation {
            images: Images {
                screenshot_jpeg: vec![0xff, 0xd8, 0xff, 0xd9],
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(els),
            screen_hash: self.frame.load(Ordering::SeqCst),
            timing: Default::default(),
            screen: None,
        })
    }
}

/// Records what would happen and advances the scripted screen.
struct DryRun<'a>(&'a ScriptedScreen);

#[async_trait]
impl Actuator for DryRun<'_> {
    async fn act(&self, _: &Decision, _: &Grounding) -> anyhow::Result<()> {
        self.0.frame.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn open_app(&self, _: &str) -> anyhow::Result<()> {
        self.0.frame.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn open_url(&self, _: &str) -> anyhow::Result<()> {
        self.0.frame.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn shell(&self, cmd: &str) -> anyhow::Result<String> {
        Ok(format!("(dry run, not executed) {cmd}"))
    }
}

/// Run a setup/check/teardown command with `KAI_BENCH_DIR` set. True on exit 0.
pub async fn sh(cmd: &str, bench_dir: &Path) -> bool {
    let shell = kai_os::shell::Shell::new(kai_os::KillSwitch::new());
    let dir = bench_dir.display().to_string();
    let full = if cfg!(windows) {
        format!("$env:KAI_BENCH_DIR='{}'; {cmd}", dir.replace('\'', "''"))
    } else {
        format!(
            "export KAI_BENCH_DIR='{}'; {cmd}",
            dir.replace('\'', r"'\''")
        )
    };
    match shell.run(&full).await {
        Ok(out) => {
            let ok = out.trim_end().ends_with("[exit 0]");
            if !ok {
                log::info!("`{cmd}` -> {out}");
            }
            ok
        }
        Err(_) => false,
    }
}

/// The mock backend with no memory (tests).
#[cfg(test)]
pub async fn run_mock(
    tasks: &[Task],
    skills: &[Skill],
    config: Config,
    rec: Option<&Recording>,
    executor: Option<&dyn Decider>,
) -> Vec<Run> {
    let none = MemoryOpts::default();
    run_mock_with(tasks, skills, config, rec, executor, none).await
}

/// [`run_mock`] with experience memory and repeats.
pub async fn run_mock_with(
    tasks: &[Task],
    skills: &[Skill],
    config: Config,
    rec: Option<&Recording>,
    executor: Option<&dyn Decider>,
    mem: MemoryOpts<'_>,
) -> Vec<Run> {
    let mut out = Vec::new();
    for t in tasks
        .iter()
        .flat_map(|t| (0..mem.passes()).map(move |p| (t, p)))
    {
        let (t, pass) = t;
        let log = Arc::new(Mutex::new(Vec::new()));
        let recorded = MockClient::new(t.mock.clone());
        let client = Metered::new(&recorded, log.clone());
        let screen = ScriptedScreen {
            frame: AtomicU64::new(0),
        };
        let hands = DryRun(&screen);
        let approver = TerminalApprover { auto_yes: true };
        let engine = Engine {
            client: &client,
            careful: None,
            perceptor: &screen,
            actuator: &hands,
            approver: &approver,
            interrupt: &kai_core::engine::NeverStops,
            sink: &NullSink,
            steering: &kai_core::engine::NoSteering,
            verifier: None,
            skills,
            config,
        };
        let started = Instant::now();
        // Only the first pass is recorded (one folder per task id).
        let recorder = rec.filter(|_| pass == 0).and_then(|r| r.start(t));
        let opts = kai_core::RunOptions {
            recorder: recorder.as_ref(),
            executor,
            memory: mem.store,
            ..Default::default()
        };
        let trace = engine.run_with(&t.prompt, opts).await;
        if let Some(r) = &recorder {
            record::finish(r, &trace, None, false);
        }
        let calls = std::mem::take(&mut *log.lock().unwrap());
        out.push(Run {
            task: t.clone(),
            trace: Some(trace),
            took: started.elapsed(),
            calls,
            skipped: None,
            check: None,
            setup_failed: false,
            pass,
        });
    }
    out
}

pub struct Live<'a> {
    /// `None` replays each task's recorded replies on the real desktop.
    pub client: Option<&'a dyn ModelClient>,
    pub careful: Option<&'a dyn ModelClient>,
    pub skills: &'a [Skill],
    pub config: Config,
    pub auto_yes: bool,
    /// Folder of tasks.yaml, exported to setup/check as `KAI_BENCH_DIR`.
    pub bench_dir: &'a Path,
    /// Run tasks even when this machine lacks what they need.
    pub force: bool,
    /// Run OpenAI's own computer-use loop instead of Kai's engine.
    pub native: Option<kai_core::cua::CuaConfig>,
    /// Record every task run as training data.
    pub record: Option<&'a Recording>,
    /// The executor's decision model, when `--executor` is on.
    pub executor: Option<&'a dyn Decider>,
    /// Experience memory and repeats.
    pub memory: MemoryOpts<'a>,
}

pub async fn run_live(tasks: &[Task], live: &Live<'_>) -> anyhow::Result<Vec<Run>> {
    let kill = kai_os::KillSwitch::new();
    let screen = kai_os::DesktopPerceptor::new();
    let hands = kai_os::DesktopActuator::new(kill.clone())?;
    let approver = TerminalApprover {
        auto_yes: live.auto_yes,
    };
    let available = Available::probe();
    eprintln!("this machine has: {}", available.have.join(", "));
    let replay = live.client.is_none();
    let mut out = Vec::new();
    let passes = live.memory.passes();
    for (t, pass) in tasks.iter().flat_map(|t| (0..passes).map(move |p| (t, p))) {
        let missing = available.missing(t);
        let skip = if replay && t.replay == ReplayMode::Skip {
            Some("not replayable (see `replay: skip` in tasks.yaml)".to_string())
        } else if !missing.is_empty() && !live.force {
            Some(format!("needs {}", missing.join(", ")))
        } else {
            None
        };
        if let Some(reason) = skip {
            eprintln!("\n- {} skipped: {reason}", t.id);
            out.push(Run {
                task: t.clone(),
                trace: None,
                took: Duration::ZERO,
                calls: Vec::new(),
                skipped: Some(reason),
                check: None,
                setup_failed: false,
                pass,
            });
            continue;
        }
        let setup_failed = match t.setup.here() {
            Some(cmd) => !sh(cmd, live.bench_dir).await,
            None => false,
        };
        if setup_failed {
            log::warn!("setup for {} failed; running anyway", t.id);
            eprintln!("  (setup for {} failed; running anyway)", t.id);
        }
        eprintln!("\n▶ {} — {}", t.id, t.title);
        kill.reset();
        let log = Arc::new(Mutex::new(Vec::new()));
        let recorded = MockClient::new(t.mock.clone());
        let fast = Metered::new(live.client.unwrap_or(&recorded), log.clone());
        let careful = live.careful.map(|c| Metered::new(c, log.clone()));
        let engine = Engine {
            client: &fast,
            careful: careful.as_ref().map(|c| c as &dyn ModelClient),
            perceptor: &screen,
            actuator: &hands,
            approver: &approver,
            interrupt: &kill,
            sink: &Printer,
            steering: &kai_core::engine::NoSteering,
            verifier: None,
            skills: live.skills,
            config: live.config,
        };
        let recorder = live.record.filter(|_| pass == 0).and_then(|r| r.start(t));
        let opts = kai_core::RunOptions {
            recorder: recorder.as_ref(),
            executor: live.executor,
            memory: live.memory.store,
            // Learn after the check, which knows whether `done` was true.
            learn_later: true,
            ..Default::default()
        };
        let started = Instant::now();
        let trace = match &live.native {
            Some(cfg) => native_trace(cfg, &t.prompt, &screen, &hands, &kill).await,
            None => engine.run_with(&t.prompt, opts).await,
        };
        let took = started.elapsed();
        let calls = std::mem::take(&mut *log.lock().unwrap());
        let check = match t.check.here() {
            Some(cmd) => Some(sh(cmd, live.bench_dir).await),
            None => None,
        };
        if let Some(cmd) = t.teardown.here() {
            sh(cmd, live.bench_dir).await;
        }
        if let Some(r) = &recorder {
            record::finish(r, &trace, check, setup_failed);
        }
        if let Some(m) = live.memory.store
            && let Err(e) = m.after_run(&trace, check, Vec::new(), live.config.learn)
        {
            log::warn!("memory: {e}");
        }
        out.push(Run {
            task: t.clone(),
            trace: Some(trace),
            took,
            calls,
            skipped: None,
            check,
            setup_failed,
            pass,
        });
    }
    Ok(out)
}

/// One task through OpenAI's native computer-use loop, as a Kai trace.
async fn native_trace(
    cfg: &kai_core::cua::CuaConfig,
    task: &str,
    screen: &kai_os::DesktopPerceptor,
    hands: &kai_os::DesktopActuator,
    kill: &kai_os::KillSwitch,
) -> kai_core::Trace {
    let runner = kai_core::cua::CuaRunner {
        config: cfg.clone(),
        perceptor: screen,
        actuator: hands,
        interrupt: kill,
        sink: &Printer,
    };
    match runner.run(task).await {
        Ok(run) => {
            if let Some(e) = &run.error {
                eprintln!("  native loop error: {e}");
            }
            run.trace
        }
        Err(e) => {
            eprintln!("  native loop could not start: {e}");
            let mut t = kai_core::Trace::new(task);
            t.finish(kai_core::Outcome::Failed);
            t
        }
    }
}
