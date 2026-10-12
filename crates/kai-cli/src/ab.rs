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
    Ok(true)
}

impl Ab {
    /// Whether the run may use the plan cache.
    pub fn plan_cache(&self) -> bool {
        !self.no_plan_cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
