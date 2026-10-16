//! Network adaptation only. All execution belongs to the shared Engine.
use super::turn::{self, Shot};
use super::{CuaConfig, jpeg_size};
use crate::client::{
    Backend, ClientError, ModelClient, ModelReply, ModelRequest, http_client, send_error,
};
use crate::cua_protocol::{self, SafetyCheck};
use crate::ports::{ApprovalChoice, Approver};
use crate::{Decision, Perceptor, computer::*};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::{collections::VecDeque, time::Instant};
use tokio::sync::Mutex;
/// Harness notes per run before a stopped batch goes to the user instead.
const MAX_NOTES: u32 = 2;
struct State {
    previous: Option<String>,
    /// Calls answered next turn, each with the safety checks the user accepted.
    calls: Vec<(String, Vec<SafetyCheck>)>,
    actions: VecDeque<ComputerAction>,
    /// The current batch's size and how many of it were handed out.
    batch: usize,
    dispatched: usize,
    notes: u32,
    message: Option<String>,
    error: Option<String>,
    image: Option<(u32, u32)>,
    origin_tree: Option<crate::Grounding>,
}
pub(super) struct Provider<'a> {
    caps: Capabilities,
    config: &'a CuaConfig,
    task: &'a str,
    eyes: &'a dyn Perceptor,
    approver: &'a dyn Approver,
    key: String,
    http: reqwest::Client,
    state: Mutex<State>,
}
impl<'a> Provider<'a> {
    pub fn new(
        config: &'a CuaConfig,
        task: &'a str,
        eyes: &'a dyn Perceptor,
        approver: &'a dyn Approver,
        caps: Capabilities,
    ) -> Result<Self, ClientError> {
        let key = config
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
            .ok_or(ClientError::MissingKey("OPENAI_API_KEY"))?;
        Ok(Self {
            caps,
            config,
            task,
            eyes,
            approver,
            key,
            http: http_client(),
            state: Mutex::new(State {
                previous: None,
                calls: vec![],
                actions: VecDeque::new(),
                batch: 0,
                dispatched: 0,
                notes: 0,
                message: None,
                error: None,
                image: None,
                origin_tree: None,
            }),
        })
    }
    pub async fn finish(&self) -> (Option<String>, Option<String>) {
        let s = self.state.lock().await;
        (s.message.clone(), s.error.clone())
    }
    async fn post(&self, body: &Value) -> Result<Value, ClientError> {
        let response = self
            .http
            .post(format!(
                "{}/responses",
                self.config.base.trim_end_matches('/')
            ))
            .bearer_auth(&self.key)
            .json(body)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| send_error(e, self.config.timeout))?;
        let status = response.status();
        let text = response.text().await.map_err(|_| ClientError::EmptyBody)?;
        if !status.is_success() {
            return Err(ClientError::from_status(status.as_u16(), &text));
        }
        serde_json::from_str(&text).map_err(|_| ClientError::EmptyBody)
    }
}
fn reply(raw: Value, started: Instant) -> ModelReply {
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
fn ask(why: &str, started: Instant) -> ModelReply {
    reply(
        json!({"thought":"needs user verification","action":"ask_user","text":why,"risk":"high"}),
        started,
    )
}
#[async_trait]
impl ModelClient for Provider<'_> {
    fn backend(&self) -> Backend {
        Backend::OpenAiNative
    }
    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let started = Instant::now();
        let mut state = self.state.lock().await;
        let Some(observation) = req.observation_id else {
            return Ok(ask("Missing host observation identity", started));
        };
        let Some(seen) = jpeg_size(&req.images.screenshot_jpeg) else {
            return Ok(ask(
                "Computer use needs a safe screenshot; please continue manually",
                started,
            ));
        };
        if state.image.is_some_and(|previous| previous != seen) && !state.actions.is_empty() {
            return Ok(ask(
                "Display dimensions changed during the provider batch",
                started,
            ));
        }
        let mut note = None;
        if !state.actions.is_empty() {
            let fresh = self
                .eyes
                .observe_tree_only()
                .await
                .map_err(|_| ClientError::EmptyBody)?;
            let failed = req.computer_result.is_none_or(|r| r.error.is_some());
            // Pixels always differ after an action (a caret, a hover), so
            // the layout decides: the next queued action still has its place.
            let moved = !state
                .origin_tree
                .as_ref()
                .is_some_and(|t| crate::engine::fresh::same_layout(t, &fresh.grounding));
            if failed || moved {
                let why = if failed {
                    "the last one was skipped or failed"
                } else {
                    "the screen layout changed"
                };
                let ran = state.dispatched - usize::from(failed);
                state.actions.clear();
                if state.notes >= MAX_NOTES {
                    return Ok(ask(&format!("Provider batch stopped: {why}"), started));
                }
                state.notes += 1;
                note = Some(turn::stopped_note(ran, state.batch, why));
            }
        }
        if state.actions.is_empty()
            && let Some(reply) = self
                .next_batch(&mut state, req, observation, seen, note)
                .await?
        {
            return Ok(reply);
        }
        let mut action = state.actions.pop_front().expect("nonempty validated queue");
        state.dispatched += 1;
        // Each ordered action receives a fresh observation and another host
        // policy check. Coordinates are preserved; dimensions cannot change.
        let refresh = |target: &mut Target| match target {
            Target::Point {
                observation: id, ..
            }
            | Target::Element {
                observation: id, ..
            } => *id = observation,
        };
        match &mut action {
            ComputerAction::Click { target, .. }
            | ComputerAction::Move { target, .. }
            | ComputerAction::Scroll { target, .. } => refresh(target),
            ComputerAction::Drag { points, .. } => points.iter_mut().for_each(refresh),
            ComputerAction::Type { target, .. } => {
                let fresh = self
                    .eyes
                    .observe_tree_only()
                    .await
                    .map_err(|_| ClientError::EmptyBody)?;
                let Some(mark) = fresh.grounding.focused_field() else {
                    return Ok(ask("No confirmed focused editable field", started));
                };
                *target = Target::Element { observation, mark };
            }
            _ => {}
        }
        Ok(reply(
            json!({"thought":"provider computer action","action":"computer","computer":action,"risk":"high"}),
            started,
        ))
    }
}

impl Provider<'_> {
    /// Ask the provider for the next batch and queue it, or return the
    /// reply that ends the turn (a question, a refusal, the final answer).
    async fn next_batch(
        &self,
        state: &mut State,
        req: &ModelRequest<'_>,
        observation: u64,
        seen: (u32, u32),
        note: Option<String>,
    ) -> Result<Option<ModelReply>, ClientError> {
        let started = Instant::now();
        let native = self.eyes.native_frame().await;
        let shot = match &native {
            Some(f) => Shot {
                image: &f.image,
                mime: f.mime,
                size: (f.width, f.height),
            },
            None => Shot {
                image: &req.images.screenshot_jpeg,
                mime: "image/jpeg",
                size: seen,
            },
        };
        let ga = self.config.computer_tools_v1;
        let input = match state.previous {
            Some(_) => turn::next_input(&state.calls, &shot, !ga, note.as_deref()),
            None => turn::first_input(self.task, &shot),
        };
        let body = turn::body(
            &self.config.model,
            input,
            state.previous.as_deref(),
            ga,
            shot.size,
            &self.config.environment,
        );
        let response = match self.post(&body).await {
            Ok(v) => v,
            Err(e) => {
                state.error = Some(e.to_string());
                return Err(e);
            }
        };
        let decoded = match cua_protocol::parse_response(&response, observation, Some(1)) {
            Ok(v) => v,
            Err(e) => {
                state.error = Some(e.to_string());
                state.message = Some(e.to_string());
                return Ok(Some(ask(&e.to_string(), started)));
            }
        };
        let Some(id) = response["id"].as_str().filter(|id| !id.is_empty()) else {
            state.error = Some("Missing response id".into());
            return Ok(Some(ask("Malformed provider response", started)));
        };
        state.previous = Some(id.into());
        state.image = Some(seen);
        state.message = decoded.message.map(|(text, _)| text);
        let origin = self
            .eyes
            .observe_tree_only()
            .await
            .map_err(|_| ClientError::EmptyBody)?;
        for call in &decoded.calls {
            if call.pending.is_empty() {
                continue;
            }
            let flagged = turn::safety_prompt(&call.pending, call.actions.len());
            let d = Decision::parse_checked(
                &json!({"thought":format!("{flagged} Allow them?"),"action":"wait","risk":"high"})
                    .to_string(),
            )
            .map_err(|_| ClientError::EmptyBody)?;
            if self
                .approver
                .approve_safety_check(&d, &origin.grounding)
                .await
                != ApprovalChoice::Approve
            {
                let why = format!("{flagged} Not accepted, so nothing ran.");
                state.message = Some(why.clone());
                return Ok(Some(ask(&why, started)));
            }
        }
        let space = ObservationSpace {
            session: 1,
            id: observation,
            image_width: seen.0,
            image_height: seen.1,
            region: origin.screen.unwrap_or(crate::Rect::new(0., 0., 0., 0.)),
        };
        let mut queued = Vec::new();
        for call in &decoded.calls {
            for action in &call.actions {
                let mut action = action.clone();
                crate::oai_contract::rescale(&mut action, shot.size, seen);
                queued.push(action);
            }
        }
        let requests: Vec<ToolRequest> = queued
            .iter()
            .enumerate()
            .map(|(i, action)| ToolRequest {
                version: 1,
                id: i as u64,
                session: 1,
                timeout_ms: 30_000,
                action: action.clone(),
            })
            .collect();
        if !requests.is_empty()
            && let Err(e) = validate_batch(&requests, &self.caps, &space)
        {
            state.error = Some(e.to_string());
            return Ok(Some(ask(&e.to_string(), started)));
        }
        state.origin_tree = Some(origin.grounding);
        // The user accepted every flagged call above; the next turn says so.
        state.calls = decoded
            .calls
            .into_iter()
            .map(|c| (c.call_id, c.pending))
            .collect();
        state.batch = queued.len();
        state.dispatched = 0;
        state.actions.extend(queued);
        if state.actions.is_empty() {
            let why = state
                .message
                .as_deref()
                .unwrap_or("The provider returned no actions; verify the task");
            return Ok(Some(ask(why, started)));
        }
        Ok(None)
    }
}
