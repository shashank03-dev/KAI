//! `kai bench`: run the tasks in `bench/tasks.yaml` the same way on each
//! backend and write `bench/results/<date>-<backend>.json`.
//!
//! The `mock` backend replays each task's recorded replies against a scripted
//! screen and a dry-run actuator. It measures the harness itself, not a model,
//! and touches nothing on the machine; results say so.

use crate::console::{Printer, TerminalApprover};
use async_trait::async_trait;
use kai_core::engine::{Actuator, Engine, NullSink, Perceptor};
use kai_core::trace::{Outcome, Trace};
use kai_core::{
    Config, Decision, Element, Grounding, Images, MockClient, ModelClient, Observation, Rect, Role,
    Skill,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A command per OS. Missing means "not available on this OS".
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PerOs {
    pub linux: Option<String>,
    pub windows: Option<String>,
}

impl PerOs {
    pub fn here(&self) -> Option<&str> {
        if cfg!(windows) {
            self.windows.as_deref()
        } else {
            self.linux.as_deref()
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub prompt: String,
    #[serde(default)]
    pub setup: PerOs,
    /// Exit code 0 means pass. Without a check, the run's own outcome decides.
    #[serde(default)]
    pub check: PerOs,
    /// Recorded replies for the mock backend.
    #[serde(default)]
    pub mock: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskResult {
    pub id: String,
    pub title: String,
    pub outcome: Option<Outcome>,
    pub pass: bool,
    /// How pass was decided: "check", "outcome" or "skipped".
    pub judged_by: String,
    pub steps: usize,
    pub seconds: f64,
    pub step_p50_ms: u64,
    pub step_p95_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub date: String,
    pub backend: String,
    pub os: String,
    pub note: String,
    pub tasks: Vec<TaskResult>,
    pub success_rate: f64,
    pub passed: usize,
    pub total: usize,
    pub step_p50_ms: u64,
    pub step_p95_ms: u64,
    pub total_seconds: f64,
}

pub fn load_tasks(path: &Path) -> anyhow::Result<Vec<Task>> {
    let tasks: Vec<Task> = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    anyhow::ensure!(!tasks.is_empty(), "no tasks in {}", path.display());
    Ok(tasks)
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

async fn sh(cmd: &str) -> bool {
    let shell = kai_os::shell::Shell::new(kai_os::KillSwitch::new());
    match shell.run(cmd).await {
        Ok(out) => out.trim_end().ends_with("[exit 0]"),
        Err(_) => false,
    }
}

pub async fn run_mock(tasks: &[Task], skills: &[Skill]) -> Vec<(Task, Trace, Duration)> {
    let mut out = Vec::new();
    for t in tasks {
        let client = MockClient::new(t.mock.clone());
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
            config: Config::default(),
        };
        let started = Instant::now();
        let trace = engine.run(&t.prompt).await;
        out.push((t.clone(), trace, started.elapsed()));
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
}

pub async fn run_live(
    tasks: &[Task],
    live: &Live<'_>,
) -> anyhow::Result<Vec<(Task, Trace, Duration)>> {
    let kill = kai_os::KillSwitch::new();
    let screen = kai_os::DesktopPerceptor::new();
    let hands = kai_os::DesktopActuator::new(kill.clone())?;
    let approver = TerminalApprover {
        auto_yes: live.auto_yes,
    };
    let mut out = Vec::new();
    for t in tasks {
        if let Some(cmd) = t.setup.here()
            && !sh(cmd).await
        {
            log::warn!("setup for {} failed; running anyway", t.id);
        }
        eprintln!("\n▶ {} — {}", t.id, t.title);
        kill.reset();
        let replay = MockClient::new(t.mock.clone());
        let engine = Engine {
            client: live.client.unwrap_or(&replay),
            careful: live.careful,
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
        let started = Instant::now();
        let trace = engine.run(&t.prompt).await;
        out.push((t.clone(), trace, started.elapsed()));
    }
    Ok(out)
}

pub async fn summarise(backend: &str, runs: Vec<(Task, Trace, Duration)>, mock: bool) -> Summary {
    let replay = backend == "replay";
    let mut results = Vec::new();
    let mut all_steps: Vec<u64> = Vec::new();
    let mut total = 0.0;
    for (t, trace, took) in runs {
        let (pass, judged_by) = match (mock, t.check.here()) {
            (false, Some(cmd)) => (sh(cmd).await, "check"),
            _ => (trace.outcome == Some(Outcome::Success), "outcome"),
        };
        let (p50, p95) = trace.step_percentiles();
        all_steps.extend(trace.steps.iter().map(|s| s.timings.total_ms()));
        total += took.as_secs_f64();
        results.push(TaskResult {
            id: t.id,
            title: t.title,
            outcome: trace.outcome,
            pass,
            judged_by: judged_by.into(),
            steps: trace.steps.len(),
            seconds: (took.as_secs_f64() * 100.0).round() / 100.0,
            step_p50_ms: p50,
            step_p95_ms: p95,
        });
    }
    all_steps.sort_unstable();
    let pct = |p: f64| {
        all_steps
            .get(((all_steps.len().max(1) - 1) as f64 * p).round() as usize)
            .copied()
            .unwrap_or(0)
    };
    let passed = results.iter().filter(|r| r.pass).count();
    Summary {
        date: jiff::Zoned::now().date().to_string(),
        backend: backend.into(),
        os: std::env::consts::OS.into(),
        note: if replay {
            "Replay: each task's recorded replies executed for real on this desktop (real capture, accessibility tree, input and shell), judged by the task's check command. Proves Kai's hands, not a model.".into()
        } else if mock {
            "Mock backend: recorded replies on a scripted screen with a dry-run actuator. Measures harness overhead only. No model was called and nothing on the machine was touched.".into()
        } else {
            "Live run on the real desktop. Pass is decided by each task's check command where it has one.".into()
        },
        success_rate: if results.is_empty() {
            0.0
        } else {
            passed as f64 / results.len() as f64
        },
        passed,
        total: results.len(),
        step_p50_ms: pct(0.5),
        step_p95_ms: pct(0.95),
        total_seconds: (total * 100.0).round() / 100.0,
        tasks: results,
    }
}

pub fn write(summary: &Summary, dir: &Path) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}-{}.json", summary.date, summary.backend));
    std::fs::write(&path, serde_json::to_string_pretty(summary)?)?;
    Ok(path)
}

pub fn print(s: &Summary) {
    eprintln!(
        "\n{:<22} {:<10} {:>6} {:>8} {:>9} {:>9}",
        "task", "result", "steps", "secs", "p50 ms", "p95 ms"
    );
    for t in &s.tasks {
        eprintln!(
            "{:<22} {:<10} {:>6} {:>8.2} {:>9} {:>9}",
            t.id,
            if t.pass { "pass" } else { "FAIL" },
            t.steps,
            t.seconds,
            t.step_p50_ms,
            t.step_p95_ms
        );
    }
    eprintln!(
        "\n{} / {} passed ({:.0}%) · step p50 {} ms · p95 {} ms · {:.1} s total · backend {}",
        s.passed,
        s.total,
        s.success_rate * 100.0,
        s.step_p50_ms,
        s.step_p95_ms,
        s.total_seconds,
        s.backend
    );
}
