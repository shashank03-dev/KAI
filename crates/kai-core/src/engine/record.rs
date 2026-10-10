//! Feeding the [`crate::recorder::TraceRecorder`]: what one model call sent
//! and got back, attached to the step it produced.

use super::{Engine, Run};
use crate::client::{ClientError, ModelReply};
use crate::ports::Observation;
use crate::reply::StepReply;
use crate::trace::CallRecord;
use std::time::Duration;

/// A decoded reply plus what the trace needs from the call(s) behind it.
pub(super) struct Decided {
    pub reply: StepReply,
    /// Model wall time (both calls when a repair retry ran).
    pub total: Duration,
    pub first_token: Option<Duration>,
    /// The raw text that decoded.
    pub raw: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

impl Decided {
    pub fn new(reply: StepReply, m: &ModelReply) -> Self {
        Self {
            reply,
            total: m.total,
            first_token: m.first_token,
            raw: m.raw.clone(),
            prompt_tokens: m.prompt_tokens,
            completion_tokens: m.completion_tokens,
        }
    }

    /// Fold in an earlier call of the same step (the one that failed to parse).
    pub fn add_usage(&mut self, earlier: &ModelReply) {
        let add = |a: Option<u32>, b: Option<u32>| match (a, b) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
        self.total += earlier.total;
        self.prompt_tokens = add(self.prompt_tokens, earlier.prompt_tokens);
        self.completion_tokens = add(self.completion_tokens, earlier.completion_tokens);
    }
}

/// The `Trace::errors` line for a failed decide: a transport/status error
/// from the backend, or a reply that would not decode.
pub(super) fn error_line(step: u32, e: &anyhow::Error) -> String {
    let kind = if e.downcast_ref::<ClientError>().is_some() {
        "model error"
    } else {
        "malformed reply"
    };
    let msg: String = e.to_string().chars().take(300).collect();
    format!("{kind} at step {step}: {msg}")
}

impl Engine<'_> {
    /// When the run is recorded: write the frames sent and return the
    /// screenshot path and the call record for the step.
    pub(super) fn record_call(
        &self,
        r: &Run<'_>,
        step: u32,
        user_text: &str,
        obs: &Observation,
        d: &Decided,
        careful: bool,
    ) -> Option<(Option<String>, CallRecord)> {
        let rec = r.recorder?;
        if let Some(sc) = obs.screen {
            rec.note_screen(sc.w, sc.h);
        }
        let (shot, crop) = rec.record_frames(step, &obs.images);
        Some((
            shot,
            CallRecord {
                prompt_system_hash: r.system_hash.clone(),
                prompt_user: Some(user_text.to_string()),
                reply_raw: Some(d.raw.clone()),
                model: rec.model_for(careful),
                prompt_tokens: d.prompt_tokens,
                completion_tokens: d.completion_tokens,
                crop,
            },
        ))
    }
}
