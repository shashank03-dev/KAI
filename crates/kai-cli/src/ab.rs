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

/// Every switch defaults to "feature on"; the flags turn one off.
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
}

/// Apply the switches to an engine config. Returns whether this build's
/// `Config` actually has the switches (false: features are whatever the
/// engine does by default, and results must say so).
///
/// TODO(merge): once `Config` has the fields, set them here, e.g.
///   config.batch = !ab.no_batch;
///   config.tree_only = !ab.no_tree_only;
///   config.a11y_actions = !ab.no_a11y;
///   config.plan_cache = !ab.no_plan_cache;
/// and return `Ok(true)`.
pub fn apply_ab(config: &mut Config, ab: &Ab) -> anyhow::Result<bool> {
    let _ = config;
    let off = ab.disabled();
    anyhow::ensure!(
        off.is_empty(),
        "{} not available in this build: kai_core::Config has no such switch yet",
        off.join(", ")
    );
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_apply_and_off_switches_are_refused_until_wired() {
        let mut c = Config::default();
        assert!(!apply_ab(&mut c, &Ab::default()).unwrap());
        let ab = Ab {
            no_batch: true,
            no_a11y: true,
            ..Default::default()
        };
        assert_eq!(ab.disabled(), vec!["--no-batch", "--no-a11y"]);
        let err = apply_ab(&mut c, &ab).unwrap_err().to_string();
        assert!(err.contains("--no-batch, --no-a11y"), "{err}");
    }
}
