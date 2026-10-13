//! Feeding the [`crate::recorder::TraceRecorder`]: what one model call sent
//! and got back, attached to the step it produced.

use super::{Engine, Run, RunOptions};
use crate::client::{ClientError, ModelClient, ModelReply, ModelRequest};
use crate::ports::{EngineEvent, Observation};
use crate::reply::StepReply;
use crate::trace::{CallRecord, Outcome};
use std::time::Duration;

/// Reply budget for the one repair retry (a cut-off script needs room).
const REPAIR_MAX_TOKENS: u32 = 1536;

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
    /// After the run: finish the recording, tell the UI, and keep what a
    /// later run can reuse (the plan cache prefix, a procedure).
    pub(super) fn wrap_up(&self, r: &Run<'_>, opts: RunOptions<'_>, outcome: Outcome) {
        if let Some(rec) = opts.recorder
            && let Err(e) = rec.finish(&r.trace)
        {
            log::warn!("recording the run failed: {e}");
        }
        self.sink.emit(&EngineEvent::Finished { outcome });
        log::info!("run finished: {outcome:?}, {} steps", r.trace.steps.len());
        if outcome == Outcome::Success
            && let Some(cache) = opts.plan_cache
            && let Err(e) = cache.record(&r.trace)
        {
            log::warn!("plan cache record failed: {e}");
        }
        self.finish_procedure(r, opts, outcome);
    }

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

impl Engine<'_> {
    /// Returns the reply plus the model wall time and time-to-first-token.
    pub(super) async fn decide(
        &self,
        client: &dyn ModelClient,
        system: &str,
        user_text: &str,
        obs: &Observation,
        schema: &serde_json::Value,
        max_tokens: u32,
    ) -> anyhow::Result<Decided> {
        // A `type` with no target goes to the field that has focus.
        let focused = obs.grounding.focused_field();
        let images = &obs.images;
        let req = ModelRequest {
            system,
            user_text,
            images,
            schema,
            max_tokens,
            temperature: 0.0,
        };
        let reply = client.decide(&req).await?;

        match StepReply::parse_with_focus(&reply.raw, focused) {
            Ok(d) => Ok(Decided::new(d, &reply)),
            Err(_) if let Some(d) = salvaged(&reply.raw, focused) => Ok(Decided::new(d, &reply)),
            Err(first) => {
                // One repair retry (needed on the Gemini JSON-mode backend; a
                // no-op improvement on vLLM structured outputs).
                log::warn!(
                    "repairing malformed reply: {first}; reply began: {:?}",
                    reply.raw.chars().take(900).collect::<String>()
                );
                // A long command or text (e.g. a script) can overrun the
                // short reply budget and arrive cut off: the retry gets room.
                let hint = match &first {
                    crate::schema::SchemaError::MissingText(crate::schema::Action::Shell) => {
                        " Put the exact terminal command in `text`, e.g. \
                         {\"thought\":\"run it\",\"action\":\"shell\",\"text\":\"ls -la\"}."
                    }
                    crate::schema::SchemaError::MissingUrl => {
                        " Put the full web address in `text`, e.g. \
                         {\"thought\":\"open it\",\"action\":\"open_url\",\"text\":\"https://example.com/page\"}."
                    }
                    crate::schema::SchemaError::MissingText(_) => {
                        " Put the text to type or the app name in `text`."
                    }
                    _ => " If it held a long command or text, it was cut off: write it in full.",
                };
                let repair_text = format!(
                    "{user_text}\n\nYour previous reply was not valid: {first}.{hint} \
                     Reply with ONLY the JSON object."
                );
                let req2 = ModelRequest {
                    user_text: &repair_text,
                    max_tokens: max_tokens.saturating_mul(8).max(REPAIR_MAX_TOKENS),
                    ..req
                };
                let reply2 = client.decide(&req2).await?;
                let mut d =
                    Decided::new(StepReply::parse_with_focus(&reply2.raw, focused)?, &reply2);
                d.add_usage(&reply);
                Ok(d)
            }
        }
    }
}

/// A reply cut off inside `then`: its main decision and complete follow-ups
/// ([`crate::reply_salvage`]), with no repair call.
fn salvaged(raw: &str, focused: Option<u32>) -> Option<StepReply> {
    let fixed = crate::reply_salvage::salvage_cut_then(raw)?;
    let r = StepReply::parse_with_focus(&fixed, focused).ok()?;
    log::info!(
        "reply was cut off in `then`: kept the action and {} follow-up(s)",
        r.then.len()
    );
    Some(r)
}
