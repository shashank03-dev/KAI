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
    }
}
