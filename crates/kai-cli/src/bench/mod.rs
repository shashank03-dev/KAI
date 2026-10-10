//! `kai bench`: run the tasks in `bench/tasks.yaml` the same way on each
//! backend and write `bench/results/<date>-<backend>-<model>.{json,md}`;
//! `kai bench report` merges those files into one head-to-head table.
//!
//! The `mock` backend plays each task's recorded replies against a scripted
//! screen and a dry-run actuator: it measures the harness itself, not a model,
//! and touches nothing. `replay` runs the same replies for real on this
//! desktop and judges them with each task's check command.

#[cfg(test)]
mod executor_tests;
pub mod machine;
pub mod record;
pub mod repeat;
pub mod report;
pub mod runner;
pub mod stats;
pub mod tasks;
#[cfg(test)]
mod tests;
pub mod variants;

use crate::backend::{self, BackendArg, BackendOpts};
use clap::{Args, Subcommand};
use machine::{AbRecord, Machine};
use stats::{Meta, Prices};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tasks::Kind;

#[derive(Args, Debug)]
pub struct BenchArgs {
    #[command(flatten)]
    pub backend: BackendOpts,
    #[arg(long, default_value = "bench/tasks.yaml")]
    pub tasks: PathBuf,
    #[arg(long, default_value = "bench/results")]
    pub out: PathBuf,
    /// Only these task ids (repeatable).
    #[arg(long)]
    pub only: Vec<String>,
    /// Only tasks of this kind (repeatable): terminal, gui, mixed.
    #[arg(long, value_enum)]
    pub kind: Vec<Kind>,
    /// Run tasks even when this machine lacks what they need.
    #[arg(long)]
    pub force: bool,
    /// Approve everything except passwords and payments.
    #[arg(long)]
    pub yes: bool,
    /// Input price in US$ per million tokens, for the cost column.
    #[arg(long)]
    pub usd_per_mtok_in: Option<f64>,
    /// Output price in US$ per million tokens, for the cost column.
    #[arg(long)]
    pub usd_per_mtok_out: Option<f64>,
    /// Record every task run as training data under
    /// `<runs>/<date>-<backend>-<model>/<task-id>/` (docs/DATASET.md).
    #[arg(long, default_value = "bench/runs")]
    pub runs: PathBuf,
    /// Do not record task runs.
    #[arg(long)]
    pub no_record: bool,
    /// Use experience memory (A/B): a fresh lesson store for this bench run
    /// that earlier tasks teach later ones. Off by default for fair numbers;
    /// the user's own lessons are never used.
    #[arg(long)]
    pub memory: bool,
    /// Run each task this many times back to back (with `--memory`, later
    /// runs can use the first run's lessons); prints first vs repeat.
    #[arg(long, default_value = "1")]
    pub repeat: u32,
}

impl clap::ValueEnum for Kind {
    fn value_variants<'a>() -> &'a [Self] {
        &[Kind::Terminal, Kind::Gui, Kind::Mixed]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(match self {
            Kind::Terminal => "terminal",
            Kind::Gui => "gui",
            Kind::Mixed => "mixed",
        }))
    }
}

#[derive(Subcommand, Debug)]
pub enum BenchCmd {
    /// Merge results files into one head-to-head Markdown table.
    Report {
        #[arg(long, default_value = "bench/results")]
        dir: PathBuf,
        /// Write here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write parameterised variants of the tasks that declare `vars:` (for
    /// training data only, never for the head-to-head numbers).
    Variants {
        #[arg(long, default_value = "bench/tasks.yaml")]
        tasks: PathBuf,
        #[arg(long, default_value = "bench/variants.yaml")]
        out: PathBuf,
        /// Variants per task (at most the number of value combinations - 1).
        #[arg(long, default_value = "3")]
        n: usize,
    },
}

pub fn report(dir: &Path, out: Option<&Path>) -> anyhow::Result<ExitCode> {
    let runs = report::load_latest(dir)?;
    let md = report::merged_markdown(&runs);
    match out {
        Some(p) => {
            std::fs::write(p, &md)?;
            eprintln!("wrote {} ({} results files)", p.display(), runs.len());
        }
        None => print!("{md}"),
    }
    Ok(ExitCode::SUCCESS)
}

fn note(b: BackendArg) -> String {
    match b {
        BackendArg::Replay => "Replay: each task's recorded replies executed for real on this desktop (real capture, accessibility tree, input and shell), judged by the task's check command. Proves Kai's hands and the tasks' setup/check, not a model. Tasks marked `replay: terminal` reach the end state from the terminal; `replay: skip` tasks are skipped.".into(),
        BackendArg::Mock => "Mock backend: recorded replies on a scripted screen with a dry-run actuator. Measures harness overhead only. No model was called and nothing on the machine was touched.".into(),
        _ => "Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.".into(),
    }
}

pub async fn run(args: BenchArgs, skills: &[kai_core::Skill]) -> anyhow::Result<ExitCode> {
    let b = args.backend.backend.unwrap_or(BackendArg::Mock);
    let mut list = tasks::load_tasks(&args.tasks)?;
    if !args.only.is_empty() {
        for id in &args.only {
            anyhow::ensure!(list.iter().any(|t| &t.id == id), "no task with id {id}");
        }
        list.retain(|t| args.only.contains(&t.id));
    }
    if !args.kind.is_empty() {
        list.retain(|t| args.kind.contains(&t.kind));
    }
    anyhow::ensure!(!list.is_empty(), "no tasks selected");
    let bench_dir = std::fs::canonicalize(&args.tasks)?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    let s = backend::settings_for(b, args.backend.model.as_deref())?;
    let (config, applied) = backend::config(&s, &args.backend.ab)?;
    let keys = kai_os::secrets::keys();
    let prices = match (args.usd_per_mtok_in, args.usd_per_mtok_out) {
        (None, None) => None,
        (i, o) => Some(Prices {
            input_per_mtok: i.unwrap_or(0.0),
            output_per_mtok: o.unwrap_or(0.0),
        }),
    };

    let backend_name = match b {
        // Name what `auto` resolved to.
        BackendArg::Auto => format!("{:?}", s.backend).to_lowercase(),
        _ => b.name().to_string(),
    };
    let model = backend::model_label(b, &s);
    let date = jiff::Zoned::now().date().to_string();
    let recording = (!args.no_record).then(|| {
        let mut r = record::Recording::new(&args.runs, &date, &backend_name, &model);
        if !b.recorded() && !s.careful_model.is_empty() && s.careful_model != s.fast_model {
            r.careful_model = Some(s.careful_model.clone());
        }
        r
    });
    if let Some(r) = &recording {
        eprintln!("recording runs under {}", r.batch_dir.display());
    }
    let rec = recording.as_ref();
    // The executor's decision model (jevos), only with `--executor`. Each
    // run health-checks it and runs without it when it is not there.
    let jevos = config.executor.then(|| s.executor_client());
    if let Some(j) = &jevos {
        eprintln!("executor: jevos at {}", j.base());
    }
    let executor = jevos
        .as_ref()
        .map(|j| j as &dyn kai_core::executor::Decider);

    let store = args.memory.then(kai_core::MemoryStore::in_memory);
    let mem = runner::MemoryOpts {
        store: store.as_ref(),
        repeat: args.repeat.clamp(1, 5),
    };
    let runs = match b {
        BackendArg::Mock => runner::run_mock_with(&list, skills, config, rec, executor, mem).await,
        BackendArg::Replay => {
            let live = runner::Live {
                client: None,
                careful: None,
                skills,
                config,
                auto_yes: args.yes,
                bench_dir: &bench_dir,
                force: args.force,
                native: None,
                record: rec,
                executor,
                memory: mem,
            };
            runner::run_live(&list, &live).await?
        }
        BackendArg::OpenaiNative => {
            let cfg = backend::native_config(&keys, args.backend.model.as_deref(), &config)?;
            let live = runner::Live {
                client: None,
                careful: None,
                skills,
                config,
                auto_yes: args.yes,
                bench_dir: &bench_dir,
                force: args.force,
                native: Some(cfg),
                record: rec,
                executor,
                memory: mem,
            };
            runner::run_live(&list, &live).await?
        }
        _ => {
            backend::require_key(b, &keys)?;
            let (fast, careful) = s.clients(&keys)?;
            let live = runner::Live {
                client: Some(fast.as_ref()),
                careful: careful.as_deref(),
                skills,
                config,
                auto_yes: args.yes,
                bench_dir: &bench_dir,
                force: args.force,
                native: None,
                record: rec,
                executor,
                memory: mem,
            };
            runner::run_live(&list, &live).await?
        }
    };

    let repeat_md = (mem.repeat > 1 || args.memory)
        .then(|| repeat::table(&runs, store.as_ref().map_or(0, |s| s.len())));
    // The head-to-head numbers are first runs only.
    let runs: Vec<runner::Run> = runs.into_iter().filter(|r| r.pass == 0).collect();
    let mut bench_note = note(b);
    if args.memory {
        bench_note.push_str(" Experience memory ON (--memory): a fresh lesson store shared by the tasks of this bench run.");
    }
    let meta = Meta {
        backend: backend_name,
        model,
        note: bench_note,
        command: machine::command_line(),
        machine: Some(Machine::probe()),
        ab: Some(AbRecord {
            flags: args.backend.ab,
            applied,
        }),
        prices,
        recorded: b.recorded(),
        replay: b == BackendArg::Replay,
        date,
    };
    let summary = stats::summarise(meta, runs);
    report::print(&summary);
    // Variants are training data, never head-to-head numbers: keep their
    // results out of the folder `kai bench report` reads.
    let out = if list.iter().any(|t| t.variant_of.is_some()) {
        args.out.join("variants")
    } else {
        args.out.clone()
    };
    let (json, md) = report::write(&summary, &out)?;
    eprintln!("wrote {} and {}", json.display(), md.display());
    if let Some(t) = repeat_md {
        println!("\n{t}");
        let p = md.with_extension("memory.md");
        std::fs::write(&p, t)?;
        eprintln!("wrote {}", p.display());
    }
    Ok(ExitCode::SUCCESS)
}
