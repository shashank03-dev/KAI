//! Kai's model harness — backend-agnostic, OS-agnostic, and tested end to end.
//!
//! The loop ([`engine::Engine`]) runs observe → ground → decide → act → verify.
//! The screen and input are behind [`engine::Perceptor`] and [`engine::Actuator`]
//! so `kai-os` can implement them per platform and tests can fake them. The
//! model is behind [`client::ModelClient`], with vLLM, Gemini, failover and mock
//! backends. See `docs/HARNESS.md`.

pub mod best_of;
pub mod client;
pub mod cua;
pub mod element;
pub mod engine;
pub mod executor;
pub mod failure;
pub mod human;
pub mod ledger;
pub mod macro_history;
pub mod mcp;
pub mod memo;
pub mod memory;
pub mod models;
pub mod orchestrator;
pub mod plan;
pub mod plan_cache;
pub mod ports;
pub mod probe;
pub mod procedure;
pub mod prompt;
pub mod recorder;
pub mod reply;
pub mod risk;
pub mod schema;
pub mod settings;
pub mod shell_ro;
pub mod skill;
pub mod trace;
pub mod untrusted;
pub mod web;

pub use client::{
    Backend, ClientError, FailoverClient, GeminiClient, Images, MockClient, ModelClient, VllmClient,
};
pub use element::{Element, Grounding, Rect, Role};
pub use engine::{
    Actuator, ApprovalChoice, Approver, Config, Engine, EngineEvent, EventSink, Interrupt,
    Observation, ObserveTiming, Perceptor, Probe, RunOptions,
};
pub use failure::{Category, Failure};
pub use human::{CheckKind, HumanCheck, HumanHelper, HumanReply, OneTimeCode};
pub use ledger::{Effect, Ledger, OutcomeRecord};
pub use memory::{Lesson, MemoryStore};
pub use plan_cache::PlanCache;
pub use probe::{ProbeSpec, ProbeVerdict};
pub use procedure::{Procedure, ProcedureStore};
pub use recorder::{RunMeta, TraceRecorder};
pub use reply::StepReply;
pub use risk::ApprovalLevel;
pub use schema::{Action, Decision, Direction, Risk};
pub use skill::{DeterministicStep, Skill};
pub use trace::{Outcome, Trace};
