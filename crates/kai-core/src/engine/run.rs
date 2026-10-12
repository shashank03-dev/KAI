//! The mutable state of one run, and what the last action expects.

use super::{RunOptions, gate, human, memory};
use crate::executor::Decider;
use crate::memory::MemoryStore;
use crate::ports::Probe;
use crate::prompt::HistoryItem;
use crate::recorder::TraceRecorder;
use crate::schema::Action;
use crate::trace::Trace;

/// What the last action expected, waiting for the next observation to check.
pub(super) struct Pending {
    pub(super) summary: String,
    pub(super) action: Action,
    pub(super) expect: String,
    /// Text typed by this step (or batch), to look for in a tree-only read.
    pub(super) typed: Option<String>,
    /// Keys pressed last; Enter usually submits/navigates, so look properly.
    pub(super) keys: Vec<String>,
    /// The last click hit a dropdown/menu: a popup needs a real look.
    pub(super) popup: bool,
    /// Indices into `trace.steps` this check confirms.
    pub(super) records: Vec<usize>,
}

/// Mutable state of one run.
pub(super) struct Run<'t> {
    pub(super) task: &'t str,
    pub(super) trace: Trace,
    pub(super) history: Vec<HistoryItem>,
    pub(super) step: u32,
    /// (screen hash, element-list hash, was tree-only) of the last observation.
    pub(super) last_sig: Option<(u64, u64, bool)>,
    pub(super) no_progress_streak: u32,
    pub(super) failure_streak: u32,
    /// The next observation must include a screenshot.
    pub(super) force_full: bool,
    /// Use the careful model for the next decision.
    pub(super) careful_next: bool,
    pub(super) tree_only_streak: u32,
    /// The last observation had a rich Set-of-Marks list.
    pub(super) last_rich: bool,
    pub(super) pending: Option<Pending>,
    /// One-shot lines for the next user turn.
    pub(super) notes: Vec<String>,
    /// A grid click (cell number, screen rect) that may have missed.
    pub(super) grid_click: Option<(u32, crate::element::Rect)>,
    /// URLs already opened in this run (normalised), to refuse a reload that
    /// would wipe what was typed.
    pub(super) opened: Vec<String>,
    /// Where this run is recorded, if anywhere, and the system prompt hash.
    pub(super) recorder: Option<&'t TraceRecorder>,
    pub(super) system_hash: Option<String>,
    /// The executor's decision model, while it is usable this run.
    pub(super) decider: Option<&'t dyn Decider>,
    /// Escalations in a row, and model steps left before plans run again.
    pub(super) exec_streak: u32,
    pub(super) exec_cooldown: u32,
    /// Experience memory, the apps/sites seen so far and the lesson block.
    pub(super) memory: Option<&'t MemoryStore>,
    pub(super) mem: memory::Recalled,
    /// Human checks this run: handled ones, codes typed, time waited.
    pub(super) human: human::HumanState,
    /// Required outcomes, gate refusals, and the OS probe port.
    pub(super) ledger: gate::LedgerState,
    pub(super) probe: Option<&'t dyn Probe>,
    /// Accessibility-event count taken just before the last action.
    pub(super) events_mark: Option<u64>,
    /// The shape of the procedure this run replayed, if any.
    pub(super) procedure: Option<String>,
    /// The best-of-N plan, shown in every turn (`crate::best_of`).
    pub(super) plan_note: Option<String>,
    /// Diagnosed recoveries so far (`Config::recover`).
    pub(super) recoveries: u32,
}

impl<'t> Run<'t> {
    /// A fresh run of `task`. `decider`: the executor, when it is on.
    pub(super) fn new(
        task: &'t str,
        opts: &RunOptions<'t>,
        decider: Option<&'t dyn Decider>,
    ) -> Self {
        Run {
            task,
            trace: Trace::new(task),
            history: Vec::new(),
            step: 0,
            last_sig: None,
            no_progress_streak: 0,
            failure_streak: 0,
            force_full: true,
            careful_next: false,
            tree_only_streak: 0,
            last_rich: false,
            pending: None,
            opened: Vec::new(),
            notes: Vec::new(),
            grid_click: None,
            recorder: opts.recorder,
            system_hash: None,
            decider,
            exec_streak: 0,
            exec_cooldown: 0,
            memory: opts.memory,
            mem: Default::default(),
            human: Default::default(),
            ledger: Default::default(),
            probe: opts.probe,
            events_mark: None,
            procedure: None,
            plan_note: None,
            recoveries: 0,
        }
    }
}
