//! `kai` — run Gemma 4 on your computer from the terminal, validate skills and
//! run the benchmark. The desktop app is `kai-app`; both share kai-core.

mod ab;
mod backend;
mod bench;
mod console;
mod data;
mod memory;
mod models;

use backend::{BackendArg, BackendOpts};
use clap::{Parser, Subcommand};
use console::{Printer, TerminalApprover};
use kai_core::settings::Settings;
use kai_core::{Engine, Skill};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "kai", version, about = "Kai: Gemma 4's hands on your computer")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Do a task on this computer, step by step, asking before risky actions.
    Run {
        task: String,
        #[command(flatten)]
        backend: BackendOpts,
        /// Split the task across parallel sub-agents.
        #[arg(long)]
        team: bool,
        /// Approve everything except passwords and payments.
        #[arg(long)]
        yes: bool,
        /// Record this run as training data under this folder
        /// (`<dir>/<time>-<task>/`, see docs/DATASET.md). Screenshots can
        /// hold private data; recording stays on this machine.
        #[arg(long, value_name = "DIR")]
        record: Option<PathBuf>,
        /// Neither use nor add to what Kai learned on this computer.
        #[arg(long)]
        no_memory: bool,
    },
    /// What Kai learned from earlier runs on this computer (docs/MEMORY.md).
    Memory {
        #[command(subcommand)]
        cmd: memory::MemoryCmd,
    },
    /// Training data: export recorded runs, or summarise them.
    Data {
        #[command(subcommand)]
        cmd: data::DataCmd,
    },
    /// Skill tools.
    Skill {
        #[command(subcommand)]
        cmd: SkillCmd,
    },
    /// Run bench/tasks.yaml and write bench/results/<date>-<backend>-<model>.{json,md};
    /// `kai bench report` merges results into a head-to-head table.
    #[command(args_conflicts_with_subcommands = true)]
    Bench {
        #[command(subcommand)]
        cmd: Option<bench::BenchCmd>,
        #[command(flatten)]
        args: bench::BenchArgs,
    },
    /// Send one real screenshot to the model and report the latency.
    Smoke {
        #[command(flatten)]
        backend: BackendOpts,
        #[arg(long, default_value = "5")]
        n: u32,
    },
    /// Check screen, accessibility and input permissions, and which backends have keys.
    Doctor,
    /// Open models on this computer: list, install, start, stop, remove.
    Models {
        #[command(subcommand)]
        cmd: models::ModelsCmd,
    },
    /// Store an API key in the OS keychain: `kai key gateway` (reads stdin).
    Key {
        #[arg(value_parser = ["gemini", "vllm", "gateway", "openai"])]
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
            let name = match which.as_str() {
                "gemini" => kai_os::secrets::GEMINI,
                "gateway" => kai_os::secrets::GATEWAY,
                "openai" => kai_os::secrets::OPENAI,
                _ => kai_os::secrets::VLLM,
            };
            anyhow::ensure!(!line.trim().is_empty(), "no key given; nothing saved");
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
            println!("\nmodel backends:");
            for line in backend::key_report() {
                println!("  {line}");
            }
            println!("\nexecutor (local decision model, `--executor`):");
            println!("  {}", backend::executor_report(&Settings::load()).await);
            println!("\nlocal models (`kai models`):");
            for line in models::doctor_report().await {
                println!("  {line}");
            }
            Ok(if kai_os::permissions::all_granted(&checks) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Cmd::Models { cmd } => models::run(cmd).await,
        Cmd::Smoke { backend, n } => smoke(&backend, n).await,
        Cmd::Run {
            task,
            backend,
            team,
            yes,
            record,
            no_memory,
        } => run_task(&backend, &task, team, yes, record.as_deref(), !no_memory).await,
        Cmd::Memory { cmd } => memory::run(cmd),
        Cmd::Data { cmd } => data::run(cmd),
        Cmd::Bench {
            cmd: Some(bench::BenchCmd::Report { dir, out }),
            ..
        } => bench::report(&dir, out.as_deref()),
        Cmd::Bench {
            cmd: Some(bench::BenchCmd::Variants { tasks, out, n }),
            ..
        } => bench::variants::write(&tasks, &out, n),
        Cmd::Bench { cmd: None, args } => bench::run(args, &all_skills()).await,
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

/// Settings and keys for a live (model-calling) command.
fn live_settings(
    o: &BackendOpts,
) -> anyhow::Result<(BackendArg, Settings, kai_core::settings::Keys)> {
    let b = o.backend.unwrap_or(BackendArg::Auto);
    anyhow::ensure!(
        !b.recorded(),
        "--backend {} is for `kai bench` only",
        b.name()
    );
    let s = backend::settings_for(b, o.model.as_deref())?;
    let keys = kai_os::secrets::keys();
    if b != BackendArg::OpenaiNative {
        backend::require_key(b, &keys)?;
    }
    Ok((b, s, keys))
}

/// `--backend local`: the model's server, started for this run if needed.
async fn local_server(
    b: BackendArg,
    s: &mut Settings,
) -> anyhow::Result<Option<kai_os::local::LocalServer>> {
    if b == BackendArg::Local
        || (b == BackendArg::Auto && s.backend == kai_core::settings::BackendChoice::Local)
    {
        models::ensure_local(s).await
    } else {
        Ok(None)
    }
}

async fn smoke(o: &BackendOpts, n: u32) -> anyhow::Result<ExitCode> {
    let (b, mut s, keys) = live_settings(o)?;
    let _local = local_server(b, &mut s).await?;
    let (client, _) = s.clients(&keys)?;
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

async fn run_task(
    o: &BackendOpts,
    task: &str,
    team: bool,
    yes: bool,
    record: Option<&Path>,
    use_memory: bool,
) -> anyhow::Result<ExitCode> {
    let (b, mut s, keys) = live_settings(o)?;
    let _local = local_server(b, &mut s).await?;
    let recorder = match record {
        Some(root) if team => {
            eprintln!("--record is not supported with --team; not recording");
            let _ = root;
            None
        }
        Some(root) => Some(run_recorder(root, task, b, &s)?),
        None => None,
    };
    let (config, _) = backend::config(&s, &o.ab)?;
    let native = match b {
        BackendArg::OpenaiNative => {
            Some(backend::native_config(&keys, o.model.as_deref(), &config)?)
        }
        _ => None,
    };
    let (fast, careful) = if native.is_some() {
        (backend::placeholder_client(), None)
    } else {
        s.clients(&keys)?
    };
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

    let ok = if let Some(cfg) = native {
        let runner = kai_core::cua::CuaRunner {
            config: cfg,
            perceptor: &screen,
            actuator: &hands,
            interrupt: &kill,
            sink: &Printer,
        };
        let run = runner.run(task).await?;
        if let Some(e) = &run.error {
            eprintln!("native loop error: {e}");
        }
        save_trace(&run.trace);
        if let Some(rec) = &recorder {
            rec.finish(&run.trace)?;
        }
        run.trace.outcome == Some(kai_core::Outcome::Success)
    } else if team {
        let orch = kai_core::orchestrator::Orchestrator {
            client: fast.as_ref(),
            careful: careful.as_deref(),
            screen: &screen,
            actuator: &hands,
            approver: &approver,
            interrupt: &kill,
            sink: &Printer,
            steering: &kai_core::engine::NoSteering,
            skills: &skills,
            config,
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
            steering: &kai_core::engine::NoSteering,
            verifier: None,
            skills: &skills,
            config,
        };
        // The executor (`--executor`): jevos carries out the model's plans.
        let jevos = config.executor.then(|| s.executor_client());
        // The plan cache replays the known-good opening of a repeated task.
        let cache =
            o.ab.plan_cache()
                .then(|| kai_core::PlanCache::load(Settings::dir().join("plans")).ok())
                .flatten();
        // Experience memory: lessons from earlier runs, and learning from this one.
        let memory = use_memory
            .then(|| kai_core::MemoryStore::load(Settings::memory_dir()))
            .and_then(|m| m.map_err(|e| log::warn!("memory off: {e}")).ok());
        let trace = engine
            .run_with(
                task,
                kai_core::RunOptions {
                    plan_cache: cache.as_ref(),
                    recorder: recorder.as_ref(),
                    executor: jevos
                        .as_ref()
                        .map(|j| j as &dyn kai_core::executor::Decider),
                    memory: memory.as_ref(),
                    learn_later: false,
                },
            )
            .await;
        save_trace(&trace);
        if let Some(rec) = &recorder {
            eprintln!("recorded to {}", rec.dir().display());
        }
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

/// A recorder for `kai run --record <root>`: `<root>/<time>-<task slug>/`.
fn run_recorder(
    root: &Path,
    task: &str,
    b: BackendArg,
    s: &Settings,
) -> anyhow::Result<kai_core::TraceRecorder> {
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    let slug: String = bench::report::slug(task).chars().take(40).collect();
    let run_id = format!("{stamp}-{}", slug.trim_end_matches('-'));
    let backend = match b {
        BackendArg::Auto => format!("{:?}", s.backend).to_lowercase(),
        _ => b.name().to_string(),
    };
    let mut meta = kai_core::RunMeta::new(&run_id, task, "run", backend, &s.fast_model);
    meta.kai_version = bench::record::kai_version();
    let mut rec = kai_core::TraceRecorder::new(root.join(&run_id), meta)?;
    if s.careful_model != s.fast_model && !s.careful_model.is_empty() {
        rec = rec.with_careful_model(&s.careful_model);
    }
    Ok(rec)
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
