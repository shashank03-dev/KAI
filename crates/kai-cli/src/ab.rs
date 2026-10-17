//! A/B switches for engine features, shared by `kai run` and `kai bench`.
//!
//! This is the ONE place that maps CLI flags onto `kai_core::Config`. The
//! engine features (multi-action batches, tree-only steps, accessibility-first
//! actions, the plan cache) are being added to `Config` in parallel; until the
//! fields exist here, turning a feature off is refused with a clear error
//! rather than silently ignored.

use clap::Args;
use kai_core::Config;
use serde::{Deserialize, Serialize};

/// Every speed switch defaults to "feature on" and its flag turns it off;
/// the executor is the exception (off until measured; `--executor` turns it
/// on).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Args, Serialize, Deserialize)]
#[serde(default)]
pub struct Ab {
    /// Expose the experimental shared computer tools to local models.
    #[arg(long)]
    pub computer_tools_v1: bool,
    /// Open models read and write OpenAI's computer-tool contract, with
    /// coordinates in this frame: marks, pixels, norm1000, smart-resize, or
    /// auto (the model's documented frame from the catalog)
    /// (`docs/RESEARCH_2026_10_OPENAI_PARITY.md` §5b). Implies the tools.
    #[arg(long, value_name = "FRAME", value_parser = frame)]
    pub oai_contract: Option<OaiFrame>,
    /// Decode native GUI actions: Fara 1.5, UI-TARS 1.5, or Holo3.1 function calls.
    #[arg(long, value_name = "MODEL", value_parser = native_model)]
    pub native_model: Option<kai_core::native_models::NativeModel>,
    /// Kai's own OpenAI-shaped safety checks: ask before acting on a site
    /// the task does not name, or on a sensitive one (mail, money, identity).
    #[arg(long)]
    pub local_safety_checks: bool,
    /// Most actions per turn on the OpenAI contract (default 1); later
    /// actions run only while the screen layout holds.
    #[arg(long, value_name = "K", default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=16))]
    pub oai_batch: u8,
    /// Replies sampled per step on the OpenAI contract; the largest group
    /// of agreeing replies wins and only its action runs. Costs K model calls.
    #[arg(long, value_name = "K", default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=8))]
    pub step_candidates: u8,
    /// Resolve ties between equally sized candidate clusters: deterministic
    /// first cluster (default), or ask the model to judge the tied actions.
    #[arg(long, value_enum, default_value_t = StepJudge::Cluster)]
    pub step_judge: StepJudge,
    /// Disable multi-action steps (one action per model call).
    #[arg(long)]
    pub no_batch: bool,
    /// Disable tree-only steps (always send a screenshot).
    #[arg(long)]
    pub no_tree_only: bool,
    /// Disable accessibility-first actions (always use pointer and keyboard).
    #[arg(long)]
    pub no_a11y: bool,
    /// Disable the plan cache.
    #[arg(long)]
    pub no_plan_cache: bool,
    /// Turn on the executor: a local decision model (jevos-v4 at the
    /// settings' `executor_endpoint`) carries out the model's plan
    /// (docs/EXECUTOR.md).
    #[arg(long)]
    pub executor: bool,
    /// Turn on the outcome ledger and DONE gate: the first call names
    /// required outcomes with read-only probes, and `done` is accepted only
    /// when they hold (docs/VERIFICATION.md).
    #[arg(long)]
    pub outcomes: bool,
    /// Summarise steps older than the last 2 for the model instead of
    /// dropping them (macro-action history).
    #[arg(long)]
    pub macro_history: bool,
    /// Replay remembered procedures for known tasks and record new ones
    /// (docs/PROCEDURES.md).
    #[arg(long)]
    pub procedures: bool,
    /// For long tasks, sample this many plans first and follow the one the
    /// others agree with most (0 = off; one model call per plan).
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=5))]
    pub best_of: u32,
    /// When two steps change nothing, diagnose why and change course once
    /// before stopping to ask.
    #[arg(long)]
    pub recover: bool,
    /// Give the model a run memo: a short scratchpad it writes and sees on
    /// every later step (docs/LONG_RUNS.md).
    #[arg(long)]
    pub memo: bool,
    /// Accept "done" only when the output the task names exists (a file,
    /// a folder shown in the file manager); docs/VERIFICATION.md.
    #[arg(long)]
    pub task_checks: bool,
}

impl Ab {
    /// The flags that turn something off, as typed on the command line.
    pub fn disabled(&self) -> Vec<&'static str> {
        [
            (self.no_batch, "--no-batch"),
            (self.no_tree_only, "--no-tree-only"),
            (self.no_a11y, "--no-a11y"),
            (self.no_plan_cache, "--no-plan-cache"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }

    /// Every flag that differs from the defaults, as typed.
    pub fn changed(&self) -> Vec<&'static str> {
        let mut v = self.disabled();
        if self.computer_tools_v1 {
            v.push("--computer-tools-v1");
        }
        if self.oai_contract.is_some() {
            v.push("--oai-contract");
        }
        if self.native_model.is_some() {
            v.push("--native-model");
        }
        if self.local_safety_checks {
            v.push("--local-safety-checks");
        }
        if self.oai_batch > 1 {
            v.push("--oai-batch");
        }
        if self.step_candidates > 1 {
            v.push("--step-candidates");
        }
        if self.step_judge == StepJudge::Model {
            v.push("--step-judge");
        }
        if self.executor {
            v.push("--executor");
        }
        if self.outcomes {
            v.push("--outcomes");
        }
        if self.macro_history {
            v.push("--macro-history");
        }
        if self.procedures {
            v.push("--procedures");
        }
        if self.best_of >= 2 {
            v.push("--best-of");
        }
        if self.recover {
            v.push("--recover");
        }
        if self.memo {
            v.push("--memo");
        }
        if self.task_checks {
            v.push("--task-checks");
        }
        v
    }
}

/// Apply the switches to an engine config. Always `Ok(true)`: every switch
/// maps to a real `Config` field (the plan cache is chosen by the caller, see
/// [`Ab::plan_cache`]).
pub fn apply_ab(config: &mut Config, ab: &Ab) -> anyhow::Result<bool> {
    config.max_batch = if ab.no_batch {
        0
    } else {
        config.max_batch.max(1)
    };
    config.tree_only = !ab.no_tree_only;
    config.a11y_actions = !ab.no_a11y;
    config.executor = ab.executor;
    config.outcomes = ab.outcomes;
    config.macro_history = ab.macro_history;
    config.procedures = ab.procedures;
    config.best_of = ab.best_of;
    config.recover = ab.recover;
    config.memo = ab.memo;
    config.task_checks = ab.task_checks;
    anyhow::ensure!(
        ab.oai_contract.is_none() || ab.native_model.is_none(),
        "--native-model and --oai-contract cannot be combined"
    );
    config.computer_tools_v1 =
        ab.computer_tools_v1 || ab.oai_contract.is_some() || ab.native_model.is_some();
    config.native_model = ab.native_model;
    config.oai_contract = match ab.oai_contract {
        Some(OaiFrame::Frame(f)) => Some(f),
        _ => None,
    };
    config.local_safety_checks = ab.local_safety_checks;
    config.oai_batch = ab.oai_batch.max(1);
    config.step_candidates = ab.step_candidates.max(1);
    config.step_judge_model = ab.step_judge == StepJudge::Model;
    Ok(true)
}

impl Ab {
    /// Whether the run may use the plan cache.
    pub fn plan_cache(&self) -> bool {
        !self.no_plan_cache
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepJudge {
    #[default]
    Cluster,
    Model,
}

/// `--oai-contract`: a coordinate frame, or the model's own from the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OaiFrame {
    Auto,
    Frame(kai_core::oai_contract::CoordFrame),
}

fn native_model(s: &str) -> Result<kai_core::native_models::NativeModel, String> {
    serde_json::from_value(serde_json::Value::String(s.to_ascii_lowercase()))
        .map_err(|_| "expected fara15, ui-tars15 or holo31".to_string())
}

fn frame(name: &str) -> Result<OaiFrame, String> {
    if name.trim().eq_ignore_ascii_case("auto") {
        return Ok(OaiFrame::Auto);
    }
    kai_core::oai_contract::CoordFrame::parse(name)
        .map(OaiFrame::Frame)
        .ok_or_else(|| {
            format!("unknown frame {name:?}: use marks, pixels, norm1000, smart-resize or auto")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oai_contract_takes_a_frame_or_auto() {
        use kai_core::oai_contract::CoordFrame;
        assert_eq!(frame("auto"), Ok(OaiFrame::Auto));
        assert_eq!(frame("norm1000"), Ok(OaiFrame::Frame(CoordFrame::Norm1000)));
        assert!(frame("guess").is_err());
        let mut c = Config::default();
        let ab = Ab {
            oai_contract: Some(OaiFrame::Frame(CoordFrame::Marks)),
            ..Ab::default()
        };
        apply_ab(&mut c, &ab).unwrap();
        assert_eq!(c.oai_contract, Some(CoordFrame::Marks));
        assert!(c.computer_tools_v1, "the contract implies the tools");
    }

    #[test]
    fn switches_map_onto_config() {
        let mut c = Config::default();
        assert!(apply_ab(&mut c, &Ab::default()).unwrap());
        assert!(c.tree_only && c.a11y_actions && c.max_batch > 0);
        let ab = Ab {
            no_batch: true,
            no_a11y: true,
            ..Default::default()
        };
        assert_eq!(ab.disabled(), vec!["--no-batch", "--no-a11y"]);
        apply_ab(&mut c, &ab).unwrap();
        assert_eq!(c.max_batch, 0);
        assert!(!c.a11y_actions && c.tree_only);
        assert!(ab.plan_cache());
        assert!(!c.executor, "executor off unless asked for");
        let ab = Ab {
            executor: true,
            ..Default::default()
        };
        apply_ab(&mut c, &ab).unwrap();
        assert!(c.executor);
        assert_eq!(ab.changed(), vec!["--executor"]);
        assert!(!c.macro_history, "macro history off unless asked for");
        let ab = Ab {
            macro_history: true,
            ..Default::default()
        };
        apply_ab(&mut c, &ab).unwrap();
        assert!(c.macro_history);
        assert_eq!(ab.changed(), vec!["--macro-history"]);
        assert!(!c.memo, "memo off unless asked for");
        let ab = Ab {
            memo: true,
            ..Default::default()
        };
        apply_ab(&mut c, &ab).unwrap();
        assert!(c.memo);
        assert_eq!(ab.changed(), vec!["--memo"]);
        let ab = Ab {
            task_checks: true,
            ..Default::default()
        };
        apply_ab(&mut c, &ab).unwrap();
        assert!(c.task_checks);
        assert_eq!(ab.changed(), vec!["--task-checks"]);
    }

    #[test]
    fn step_judge_mode_preserves_candidate_count() {
        let mut c = Config::default();
        let ab = Ab {
            step_candidates: 4,
            step_judge: StepJudge::Model,
            ..Ab::default()
        };
        apply_ab(&mut c, &ab).unwrap();
        assert_eq!(c.step_candidates, 4);
        assert!(c.step_judge_model);
        assert!(ab.changed().contains(&"--step-judge"));
    }
}
