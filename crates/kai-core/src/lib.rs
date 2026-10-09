//! Kai's model harness — backend-agnostic, OS-agnostic, and tested end to end.
//!
//! The loop ([`engine::Engine`]) runs observe → ground → decide → act → verify.
//! The screen and input are behind [`engine::Perceptor`] and [`engine::Actuator`]
//! so `kai-os` can implement them per platform and tests can fake them. The
//! model is behind [`client::ModelClient`], with vLLM, Gemini, failover and mock
//! backends. See `docs/HARNESS.md`.

pub mod client;
pub mod element;
pub mod engine;
pub mod orchestrator;
pub mod prompt;
pub mod risk;
pub mod schema;
pub mod skill;
pub mod trace;

pub use client::{
    Backend, ClientError, FailoverClient, GeminiClient, Images, MockClient, ModelClient, VllmClient,
};
pub use element::{Element, Grounding, Rect, Role};
pub use engine::{
    Actuator, ApprovalChoice, Approver, Config, Engine, EngineEvent, EventSink, Interrupt,
    Observation, ObserveTiming, Perceptor,
};
pub use risk::ApprovalLevel;
pub use schema::{Action, Decision, Direction, Risk};
pub use skill::{DeterministicStep, Skill};
pub use trace::{Outcome, Trace};
