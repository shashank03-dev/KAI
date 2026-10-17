//! Model replies used when native actions or user checks become decisions.
use crate::client::{Backend, ModelReply};
use serde_json::{Value, json};
use std::time::Instant;

pub(super) fn reply(raw: Value, started: Instant) -> ModelReply {
    ModelReply {
        backend: Backend::OpenAiNative,
        cost_usd: None,
        raw: raw.to_string(),
        total: started.elapsed(),
        first_token: None,
        prompt_tokens: None,
        completion_tokens: None,
    }
}

pub(super) fn ask(why: &str, started: Instant) -> ModelReply {
    reply(
        json!({"thought":"needs user verification","action":"ask_user","text":why,"risk":"high"}),
        started,
    )
}
