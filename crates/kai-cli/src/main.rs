//! `kai` — run Gemma 4 on your computer from the terminal, validate skills and
//! run the benchmark. The desktop app is `kai-app`; both share kai-core.

mod bench;
mod console;

use clap::{Parser, Subcommand, ValueEnum};
use console::{Printer, TerminalApprover};
use kai_core::settings::{BackendChoice, Settings};
use kai_core::{Config, Engine, Skill};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "kai",
    version,
    about = "Kai: Gemma 4's hands on your computer"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum BackendArg {
    Auto,
    Vllm,
    Gemini,
    Mock,
    /// Recorded replies, executed for real on this desktop (bench only).
    Replay,
}

#[derive(Subcommand)]
enum Cmd {
    /// Do a task on this computer, step by step, asking before risky actions.
    Run {
        task: String,
        #[arg(long, value_enum, default_value = "auto")]
        backend: BackendArg,
        /// Split the task across parallel sub-agents.
        #[arg(long)]
        team: bool,
        /// Approve everything except passwords and payments.
        #[arg(long)]
        yes: bool,
    },
    /// Skill tools.
    Skill {
        #[command(subcommand)]
        cmd: SkillCmd,
    },
    /// Run bench/tasks.yaml and write bench/results/<date>-<backend>.json.
    Bench {
        #[arg(long, value_enum, default_value = "mock")]
        backend: BackendArg,
        #[arg(long, default_value = "bench/tasks.yaml")]
        tasks: PathBuf,
        #[arg(long, default_value = "bench/results")]
        out: PathBuf,
        /// Only these task ids.
        #[arg(long)]
        only: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Send one real screenshot to the model and report the latency.
    Smoke {
        #[arg(long, value_enum, default_value = "auto")]
        backend: BackendArg,
        #[arg(long, default_value = "5")]
        n: u32,
    },
    /// Check screen, accessibility and input permissions.
    Doctor,
    /// Store an API key in the OS keychain: `kai key gemini` (reads stdin).
    Key {
        #[arg(value_parser = ["gemini", "vllm"])]
        which: String,
    },
}

#[derive(Subcommand)]
enum SkillCmd {
    /// Check skills against the Agent Skill Open Standard.
    Validate { dirs: Vec<PathBuf> },
    /// List installed skills.
    List,
}

fn settings_for(b: BackendArg) -> Settings {
    let mut s = Settings::load();
    s.backend = match b {
        BackendArg::Auto => s.backend,
        BackendArg::Vllm => BackendChoice::Vllm,
        BackendArg::Gemini => BackendChoice::Gemini,
        BackendArg::Mock | BackendArg::Replay => BackendChoice::Mock,
    };
    s
}

fn config(s: &Settings) -> Config {
    let (max_steps, max_time) = s.limits();
    Config {
        max_steps,
        max_time,
        approval: s.approval,
        ..Config::default()
    }
}

/// Built-in skills next to the binary or in the repo, plus the user's own.
fn all_skills() -> Vec<Skill> {
    let mut v = Vec::new();
    for dir in ["skills", "../skills", "../../skills"] {
        let p = Path::new(dir);
        if p.is_dir() {
            v.extend(kai_core::skill::load_dir(p, true));
            break;
        }
    }
    v.extend(kai_core::skill::load_dir(&Settings::skills_dir(), false));
    v
}

#[tokio::main]
async fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    match run(Cli::parse()).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.cmd {
        Cmd::Skill {
            cmd: SkillCmd::Validate { dirs },
        } => Ok(validate(&dirs)),
        Cmd::Skill {
            cmd: SkillCmd::List,
        } => {
            for s in all_skills() {
                let tag = if s.builtin { "built-in" } else { "yours" };
                println!("{:<28} {:<9} {}", s.front.name, tag, s.front.description);
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Key { which } => {
            eprint!("Paste the {which} API key and press Enter: ");
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            let name = if which == "gemini" {
                kai_os::secrets::GEMINI
            } else {
                kai_os::secrets::VLLM
            };
            kai_os::secrets::set(name, &line)?;
            eprintln!("Saved to the OS keychain.");
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Doctor => {
            let checks = kai_os::permissions::check_all().await;
            for c in &checks {
                println!(
                    "{:<20} {:?}\n  why: {}\n  fix: {}",
                    c.name, c.status, c.why, c.fix
                );
            }
            Ok(if kai_os::permissions::all_granted(&checks) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Cmd::Smoke { backend, n } => smoke(settings_for(backend), n).await,
        Cmd::Run {
            task,
            backend,
            team,
            yes,
        } => run_task(settings_for(backend), &task, team, yes).await,
        Cmd::Bench {
            backend,
            tasks,
            out,
            only,
            yes,
        } => {
            let mut list = bench::load_tasks(&tasks)?;
            if !only.is_empty() {
                list.retain(|t| only.contains(&t.id));
            }
            let skills = all_skills();
            let s = settings_for(backend);
            let (name, runs, mock) = if matches!(backend, BackendArg::Replay) {
                let live = bench::Live {
                    client: None,
                    careful: None,
                    skills: &skills,
                    config: config(&s),
                    auto_yes: yes,
                };
                ("replay", bench::run_live(&list, &live).await?, false)
            } else if s.backend == BackendChoice::Mock {
                ("mock", bench::run_mock(&list, &skills).await, true)
            } else {
                let (fast, careful) = s.clients(&kai_os::secrets::keys())?;
                let live = bench::Live {
                    client: Some(fast.as_ref()),
                    careful: careful.as_deref(),
                    skills: &skills,
                    config: config(&s),
                    auto_yes: yes,
                };
                let name = match s.backend {
                    BackendChoice::Vllm => "vllm",
                    BackendChoice::Gemini => "gemini",
                    _ => "auto",
                };
                (name, bench::run_live(&list, &live).await?, false)
            };
            let summary = bench::summarise(name, runs, mock).await;
            bench::print(&summary);
            let path = bench::write(&summary, &out)?;
            eprintln!("wrote {}", path.display());
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn validate(dirs: &[PathBuf]) -> ExitCode {
    let mut targets = Vec::new();
    for d in dirs {
        if d.join("SKILL.md").is_file() {
            targets.push(d.clone());
        } else if let Ok(rd) = std::fs::read_dir(d) {
            let mut sub: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            sub.sort();
            targets.extend(sub);
        } else {
            targets.push(d.clone());
        }
    }
    let mut bad = 0;
    for t in &targets {
        match kai_core::skill::load(t) {
            Ok(s) => println!(
                "ok    {} ({} deterministic steps)",
                s.front.name,
                s.steps.len()
            ),
            Err(e) => {
                bad += 1;
                println!("FAIL  {}: {e}", t.display());
            }
        }
    }
    println!("{} checked, {} failed", targets.len(), bad);
    if bad == 0 && !targets.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

async fn smoke(s: Settings, n: u32) -> anyhow::Result<ExitCode> {
    let (client, _) = s.clients(&kai_os::secrets::keys())?;
    let screen = kai_os::DesktopPerceptor::new();
    let obs = kai_core::Perceptor::observe(&screen).await?;
    let mut times = Vec::new();
    for i in 0..n.max(1) {
        let r = client.warm_up(&obs.images).await?;
        eprintln!(
            "  call {}: {} ms via {}",
            i + 1,
            r.total.as_millis(),
            r.backend.label()
        );
        times.push(r.total.as_millis() as u64);
    }
    times.sort_unstable();
    let p = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];
    println!(
        "p50 {} ms · p95 {} ms over {} calls",
        p(0.5),
        p(0.95),
        times.len()
    );
    Ok(ExitCode::SUCCESS)
}

async fn run_task(s: Settings, task: &str, team: bool, yes: bool) -> anyhow::Result<ExitCode> {
    let (fast, careful) = s.clients(&kai_os::secrets::keys())?;
    let skills = all_skills();
    let kill = kai_os::KillSwitch::new();
    let _hotkey = kai_os::kill::install_hotkey(&kill, &s.kill_hotkey)
        .map(|h| {
            eprintln!(
                "Stop any time: {} (or move the pointer to the top-left corner)",
                h.shortcut
            )
        })
        .map_err(|e| log::warn!("no global stop hotkey: {e}"));
    let screen = kai_os::DesktopPerceptor::new();
    let hands = kai_os::DesktopActuator::new(kill.clone())?;
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    tokio::spawn(kai_os::kill::watch_corner(
        kill.clone(),
        hands.input.clone(),
        running.clone(),
    ));
    let approver = TerminalApprover { auto_yes: yes };

    let ok = if team {
        let orch = kai_core::orchestrator::Orchestrator {
            client: fast.as_ref(),
            careful: careful.as_deref(),
            screen: &screen,
            actuator: &hands,
            approver: &approver,
            interrupt: &kill,
            sink: &Printer,
            skills: &skills,
            config: config(&s),
            parallelism: s.parallelism,
        };
        let report = orch.run(task).await?;
        for r in &report.results {
            eprintln!(
                "  sub-agent {} ({:?}): {:?}",
                r.subtask.id, r.subtask.kind, r.trace.outcome
            );
        }
        println!("\n{}", report.summary);
        report.outcome() == kai_core::Outcome::Success
    } else {
        let engine = Engine {
            client: fast.as_ref(),
            careful: careful.as_deref(),
            perceptor: &screen,
            actuator: &hands,
            approver: &approver,
            interrupt: &kill,
            sink: &Printer,
            skills: &skills,
            config: config(&s),
        };
        let trace = engine.run(task).await;
        save_trace(&trace);
        let (p50, p95) = trace.step_percentiles();
        println!(
            "{:?} in {} steps · step p50 {p50} ms · p95 {p95} ms",
            trace.outcome,
            trace.steps.len()
        );
        trace.outcome == Some(kai_core::Outcome::Success)
    };
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn save_trace(trace: &kai_core::Trace) {
    let dir = Settings::traces_dir();
    let name = format!("{}.json", jiff::Timestamp::now().as_second());
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| {
        std::fs::write(
            dir.join(&name),
            serde_json::to_string_pretty(trace).unwrap_or_default(),
        )
    }) {
        log::warn!("could not save trace: {e}");
    }
}
