//! `kai bench`: run the tasks in `bench/tasks.yaml` the same way on each
//! backend and write `bench/results/<date>-<backend>-<model>.{json,md}`;
//! `kai bench report` merges those files into one head-to-head table.
//!
//! The `mock` backend plays each task's recorded replies against a scripted
//! screen and a dry-run actuator: it measures the harness itself, not a model,
//! and touches nothing. `replay` runs the same replies for real on this
//! desktop and judges them with each task's check command.

pub mod machine;
pub mod report;
pub mod runner;
pub mod stats;
pub mod tasks;
#[cfg(test)]
mod tests;

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

    let runs = match b {
        BackendArg::Mock => runner::run_mock(&list, skills, config).await,
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
            };
            runner::run_live(&list, &live).await?
        }
    };

    let meta = Meta {
        backend: match b {
            // Name what `auto` resolved to.
            BackendArg::Auto => format!("{:?}", s.backend).to_lowercase(),
            _ => b.name().to_string(),
        },
        model: backend::model_label(b, &s),
        note: note(b),
        command: machine::command_line(),
        machine: Some(Machine::probe()),
        ab: Some(AbRecord {
            flags: args.backend.ab,
            applied,
        }),
        prices,
        recorded: b.recorded(),
        replay: b == BackendArg::Replay,
        date: jiff::Zoned::now().date().to_string(),
    };
    let summary = stats::summarise(meta, runs);
    report::print(&summary);
    let (json, md) = report::write(&summary, &args.out)?;
    eprintln!("wrote {} and {}", json.display(), md.display());
    Ok(ExitCode::SUCCESS)
}
