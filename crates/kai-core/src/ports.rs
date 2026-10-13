//! The seams between the loop and the world: what the screen gives, what the
//! hands do, who approves, how a run stops, and what the UI hears.

use crate::client::Images;
use crate::element::Grounding;
use crate::risk::ApprovalLevel;
use crate::schema::Decision;
use crate::trace::{Outcome, Timings};
use async_trait::async_trait;
use std::time::Duration;

/// What one observation produced.
pub struct Observation {
    pub images: Images,
    pub grounding: Grounding,
    /// Perceptual hash of the screen, to detect "nothing changed".
    pub screen_hash: u64,
    /// Where the observe time went (capture and tree run in parallel).
    pub timing: ObserveTiming,
    /// The captured display in global screen pixels, when known.
    pub screen: Option<crate::element::Rect>,
    /// The active window's title, from the same tree read (no extra OS
    /// call); `None` when unknown. Read by the `window_title` probe.
    pub window_title: Option<String>,
    /// The page address, when the backend knows it for certain (the browser
    /// lane reads it from the page itself). `None` on the desktop, where the
    /// `url_is` probe reads the address bar from the tree instead.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObserveTiming {
    pub capture_ms: u64,
    pub tree_ms: u64,
    pub encode_ms: u64,
}

/// Looks at the screen and grounds it (Set-of-Marks or grid).
#[async_trait]
pub trait Perceptor: Send + Sync {
    async fn observe(&self) -> anyhow::Result<Observation>;
    /// Wait for the screen to settle after an action, up to a limit.
    async fn settle(&self, _max: Duration) {}
    /// Observe without encoding a screenshot: tree only (cheap). Backends
    /// that cannot read a tree fall back to a full observation.
    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        self.observe().await
    }
    /// A zoomed JPEG of `rect` (global screen pixels), for a second look
    /// after a missed click. `None` when unsupported.
    async fn zoom(&self, _rect: crate::element::Rect) -> Option<Vec<u8>> {
        None
    }
    /// A running count of accessibility events from the active app (AT-SPI
    /// state/text/property/children/window/document events). `None` when
    /// this backend cannot see events; the effect label then falls back to
    /// the tree diff and screen hash (`docs/VERIFICATION.md`).
    fn ui_events(&self) -> Option<u64> {
        None
    }
    /// Whether [`Perceptor::page_text`] can answer (Kai's browser lane). The
    /// system prompt offers `read_page` only then, so the desktop prompt
    /// stays byte-for-byte the same.
    fn reads_pages(&self) -> bool {
        false
    }
    /// The visible page's text, for a reading step the model asked for with
    /// `read_page`: `(text cut to at most max_chars, full length in chars)`.
    /// `None` where there is no page to read.
    async fn page_text(&self, _max_chars: usize) -> Option<(String, usize)> {
        None
    }
}

/// Read-only checks for the outcome ledger's OS probes (`file_exists`,
/// `file_contains`, `shell_ro`); observation probes never reach it. It must
/// never write. Each call runs under `Config::probe_timeout`.
#[async_trait]
pub trait Probe: Send + Sync {
    async fn check(
        &self,
        probe: &crate::probe::ProbeSpec,
    ) -> anyhow::Result<crate::probe::ProbeVerdict>;
}

/// Moves the mouse/keyboard and reaches the OS for fast paths.
#[async_trait]
pub trait Actuator: Send + Sync {
    async fn act(&self, decision: &Decision, grounding: &Grounding) -> anyhow::Result<()>;
    async fn open_app(&self, app: &str) -> anyhow::Result<()>;
    async fn open_url(&self, url: &str) -> anyhow::Result<()>;
    /// Run a terminal command and return its (truncated) combined output.
    async fn shell(&self, cmd: &str) -> anyhow::Result<String>;
    /// Accessibility-first path: perform the action through the element's
    /// accessibility pattern (UIA Invoke/Value/Toggle/ExpandCollapse, AT-SPI
    /// Action/EditableText) with no mouse move. `Ok(true)` when it was done
    /// that way, `Ok(false)` when the caller should fall back to `act`.
    async fn a11y_act(&self, _decision: &Decision, _grounding: &Grounding) -> anyhow::Result<bool> {
        Ok(false)
    }
}

/// Messages the user sends to a running agent ("steer"). Drained once per
/// step and folded into the next model turn.
pub trait Steering: Send + Sync {
    fn take(&self) -> Vec<String>;
}

/// No steering (headless runs).
pub struct NoSteering;
impl Steering for NoSteering {
    fn take(&self) -> Vec<String> {
        Vec::new()
    }
}

/// A cheap text-only judge (e.g. a decision model) that estimates whether a
/// step's `expect` holds, so the loop can skip a full model call or a
/// screenshot. Returns P(expect is met) in 0..=1, or `None` if unavailable.
#[async_trait]
pub trait Verifier: Send + Sync {
    async fn check(&self, task: &str, expect: &str, state: &str) -> Option<f32>;
}

/// Approval UI (or an auto-policy in headless runs).
#[async_trait]
pub trait Approver: Send + Sync {
    /// Return true to proceed, false to skip/stop the action.
    async fn approve(&self, decision: &Decision, grounding: &Grounding) -> ApprovalChoice;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    Approve,
    Skip,
    Stop,
}

/// Lets the loop be stopped within one step (kill switch / interrupt button).
pub trait Interrupt: Send + Sync {
    fn stopped(&self) -> bool;
}

/// A no-op interrupt for headless runs.
pub struct NeverStops;
impl Interrupt for NeverStops {
    fn stopped(&self) -> bool {
        false
    }
}

/// Streams live state to the UI. No-op by default.
pub trait EventSink: Send + Sync {
    fn emit(&self, _ev: &EngineEvent) {}
}
pub struct NullSink;
impl EventSink for NullSink {}

/// Events the UI renders as they happen.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Observed {
        step: u32,
        elements: String,
        /// The marked JPEG the model sees (empty for terminal sub-agents).
        screenshot: std::sync::Arc<[u8]>,
        screen: Option<crate::element::Rect>,
    },
    Thinking {
        step: u32,
    },
    Decided {
        step: u32,
        decision: Box<Decision>,
        /// The box the action points at, in global screen pixels.
        target: Option<crate::element::Rect>,
    },
    AwaitingApproval {
        step: u32,
    },
    Acted {
        step: u32,
        timings: Timings,
    },
    /// A terminal command Kai ran and its (truncated) output.
    Output {
        step: u32,
        command: String,
        output: String,
    },
    /// The run is paused until the user gets past a CAPTCHA, gives a
    /// one-time code, approves a sign-in or answers a question
    /// (`docs/HUMAN_CHECKS.md`).
    NeedsHuman {
        step: u32,
        check: crate::human::HumanCheck,
    },
    /// The user answered; `result` is the [`crate::human::HumanRecord`]
    /// result (`resolved`, `answered`, `code_typed`, …).
    HumanDone {
        step: u32,
        kind: crate::human::CheckKind,
        result: String,
    },
    /// The outcome ledger changed (set at the first call, or a status
    /// moved). Strings are already redacted.
    Outcomes {
        step: u32,
        outcomes: Vec<crate::ledger::OutcomeRecord>,
    },
    /// Kai refused the model's `done`: "not done: <outcome>: <probe>
    /// failed (<reason>)".
    Gated {
        step: u32,
        reason: String,
    },
    /// A user message was folded into this step.
    Steered {
        step: u32,
        message: String,
    },
    Finished {
        outcome: Outcome,
    },
}

/// Run limits and policy.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub max_steps: u32,
    pub max_time: Duration,
    pub approval: ApprovalLevel,
    pub max_tokens: u32,
    /// Stop and ask after this many no-progress / failed steps in a row.
    pub max_failures: u32,
    /// Tree-only steps: when the Set-of-Marks list is rich and the last
    /// `expect` is checkable from text, observe without a screenshot.
    pub tree_only: bool,
    /// Force a full (screenshot) observation at least every this many steps.
    pub full_every: u32,
    /// Max follow-up actions (`then`) run without a model call; 0 disables.
    pub max_batch: u32,
    /// Try the element's accessibility pattern before the mouse.
    pub a11y_actions: bool,
    /// Let a local decision model (jevos) carry out the model's `plan`
    /// (needs `RunOptions::executor`; see `docs/EXECUTOR.md`). Off until
    /// measured on real tasks.
    pub executor: bool,
    /// The executor's confidence gates and limits.
    pub executor_gates: crate::executor::Gates,
    /// Experience memory: learn lessons from this run and credit the ones
    /// shown (needs `RunOptions::memory`; `docs/MEMORY.md`). Recall of
    /// stored lessons happens whenever a store is passed.
    pub learn: bool,
    /// After a failed or recovered run, ask the model once for up to two
    /// lessons. Off by default: it costs one extra call per such run.
    pub reflect: bool,
    /// How long a human check waits for the user before the run ends as
    /// "asked user". The wait does not count against `max_time`.
    pub human_wait: Duration,
    /// Ask for required outcomes at the first model call and accept `done`
    /// only when every probed one holds (`docs/VERIFICATION.md`). Off until
    /// measured on a live model.
    pub outcomes: bool,
    /// Longest one outcome probe may run; a timeout counts as failed.
    pub probe_timeout: Duration,
    /// Fold steps older than the last 2 into a short running summary
    /// (`macro_history.rs`). Off until measured on a live model.
    pub macro_history: bool,
    /// Replay stored procedures for known tasks and record verified runs
    /// (needs `RunOptions::procedures`; `docs/PROCEDURES.md`). Off until
    /// measured on repeated live tasks.
    pub procedures: bool,
    /// For long tasks, sample this many plans before the first action and
    /// follow the one the others agree with most (`best_of.rs`). 0 or 1:
    /// off. Each plan is one model call (made in parallel).
    pub best_of: u32,
    /// When two steps change nothing, diagnose why and change course once
    /// before stopping to ask (`engine/recover.rs`). Off until measured.
    pub recover: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_steps: 40,
            max_time: Duration::from_secs(600),
            approval: ApprovalLevel::HighRiskOnly,
            max_tokens: 192,
            max_failures: 3,
            tree_only: true,
            full_every: 3,
            max_batch: crate::reply::MAX_THEN as u32,
            a11y_actions: true,
            executor: false,
            executor_gates: crate::executor::Gates::default(),
            learn: true,
            reflect: false,
            human_wait: Duration::from_secs(600),
            outcomes: false,
            probe_timeout: Duration::from_secs(2),
            macro_history: false,
            procedures: false,
            best_of: 0,
            recover: false,
        }
    }
}
