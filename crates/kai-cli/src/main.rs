//! `kai`: terminal runs, skill validation and benchmarks; shares kai-core with kai-app.

mod ab;
mod backend;
mod bench;
mod console;
mod data;
mod mcp;
mod memory;
mod models;
mod runs;

use backend::{BackendArg, BackendOpts};
use clap::{Parser, Subcommand};
use console::{Printer, TerminalApprover, TerminalHuman};
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
        #[command(flatten)]
        lane: runs::LaneArgs,
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
        /// Boxed: far larger than the other variants (`large_enum_variant`
        /// on Windows, where `PathBuf` is bigger).
        #[command(flatten)]
        args: Box<bench::BenchArgs>,
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
    /// Serve Kai's eyes and hands over MCP on stdio (docs/MCP.md).
    Mcp {
        #[command(flatten)]
        backend: BackendOpts,
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

/// Built-in skills in the repo or next to the binary (in a macOS app
/// bundle, its `Resources`), plus the user's own.
fn all_skills() -> Vec<Skill> {
    let mut v = Vec::new();
    let exe_dir = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let near_exe = ["skills", "../Resources/skills"]
        .into_iter()
        .filter_map(|d| exe_dir.as_ref().map(|e| e.join(d)));
    let found = ["skills", "../skills", "../../skills"]
        .into_iter()
        .map(PathBuf::from)
        .chain(near_exe)
        .find(|p| p.is_dir());
    if let Some(p) = found {
        v.extend(kai_core::skill::load_dir(&p, true));
    }
    v.extend(kai_core::skill::load_dir(&Settings::skills_dir(), false));
    v
}

#[tokio::main]
async fn main() -> ExitCode {
    console::init();
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
        Cmd::Mcp { backend } => mcp::serve(&backend).await,
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
            lane,
        } => {
            run_task(
                &backend,
                &task,
                team,
                yes,
                record.as_deref(),
                !no_memory,
                &lane,
            )
            .await
        }
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
        Cmd::Bench { cmd: None, args } => bench::run(*args, &all_skills()).await,
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

async fn smoke(o: &BackendOpts, n: u32) -> anyhow::Result<ExitCode> {
    let (b, mut s, keys) = live_settings(o)?;
    let _local = runs::local_server(b, &mut s).await?;
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
    lane: &runs::LaneArgs,
) -> anyhow::Result<ExitCode> {
    let (b, mut s, keys) = live_settings(o)?;
    let _local = runs::local_server(b, &mut s).await?;
    let recorder = match record {
        Some(root) if team => {
            eprintln!("--record is not supported with --team; not recording");
            let _ = root;
            None
        }
        Some(root) => Some(runs::run_recorder(root, task, b, &s)?),
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
    let _hotkey = if kai_os::kill::HOTKEY_WITHOUT_EVENT_LOOP {
        kai_os::kill::install_hotkey(&kill, &s.kill_hotkey)
            .inspect(|h| {
                eprintln!(
                    "Stop any time: {} (or move the pointer to the top-left corner)",
                    h.shortcut
                )
            })
            .map_err(|e| log::warn!("no global stop hotkey: {e}"))
            .ok()
    } else {
        eprintln!("Stop any time: move the pointer to the top-left corner");
        None
    };
    // `--browser`: Kai's own browser is both the eyes and the hands.
    let web = lane.open(&kill).await?;
    let desktop = match web {
        Some(_) => None,
        None => Some(kai_os::DesktopActuator::new(kill.clone())?),
    };
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    if let Some(h) = &desktop {
        tokio::spawn(kai_os::kill::watch_corner(
            kill.clone(),
            h.input.clone(),
            running.clone(),
        ));
    }
    let desktop_eyes = kai_os::DesktopPerceptor::new();
    let (screen, hands): (&dyn kai_core::Perceptor, &dyn kai_core::Actuator) =
        match (&web, &desktop) {
            (Some(l), _) => (l, l),
            (None, Some(h)) => (&desktop_eyes, h),
            (None, None) => unreachable!("desktop hands exist when there is no lane"),
        };
    let approver = TerminalApprover { auto_yes: yes };
    // CAPTCHAs, one-time codes and questions reach whoever is at this
    // terminal; piped input means nobody is, so those end the run instead.
    let human = std::io::IsTerminal::is_terminal(&std::io::stdin()).then_some(TerminalHuman);

    let ok = if let Some(cfg) = native {
        let runner = kai_core::cua::CuaRunner {
            config: cfg,
            perceptor: screen,
            actuator: hands,
            interrupt: &kill,
            sink: &Printer,
        };
        let run = runner.run_with_approver(task, Some(&approver)).await?;
        if let Some(e) = &run.error {
            eprintln!("native loop error: {e}");
        }
        memory::save_trace(&run.trace);
        if let Some(rec) = &recorder {
            rec.finish(&run.trace)?;
        }
        run.trace.outcome == Some(kai_core::Outcome::Success)
    } else if team {
        let orch = kai_core::orchestrator::Orchestrator {
            client: fast.as_ref(),
            careful: careful.as_deref(),
            screen,
            actuator: hands,
            approver: &approver,
            interrupt: &kill,
            sink: &Printer,
            steering: &kai_core::engine::NoSteering,
            human: human.as_ref().map(|h| h as &dyn kai_core::HumanHelper),
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
            perceptor: screen,
            actuator: hands,
            approver: &approver,
            interrupt: &kill,
            sink: &Printer,
            steering: &kai_core::engine::NoSteering,
            verifier: None,
            human: human.as_ref().map(|h| h as &dyn kai_core::HumanHelper),
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
        let procs = (o.ab.procedures)
            .then(|| kai_core::ProcedureStore::load(Settings::dir().join("procedures")).ok())
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
                    probe: Some(&kai_os::DesktopProbe),
                    procedures: procs.as_ref(),
                },
            )
            .await;
        memory::save_trace(&trace);
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
