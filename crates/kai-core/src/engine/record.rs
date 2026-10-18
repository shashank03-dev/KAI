//! Feeding the [`crate::recorder::TraceRecorder`]: what one model call sent
//! and got back, attached to the step it produced.

use super::{Engine, Run, RunOptions};
use crate::client::{ClientError, Images, ModelClient, ModelReply, ModelRequest};
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
    /// Later actions of the same turn (`--oai-batch`).
    pub queued: Vec<crate::computer::ComputerAction>,
    /// False for an action already returned in an earlier model response.
    pub model_called: bool,
    /// Captured only when tracing: the request that produced `raw` can use
    /// a native system prompt and an image different from the observation.
    pub recorded_request: Option<RecordedRequest>,
}

pub(super) struct RecordedRequest {
    system: String,
    user: String,
    images: Images,
    tool_spec: Option<serde_json::Value>,
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
            queued: Vec::new(),
            model_called: true,
            recorded_request: None,
        }
    }

    pub fn record_request(self, req: &ModelRequest<'_>, recording: bool) -> Self {
        self.record_details(req.system, req.user_text, req.images, recording)
    }

    pub fn record_details(
        mut self,
        system: &str,
        user: &str,
        images: &Images,
        recording: bool,
    ) -> Self {
        if recording {
            self.recorded_request = Some(RecordedRequest {
                system: system.to_owned(),
                user: user.to_owned(),
                images: images.clone(),
                tool_spec: None,
            });
        }
        self
    }

    pub fn record_tool_spec(mut self, tool: Option<&serde_json::Value>) -> Self {
        if let Some(request) = self.recorded_request.as_mut() {
            request.tool_spec = tool.cloned();
        }
        self
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
        if !d.model_called {
            // The action was queued by an earlier response; there is no
            // request, reply, or model-visible screenshot for this step.
            return Some((None, CallRecord::default()));
        }
        let request = d.recorded_request.as_ref();
        let (shot, crop) = rec.record_frames(step, request.map_or(&obs.images, |req| &req.images));
        Some((
            shot,
            CallRecord {
                prompt_system_hash: request
                    .map(|req| rec.record_system(&req.system))
                    .or_else(|| r.system_hash.clone()),
                prompt_user: Some(request.map_or(user_text, |req| &req.user).to_string()),
                tool_spec: request.and_then(|req| req.tool_spec.clone()),
                reply_raw: Some(d.raw.clone()),
                model: rec.model_for(careful),
                prompt_tokens: d.prompt_tokens,
                completion_tokens: d.completion_tokens,
                crop,
            },
        ))
    }
}

pub(super) struct DecisionObservation<'a> {
    pub obs: &'a Observation,
    pub id: u64,
    pub result: Option<&'a crate::computer::ToolResult>,
    pub recording: bool,
}
impl Engine<'_> {
    /// Returns the reply plus the model wall time and time-to-first-token.
    pub(super) async fn decide(
        &self,
        client: &dyn ModelClient,
        system: &str,
        user_text: &str,
        observed: DecisionObservation<'_>,
        schema: &serde_json::Value,
        max_tokens: u32,
    ) -> anyhow::Result<Decided> {
        if let Some(native) = self.config.native_model {
            anyhow::ensure!(
                self.config.oai_contract.is_none(),
                "native and OpenAI contracts are mutually exclusive"
            );
            return self
                .decide_native(client, native, user_text, observed, max_tokens)
                .await;
        }
        if let Some(frame) = self.config.oai_contract {
            return self
                .decide_oai(client, frame, user_text, observed, max_tokens)
                .await;
        }
        let obs = observed.obs;
        // A `type` with no target goes to the field that has focus.
        let focused = obs.grounding.focused_field();
        let images = &obs.images;
        let req = ModelRequest {
            observation_id: Some(observed.id),
            computer_result: observed.result,
            system,
            user_text,
            images,
            schema,
            max_tokens,
            temperature: 0.0,
        };
        let reply = self.decision_request(client, &req).await?;

        let parse = |raw: &str| {
            if self.config.computer_tools_v1 {
                StepReply::parse_computer(raw, focused)
            } else {
                StepReply::parse_with_focus(raw, focused)
            }
        };
        match parse(&reply.raw) {
            Ok(d) => Ok(Decided::new(d, &reply)),
            Err(_) if let Some(d) = salvaged(&reply.raw, |fixed| parse(fixed).ok()) => {
                Ok(Decided::new(d, &reply))
            }
            Err(first) => {
                // One repair retry (needed on the Gemini JSON-mode backend; a
                // no-op improvement on vLLM structured outputs).
                log::warn!("repairing malformed reply: {first}");
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
                    observation_id: Some(observed.id),
                    computer_result: observed.result,
                    user_text: &repair_text,
                    max_tokens: max_tokens.saturating_mul(8).max(REPAIR_MAX_TOKENS),
                    ..req
                };
                let reply2 = self.decision_request(client, &req2).await?;
                let mut d = Decided::new(parse(&reply2.raw)?, &reply2);
                d.add_usage(&reply);
                Ok(d)
            }
        }
    }

    pub(super) async fn decision_request(
        &self,
        client: &dyn ModelClient,
        req: &ModelRequest<'_>,
    ) -> anyhow::Result<ModelReply> {
        anyhow::ensure!(!self.interrupt.cancelled(), "run cancelled");
        tokio::select! {
            biased;
            () = crate::ports::wait_cancelled(self.interrupt) => anyhow::bail!("run cancelled"),
            reply = client.decide(req) => Ok(reply?),
        }
    }
}

/// A reply cut off inside `then`: its main decision and complete follow-ups
/// ([`crate::reply_salvage`]), with no repair call.
/// `parse` is the run's own reply parser (computer tools or not, review
/// CORE-ENG-06).
fn salvaged(raw: &str, parse: impl Fn(&str) -> Option<StepReply>) -> Option<StepReply> {
    let fixed = crate::reply_salvage::salvage_cut_then(raw)?;
    let r = parse(&fixed)?;
    log::info!(
        "reply was cut off in `then`: kept the action and {} follow-up(s)",
        r.then.len()
    );
    Some(r)
}
