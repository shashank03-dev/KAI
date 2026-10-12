//! Diagnosed recovery (`Config::recover`,
//! `docs/RESEARCH_2026_10_FOUNDATIONS.md` §2.5): when two steps change
//! nothing, name the cause ([`crate::failure::diagnose_live`]) and change
//! course once, instead of stopping at once. A second stall still stops and
//! asks.

use super::{Engine, Run};
use crate::failure::{self, Category};
use serde::{Deserialize, Serialize};

/// Recoveries per run at most.
pub const MAX_RECOVERIES: u32 = 1;

/// One recovery, kept in the trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub step: u32,
    pub category: Category,
    pub diagnosis: String,
}

impl Engine<'_> {
    /// Try a diagnosed recovery. `true`: the run goes on with a new note,
    /// the careful model and a full frame; `false`: stop and ask as before.
    pub(super) fn recover(&self, r: &mut Run<'_>) -> bool {
        if !self.config.recover || r.recoveries >= MAX_RECOVERIES {
            return false;
        }
        let (category, diagnosis) = failure::diagnose_live(&r.trace);
        let advice = match category {
            Category::GroundingMiss => {
                "You acted on the wrong element. Read the element list again and pick the one \
                 whose name matches what you mean."
            }
            Category::EnvError => {
                "The action failed. Fix the cause first (open or focus the app, wait for it, \
                 correct the command) before trying again."
            }
            _ => {
                "Do not repeat it. Take a different route: a keyboard shortcut, a menu, \
                 another element, or open_app / open_url directly."
            }
        };
        r.notes.push(format!("Diagnosis: {diagnosis}. {advice}"));
        r.trace.recoveries.push(RecoveryRecord {
            step: r.step,
            category,
            diagnosis,
        });
        r.recoveries += 1;
        r.no_progress_streak = 0;
        r.careful_next = true;
        r.force_full = true;
        true
    }
}
