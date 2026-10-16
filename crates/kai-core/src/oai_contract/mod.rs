//! OpenAI's computer-tool contract for open models: they read the same
//! action vocabulary as OpenAI's models, write the same action objects, and
//! go through the same parser, validation, approvals and dispatch. Off by
//! default (`--oai-contract <frame>`); see
//! `docs/RESEARCH_2026_10_OPENAI_PARITY.md` §5 and §7 Phase 2.
mod frame;
mod prompt;
mod reply;
#[cfg(test)]
mod tests;
pub use frame::{CoordFrame, rescale, smart_resize};
pub use prompt::system;
pub use reply::{Turn, decode, schema};
