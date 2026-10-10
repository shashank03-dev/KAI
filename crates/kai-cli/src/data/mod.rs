//! `kai data`: turn recorded runs (`kai-trajectory/1`, see `docs/DATASET.md`)
//! into training files, and summarise what has been recorded.

pub mod export;
pub mod stats;
#[cfg(test)]
mod tests;

use clap::{Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;

/// What `kai data export` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// One object per model call, every recorded run (filters apply).
    All,
    /// One object per model call, only runs that passed (check true, or
    /// outcome success when there is no check). For train/sft_gemma.py.
    Sft,
    /// One object per task, with setup/check from the task files, for an RL
    /// environment (train/grpo_env.py).
    Grpo,
}

#[derive(Subcommand, Debug)]
pub enum DataCmd {
    /// Write recorded runs as JSONL training data.
    Export {
        /// Folder holding recorded runs (searched recursively).
        #[arg(long = "in", default_value = "bench/runs")]
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Only runs that passed.
        #[arg(long)]
        only_success: bool,
        /// Only runs whose automatic check was this value.
        #[arg(long)]
        min_check: Option<bool>,
        #[arg(long, value_enum, default_value = "all")]
        format: Format,
        /// Task files to read setup/check from (grpo). Default:
        /// bench/tasks.yaml and bench/variants.yaml when they exist.
        #[arg(long)]
        tasks: Vec<PathBuf>,
    },
    /// Runs, steps, success rate and failure categories of recorded runs.
    Stats {
        #[arg(long = "in", default_value = "bench/runs")]
        input: PathBuf,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: DataCmd) -> anyhow::Result<ExitCode> {
    match cmd {
        DataCmd::Export {
            input,
            out,
            only_success,
            min_check,
            format,
            tasks,
        } => {
            let tasks = if tasks.is_empty() {
                ["bench/tasks.yaml", "bench/variants.yaml"]
                    .iter()
                    .map(PathBuf::from)
                    .filter(|p| p.is_file())
                    .collect()
            } else {
                tasks
            };
            let filter = export::Filter {
                only_success,
                min_check,
            };
            let n = export::export(&input, &out, format, filter, &tasks)?;
            eprintln!("wrote {n} records to {}", out.display());
            Ok(ExitCode::SUCCESS)
        }
        DataCmd::Stats { input, json } => {
            let s = stats::stats(&input)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&s)?);
            } else {
                print!("{}", stats::text(&s));
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
